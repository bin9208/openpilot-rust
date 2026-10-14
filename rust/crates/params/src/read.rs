use std::io::{self, Read};

pub(super) fn value(file: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut small = [0; 256];
    loop {
        let count = match file.read(&mut small) {
            Ok(0) => return Ok(bytes),
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        bytes
            .try_reserve(count)
            .map_err(|error| io::Error::new(io::ErrorKind::OutOfMemory, error))?;
        bytes.extend_from_slice(&small[..count]);
        if count == small.len() {
            // File::read_to_end sizes large values; small values avoid its metadata syscalls.
            file.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::value;
    use std::io::{self, Read};

    struct Pieces(std::vec::IntoIter<io::Result<&'static [u8]>>);

    impl Read for Pieces {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let piece = self.0.next().unwrap_or(Ok(b""))?;
            assert!(piece.len() <= output.len());
            output[..piece.len()].copy_from_slice(piece);
            Ok(piece.len())
        }
    }

    #[test]
    fn interrupted_and_short_reads_continue_until_eof() {
        let mut file = Pieces(
            vec![
                Err(io::ErrorKind::Interrupted.into()),
                Ok(b"a".as_slice()),
                Err(io::ErrorKind::Interrupted.into()),
                Ok(b"\0\xff".as_slice()),
            ]
            .into_iter(),
        );

        let actual = value(&mut file).unwrap();

        assert_eq!(actual, b"a\0\xff");
    }

    #[test]
    fn an_error_after_a_short_read_does_not_return_partial_data() {
        let mut file =
            Pieces(vec![Ok(b"a".as_slice()), Err(io::ErrorKind::Other.into())].into_iter());

        let error = value(&mut file).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
    }
}
