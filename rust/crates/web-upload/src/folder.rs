use crate::{
    api_url,
    compatibility::{or_empty, remote_size, truncate, truth},
    http::{auth, Body, Client, Request},
    Error,
};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, Read},
    path::Path,
};
pub const CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct Progress<'a> {
    pub filename: &'a str,
    pub sent: u64,
    pub size: u64,
    pub chunk: usize,
}
pub type ProgressCallback<'a> = dyn FnMut(Progress<'_>) -> Result<(), Error> + 'a;
#[derive(Default)]
pub struct Observer<'a> {
    pub cancel: Option<&'a mut dyn FnMut() -> bool>,
    pub progress: Option<&'a mut ProgressCallback<'a>>,
}
impl Observer<'_> {
    fn check(&mut self) -> Result<(), Error> {
        if self.cancel.as_mut().is_some_and(|callback| callback()) {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    }
    fn progress(&mut self, progress: Progress<'_>) -> Result<(), Error> {
        match &mut self.progress {
            Some(callback) => callback(progress),
            None => Ok(()),
        }
    }
}
pub struct FolderUpload<'a> {
    pub local_folder: &'a Path,
    pub directory: &'a str,
    pub remote_path: &'a str,
    pub base_url: &'a str,
    pub token: &'a str,
    pub filenames: Option<&'a [String]>,
}
impl FolderUpload<'_> {
    fn entries(&self) -> Result<Vec<String>, Error> {
        if let Some(filenames) = self.filenames {
            let mut seen = HashSet::new();
            let mut entries = Vec::new();
            for name in filenames {
                if name.is_empty()
                    || [".", ".."].contains(&name.as_str())
                    || name.contains(['/', '\\'])
                {
                    return Err(Error::InvalidFilename);
                }
                if seen.contains(name) {
                    continue;
                }
                if !self.local_folder.join(name).is_file() {
                    return Err(Error::MissingFile(name.clone()));
                }
                seen.insert(name);
                entries.push(name.clone());
            }
            Ok(entries)
        } else {
            let mut entries = Vec::new();
            for entry in fs::read_dir(self.local_folder).map_err(Error::Folder)? {
                let entry = entry.map_err(Error::Folder)?;
                if entry.file_type().map_err(Error::Folder)?.is_file() {
                    entries.push(
                        entry
                            .file_name()
                            .into_string()
                            .map_err(|_| Error::FilenameEncoding)?,
                    );
                }
            }
            entries.sort();
            Ok(entries)
        }
    }
    pub fn run(&self, observer: &mut Observer<'_>) -> Result<bool, Error> {
        observer.check()?;
        let entries = self.entries()?;
        if self.token.is_empty() {
            return Err(Error::MissingSession);
        }
        let client = Client::file();
        for filename in entries {
            let path = self.local_folder.join(&filename);
            let url = api_url(
                self.base_url,
                &["upload", self.directory, self.remote_path, &filename],
            )?;
            let size = fs::metadata(&path)?.len();
            let mut last_error = None;
            for _ in 0..2 {
                observer.check()?;
                let mut stream = Stream {
                    path: &path,
                    file: None,
                    observer,
                    filename: &filename,
                    size,
                    sent: 0,
                    started: false,
                    ended: false,
                    error: None,
                };
                let mut headers = auth(self.token);
                headers.insert("Content-Type".into(), "application/octet-stream".into());
                headers.insert("X-File-Size".into(), size.to_string());
                let response = client.request(
                    Request {
                        url: &url,
                        method: ureq::http::Method::PUT,
                        headers,
                    },
                    Body::Stream(&mut stream),
                );
                let sent = stream.sent;
                let response = match stream.error.take() {
                    Some(error) => Err(error),
                    None => response,
                };
                let result = response.and_then(|response| {
                    let (text, body) = response.mapping()?;
                    if !(200..300).contains(&response.status) || !body.get("ok").is_some_and(truth)
                    {
                        let error = or_empty(body.get("error"))?;
                        return Err(Error::Source(format!(
                            "HTTP {}: {}",
                            response.status,
                            truncate(if error.is_empty() { &text } else { &error }, 200)
                        )));
                    }
                    let remote = remote_size(body.get("size"))?;
                    if remote != i128::from(sent) {
                        return Err(Error::Source(format!(
                            "size mismatch for {filename}: sent {sent}, remote {remote}"
                        )));
                    }
                    Ok(())
                });
                match result {
                    Ok(()) => {
                        last_error = None;
                        break;
                    }
                    Err(error) => {
                        observer.check()?;
                        last_error = Some(error);
                    }
                }
            }
            if let Some(source) = last_error {
                return Err(Error::File {
                    filename,
                    source: Box::new(source),
                });
            }
            observer.check()?;
        }
        Ok(true)
    }
}
struct Stream<'a, 'b> {
    path: &'a Path,
    file: Option<File>,
    observer: &'a mut Observer<'b>,
    filename: &'a str,
    size: u64,
    sent: u64,
    started: bool,
    ended: bool,
    error: Option<Error>,
}
impl Stream<'_, '_> {
    fn read_chunk(&mut self, output: &mut [u8]) -> Result<usize, Error> {
        if self.ended {
            return Ok(0);
        }
        if !self.started {
            self.started = true;
            self.observer.progress(Progress {
                filename: self.filename,
                sent: 0,
                size: self.size,
                chunk: 0,
            })?;
            self.observer.check()?;
            self.file = Some(File::open(self.path)?);
        }
        self.observer.check()?;
        let limit = output.len().min(CHUNK_SIZE);
        let mut size = 0;
        if let Some(file) = &mut self.file {
            while size < limit {
                let count = file.read(&mut output[size..limit])?;
                if count == 0 {
                    break;
                }
                size += count;
            }
        }
        if size == 0 {
            self.ended = true;
            self.file = None;
            return Ok(0);
        }
        self.sent +=
            u64::try_from(size).map_err(|_| Error::Source("chunk length overflow".into()))?;
        self.observer.progress(Progress {
            filename: self.filename,
            sent: self.sent,
            size: self.size,
            chunk: size,
        })?;
        Ok(size)
    }
}
impl Read for Stream<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self.read_chunk(output) {
            Ok(count) => Ok(count),
            Err(error) => {
                let message = error.to_string();
                self.error = Some(error);
                Err(io::Error::other(message))
            }
        }
    }
}
