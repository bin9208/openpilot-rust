use super::Error;
use std::ffi::CString;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::os::fd::{FromRawFd, OwnedFd};

pub(super) fn read_file(path: &str) -> Result<Vec<u8>, Error> {
    let path = CString::new(path).map_err(|_| Error::Path)?;
    // SAFETY: FFI: path stays NUL-terminated through this one source-equivalent readonly open.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY) };
    if fd < 0 {
        return Ok(Vec::new());
    }
    // SAFETY: FD ownership: this successful open created the sole descriptor owner.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    Ok(read_contents(&mut File::from(owned)))
}

fn read_contents(input: &mut (impl Read + Seek)) -> Vec<u8> {
    if let Ok(size) = input.seek(SeekFrom::End(0)) {
        if size > 0 && size < i64::MAX as u64 && input.seek(SeekFrom::Start(0)).is_ok() {
            let mut bytes = vec![0; size as usize];
            let mut offset = 0;
            loop {
                match input.read(&mut bytes[offset..]) {
                    Ok(count) => {
                        offset += count;
                        if count == 0 || offset == bytes.len() {
                            bytes.truncate(offset);
                            return bytes;
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        }
    }
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        match input.read(&mut chunk) {
            Ok(0) => return bytes,
            Ok(count) => bytes.extend_from_slice(&chunk[..count]),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Cursor;

    struct Reader {
        bytes: Cursor<Vec<u8>>,
        failures: VecDeque<Option<i32>>,
    }
    impl Read for Reader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            match self.failures.pop_front().flatten() {
                Some(errno) => Err(std::io::Error::from_raw_os_error(errno)),
                None => self.bytes.read(output),
            }
        }
    }
    impl Seek for Reader {
        fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
            self.bytes.seek(from)
        }
    }

    #[test]
    fn sized_read_error_falls_back_without_reopening_or_rewinding() {
        for errno in [5, 11] {
            let mut reader = Reader {
                bytes: Cursor::new(b"cam-isp-extra".to_vec()),
                failures: VecDeque::from([Some(errno)]),
            };
            assert_eq!(read_contents(&mut reader), b"cam-isp-extra");
        }
    }

    #[test]
    fn repeated_read_error_returns_empty_name() {
        let mut reader = Reader {
            bytes: Cursor::new(b"cam-isp".to_vec()),
            failures: VecDeque::from([Some(5), Some(5)]),
        };
        assert!(read_contents(&mut reader).is_empty());
    }
}
