//! Python's incremental UTF-8 TextIOWrapper reads ahead before yielding a line.
//! Preserve that error boundary as well as universal newline translation.
use crate::tombstone_source::{Error, Result};
use std::{fs::File, io::Read};

pub(super) struct Lines {
    file: File,
    decoded: String,
    tail: Vec<u8>,
    pending_cr: bool,
    eof: bool,
}
impl Lines {
    pub(super) fn new(file: File) -> Self {
        Self {
            file,
            decoded: String::new(),
            tail: Vec::new(),
            pending_cr: false,
            eof: false,
        }
    }
    pub(super) fn next(
        &mut self,
        mut check_control: impl FnMut() -> Result<()>,
    ) -> Result<Option<String>> {
        loop {
            check_control()?;
            if let Some(end) = self.decoded.find('\n') {
                return Ok(Some(self.decoded.drain(..=end).collect()));
            }
            if self.eof {
                return if self.decoded.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(std::mem::take(&mut self.decoded)))
                };
            }
            let mut chunk = [0; 8192];
            let count = self.file.read(&mut chunk).map_err(Error::io)?;
            self.tail.extend_from_slice(&chunk[..count]);
            let (text, incomplete) = match std::str::from_utf8(&self.tail) {
                Ok(text) => (text.to_owned(), Vec::new()),
                Err(error) if error.error_len().is_none() && count != 0 => (
                    std::str::from_utf8(&self.tail[..error.valid_up_to()])
                        .expect("valid prefix")
                        .to_owned(),
                    self.tail[error.valid_up_to()..].to_vec(),
                ),
                Err(error) => return Err(Error::new("UnicodeDecodeError", error.to_string())),
            };
            self.tail = incomplete;
            for ch in text.chars() {
                if self.pending_cr {
                    self.decoded.push('\n');
                    self.pending_cr = false;
                    if ch == '\n' {
                        continue;
                    }
                }
                if ch == '\r' {
                    self.pending_cr = true;
                } else {
                    self.decoded.push(ch);
                }
            }
            if count == 0 {
                self.eof = true;
                if self.pending_cr {
                    self.decoded.push('\n');
                    self.pending_cr = false;
                }
            }
        }
    }
}
