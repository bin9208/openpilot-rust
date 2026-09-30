use crate::{Event, EventSink, Transfer, Uploader};
use openpilot_logging::log_site;
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

pub trait Attributes {
    fn get(&mut self, path: &Path) -> io::Result<Option<Vec<u8>>>;
    fn mark_uploaded(&mut self, path: &Path) -> io::Result<()>;
}
#[derive(Default)]
pub struct XattrCache {
    values: HashMap<PathBuf, Option<Vec<u8>>>,
}
impl Attributes for XattrCache {
    fn get(&mut self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        if let Some(value) = self.values.get(path) {
            return Ok(value.clone());
        }
        let mut buffer = vec![0; 65536];
        let value = match rustix::fs::getxattr(path, "user.upload", &mut buffer[..]) {
            Ok(length) => {
                buffer.truncate(length);
                Some(buffer)
            }
            Err(rustix::io::Errno::NODATA) => None,
            Err(error) => return Err(error.into()),
        };
        self.values.insert(path.to_owned(), value.clone());
        Ok(value)
    }
    fn mark_uploaded(&mut self, path: &Path) -> io::Result<()> {
        self.values.remove(path);
        Ok(rustix::fs::setxattr(
            path,
            "user.upload",
            b"1",
            rustix::fs::XattrFlags::empty(),
        )?)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub name: OsString,
    pub key: PathBuf,
    pub path: PathBuf,
}
fn priority(name: &OsStr) -> u16 {
    match name.as_encoded_bytes() {
        b"qlog" | b"qlog.zst" => 0,
        b"qcamera.ts" => 1,
        _ => 1000,
    }
}
fn immediate(path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    bytes.windows(6).any(|s| s == b"crash/") || bytes.windows(5).any(|s| s == b"boot/")
}
pub fn clear_locks(root: &Path, sink: &mut impl EventSink) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let result = (|| -> io::Result<()> {
            for file in fs::read_dir(entry.path())? {
                let file = file?;
                if file.file_name().as_encoded_bytes().ends_with(b".lock") {
                    fs::remove_file(file.path())?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            sink.emit(Event::exception(
                log_site!(),
                "clear_locks failed",
                &(entry.path(), error),
            ));
        }
    }
    Ok(())
}
impl<T: Transfer, A: Attributes, S: EventSink> Uploader<T, A, S> {
    pub fn list_upload_files(
        &mut self,
        metered: bool,
        requested_routes: Option<&str>,
    ) -> Vec<Candidate> {
        if !self.root.is_dir() {
            return Vec::new();
        }
        let directories = (|| -> io::Result<Vec<OsString>> {
            let mut dirs = Vec::new();
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    dirs.push(entry.file_name());
                }
            }
            dirs.sort_by_cached_key(|name| openpilot_deleter::directory_sort_key(name));
            Ok(dirs)
        })();
        let directories = match directories {
            Ok(dirs) => dirs,
            Err(error) => {
                self.events.emit(Event::exception(
                    log_site!(),
                    "listdir_by_creation failed",
                    &(&self.root, error),
                ));
                return Vec::new();
            }
        };
        let requested: Vec<_> = requested_routes
            .unwrap_or("")
            .split(',')
            .filter(|route| !route.is_empty())
            .map(|route| route.rsplit('|').next().unwrap_or(""))
            .collect();
        let mut output = Vec::new();
        for directory in directories {
            let path = self.root.join(&directory);
            let names = fs::read_dir(&path).and_then(|entries| {
                entries
                    .map(|entry| entry.map(|entry| entry.file_name()))
                    .collect::<io::Result<Vec<_>>>()
            });
            let Ok(mut names) = names else {
                continue;
            };
            if names
                .iter()
                .any(|name| name.as_encoded_bytes().ends_with(b".lock"))
            {
                continue;
            }
            names.sort_by_key(|name| priority(name));
            for name in names {
                let file = path.join(&name);
                let key = PathBuf::from(&directory).join(&name);
                let uploaded = fs::metadata(&file).and_then(|_| self.attributes.get(&file));
                match uploaded {
                    Ok(value) if value.as_deref() == Some(b"1") => continue,
                    Err(_) => {
                        self.events.emit(Event::Fields {
                            site: log_site!(),
                            name: "uploader_getxattr_failed",
                            fields: json!({"key":key.to_string_lossy(),"fn":file.to_string_lossy()}),
                        });
                        continue;
                    }
                    Ok(_) => {}
                }
                // Bare listdir names cannot equal the source's "crash/" or "boot/";
                // preserve its ineffective 12-hour metered filter.
                if metered
                    && name == "qcamera.ts"
                    && !requested
                        .iter()
                        .any(|route| directory.as_encoded_bytes().starts_with(route.as_bytes()))
                {
                    continue;
                }
                output.push(Candidate {
                    name,
                    key,
                    path: file,
                });
            }
        }
        output
    }
    pub fn next_file(
        &mut self,
        metered: bool,
        requested_routes: Option<&str>,
    ) -> Option<Candidate> {
        let files = self.list_upload_files(metered, requested_routes);
        files
            .iter()
            .find(|file| immediate(&file.path))
            .or_else(|| files.iter().find(|file| priority(&file.name) != 1000))
            .cloned()
    }
}
