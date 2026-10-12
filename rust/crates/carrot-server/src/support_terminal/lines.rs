use crate::Error;
use std::{fs::File, os::fd::AsFd};
use tokio::io::unix::AsyncFd;

pub(super) struct Lines {
    fd: AsyncFd<File>,
    buffer: Vec<u8>,
}
impl Lines {
    pub fn new<T: Into<std::os::fd::OwnedFd>>(pipe: T) -> Result<Self, Error> {
        let fd = File::from(pipe.into());
        let flags = rustix::fs::fcntl_getfl(&fd).map_err(std::io::Error::from)?;
        rustix::fs::fcntl_setfl(&fd, flags | rustix::fs::OFlags::NONBLOCK)
            .map_err(std::io::Error::from)?;
        Ok(Self {
            fd: AsyncFd::new(fd)?,
            buffer: Vec::new(),
        })
    }
    pub async fn next(&mut self) -> Result<Option<String>, Error> {
        loop {
            if let Some(index) = self.buffer.iter().position(|byte| *byte == b'\n') {
                if index > 65536 {
                    return Err(Error::Source(
                        "Separator is found, but chunk is longer than limit".into(),
                    ));
                }
                let line: Vec<_> = self.buffer.drain(..=index).collect();
                return Ok(Some(
                    String::from_utf8_lossy(&line)
                        .trim_end_matches(crate::tools::text::whitespace)
                        .into(),
                ));
            }
            if self.buffer.len() > 65536 {
                return Err(Error::Source(
                    "Separator is not found, and chunk exceed the limit".into(),
                ));
            }
            let mut bytes = [0; 8192];
            let mut ready = self.fd.readable().await?;
            match ready.try_io(|inner| {
                rustix::io::read(inner.get_ref().as_fd(), &mut bytes).map_err(std::io::Error::from)
            }) {
                Ok(Ok(0)) => {
                    return if self.buffer.is_empty() {
                        Ok(None)
                    } else {
                        let line = std::mem::take(&mut self.buffer);
                        Ok(Some(
                            String::from_utf8_lossy(&line)
                                .trim_end_matches(crate::tools::text::whitespace)
                                .into(),
                        ))
                    }
                }
                Ok(Ok(count)) => self.buffer.extend_from_slice(&bytes[..count]),
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => {}
            }
        }
    }
}
