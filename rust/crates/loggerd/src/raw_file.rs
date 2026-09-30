use std::{ffi::CString, io, os::unix::ffi::OsStrExt, path::Path, ptr::NonNull};

pub struct RawFile(NonNull<libc::FILE>);

impl RawFile {
    #[expect(unsafe_code, reason = "preserve source libc buffered file semantics")]
    pub fn create(path: &Path) -> io::Result<Self> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        loop {
            // SAFETY: both strings are terminated and live through fopen; a successful
            // FILE allocation transfers exclusively to this non-Send/non-Sync owner.
            if let Some(file) = NonNull::new(unsafe { libc::fopen(path.as_ptr(), c"wb".as_ptr()) })
            {
                return Ok(Self(file));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    #[expect(
        unsafe_code,
        reason = "retain source safe_fwrite buffering and EINTR behavior"
    )]
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < bytes.len() {
            let remaining = &bytes[written..];
            // SAFETY: the live FILE is exclusively borrowed and remaining supplies
            // exactly its advertised readable bytes for the duration of fwrite.
            let count = unsafe {
                libc::fwrite(
                    remaining.as_ptr().cast(),
                    1,
                    remaining.len(),
                    self.0.as_ptr(),
                )
            };
            if count == 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
            written += count;
        }
        Ok(())
    }

    #[expect(
        unsafe_code,
        reason = "source ZstdFileWriter ignores fflush but checks fclose"
    )]
    pub fn finish_checked(self) -> io::Result<()> {
        let this = std::mem::ManuallyDrop::new(self);
        loop {
            // SAFETY: this exclusively owns the live FILE; fflush does not consume it.
            if unsafe { libc::fflush(this.0.as_ptr()) } == 0
                || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
            {
                break;
            }
        }
        // SAFETY: ManuallyDrop suppresses Drop; fclose consumes the unique FILE once,
        // even on error. No pointer access occurs after this call and close is not retried.
        if unsafe { libc::fclose(this.0.as_ptr()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[expect(
        unsafe_code,
        reason = "source raw VideoWriter ignores flush/close errors"
    )]
    pub fn finish(self) {
        loop {
            // SAFETY: self still exclusively owns the live FILE; fflush retains ownership.
            if unsafe { libc::fflush(self.0.as_ptr()) } == 0
                || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
            {
                break;
            }
        }
    }
}

impl Drop for RawFile {
    #[expect(unsafe_code, reason = "release the unique libc FILE exactly once")]
    fn drop(&mut self) {
        // SAFETY: this owner is neither copied nor shared; fclose consumes its
        // FILE allocation once, including when closing reports an I/O failure.
        unsafe { libc::fclose(self.0.as_ptr()) };
    }
}

impl io::Write for RawFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        RawFile::write(self, bytes)?;
        Ok(bytes.len())
    }

    #[expect(unsafe_code, reason = "flush uniquely owned stdio stream for Write")]
    fn flush(&mut self) -> io::Result<()> {
        loop {
            // SAFETY: exclusive borrow guarantees a live, uniquely owned FILE.
            if unsafe { libc::fflush(self.0.as_ptr()) } == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
