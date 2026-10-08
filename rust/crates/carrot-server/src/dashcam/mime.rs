use crate::Error;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

const KNOWN_FILES: [&str; 9] = [
    "/etc/mime.types",
    "/etc/httpd/mime.types",
    "/etc/httpd/conf/mime.types",
    "/etc/apache/mime.types",
    "/etc/apache2/mime.types",
    "/usr/local/etc/httpd/conf/mime.types",
    "/usr/local/lib/netscape/mime.types",
    "/usr/local/etc/httpd/conf/mime.types",
    "/usr/local/etc/mime.types",
];
pub(super) struct DownloadMime {
    files: Vec<PathBuf>,
    loaded: Mutex<Option<[String; 3]>>,
}
impl DownloadMime {
    pub(super) fn original() -> Self {
        Self::new(KNOWN_FILES.into_iter().map(PathBuf::from).collect())
    }
    pub(super) fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            loaded: Mutex::new(None),
        }
    }
    pub(super) fn content_type(&self, name: &str) -> Result<String, Error> {
        let mut loaded = self
            .loaded
            .lock()
            .map_err(|_| Error::Source("dashcam MIME lock poisoned".into()))?;
        if loaded.is_none() {
            let mut types = [
                "application/octet-stream".into(),
                "video/mp4".into(),
                "application/octet-stream".into(),
            ];
            for path in &self.files {
                if !path.is_file() {
                    continue;
                }
                let text = fs::read_to_string(path)
                    .map_err(|error| crate::state::io_error(error, path))?;
                for line in text.split(['\n', '\r']) {
                    let words = line
                        .split(|character: char| {
                            character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
                        })
                        .filter(|word| !word.is_empty())
                        .take_while(|word| !word.starts_with('#'))
                        .collect::<Vec<_>>();
                    let Some((mime, extensions)) = words.split_first() else {
                        continue;
                    };
                    for extension in extensions {
                        if let Some(index) = ["ts", "mp4", "zst"]
                            .iter()
                            .position(|value| value == extension)
                        {
                            types[index] = (*mime).into();
                        }
                    }
                }
            }
            *loaded = Some(types);
        }
        let index = match Path::new(name).extension().and_then(|value| value.to_str()) {
            Some("ts") => Some(0),
            Some("mp4") => Some(1),
            Some("zst") => Some(2),
            _ => None,
        };
        Ok(index
            .and_then(|index| loaded.as_ref().map(|types| types[index].clone()))
            .unwrap_or_else(|| "application/octet-stream".into()))
    }
}
