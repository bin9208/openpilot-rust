use super::{
    stream::Stream,
    transport::{Body, Request},
    WebDav, WebDavError,
};
use crate::{Error, Observer, Progress};
use std::{fs::File, path::Path};

impl WebDav {
    pub(super) fn collections(
        &self,
        parts: &[&str],
        observer: &mut Observer<'_>,
    ) -> Result<(), Error> {
        // Validate the entire destination before any request is sent.
        self.url(parts, true)?;
        for end in 1..=parts.len() {
            observer.check()?;
            let response = self.request(
                Request {
                    operation: "MKCOL",
                    url: &self.url(&parts[..end], true)?,
                    headers: &[],
                },
                Body::Empty,
            )?;
            Self::status("MKCOL", &response, &[201, 405])?;
        }
        Ok(())
    }

    pub(super) fn put(
        &self,
        parts: &[&str],
        path: &Path,
        observer: &mut Observer<'_>,
    ) -> Result<(), Error> {
        let filename = *parts.last().ok_or(WebDavError::Path)?;
        let final_url = self.url(parts, false)?;
        let mut temporary_parts = parts.to_vec();
        let temporary_name = format!(".{filename}.upload-{:016x}", rand::random::<u64>());
        if let Some(last) = temporary_parts.last_mut() {
            *last = &temporary_name;
        }
        let temporary_url = self.url(&temporary_parts, false)?;
        let mut last_error = WebDavError::File.into();
        for attempt in 0..2 {
            observer.check()?;
            let file = File::open(path).map_err(|_| WebDavError::File)?;
            let metadata = file.metadata().map_err(|_| WebDavError::File)?;
            if !metadata.is_file() {
                return Err(WebDavError::File.into());
            }
            let size = metadata.len();
            let modified = metadata.modified().map_err(|_| WebDavError::File)?;
            let mut stream = Stream {
                file,
                observer,
                filename,
                size,
                sent: 0,
                error: None,
            };
            stream.observer.progress(Progress {
                filename,
                sent: 0,
                size,
                chunk: 0,
            })?;
            let mut cleanup = Partial {
                client: self,
                url: &temporary_url,
                committed: false,
            };
            let response = self.request(
                Request {
                    operation: "PUT",
                    url: &temporary_url,
                    headers: &[("Content-Type", "application/octet-stream")],
                },
                Body::Stream {
                    reader: &mut stream,
                    size,
                },
            );
            let result = if let Some(error) = stream.error.take() {
                Err(error)
            } else {
                response
                    .and_then(|response| {
                        Self::status("PUT", &response, &[200, 201, 204])?;
                        if stream.sent != size {
                            return Err(WebDavError::File);
                        }
                        let current = stream.file.metadata().map_err(|_| WebDavError::File)?;
                        if current.len() != size
                            || current.modified().map_err(|_| WebDavError::File)? != modified
                        {
                            return Err(WebDavError::File);
                        }
                        Ok(())
                    })
                    .map_err(Error::from)
            }
            .and_then(|()| {
                stream.observer.check()?;
                let response = self.request(
                    Request {
                        operation: "MOVE",
                        url: &temporary_url,
                        headers: &[("Destination", final_url.as_str()), ("Overwrite", "T")],
                    },
                    Body::Empty,
                )?;
                Self::status("MOVE", &response, &[201, 204])?;
                Ok(())
            });
            match result {
                Ok(()) => {
                    cleanup.committed = true;
                    return Ok(());
                }
                Err(error) => {
                    cleanup.remove()?;
                    let retry = matches!(
                        error,
                        Error::WebDav(
                            WebDavError::Transport
                                | WebDavError::Status {
                                    status: 500..=599,
                                    ..
                                }
                        )
                    );
                    last_error = error;
                    if !retry || attempt == 1 {
                        break;
                    }
                }
            }
        }
        Err(last_error)
    }
}

struct Partial<'a> {
    client: &'a WebDav,
    url: &'a url::Url,
    committed: bool,
}
impl Partial<'_> {
    fn remove(&mut self) -> Result<(), WebDavError> {
        self.committed = true;
        let response = self
            .client
            .request(
                Request {
                    operation: "DELETE",
                    url: self.url,
                    headers: &[],
                },
                Body::Empty,
            )
            .map_err(|_| WebDavError::Cleanup)?;
        WebDav::status("DELETE", &response, &[200, 204, 404]).map_err(|_| WebDavError::Cleanup)
    }
}
impl Drop for Partial<'_> {
    fn drop(&mut self) {
        if !self.committed {
            if let Err(error) = self.remove() {
                eprintln!("{error}");
            }
        }
    }
}
