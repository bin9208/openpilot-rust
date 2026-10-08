use crate::Error;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Assets {
    hashes: Mutex<HashMap<(PathBuf, SystemTime, u64), String>>,
    codec: Option<crate::static_web::brotli::Brotli>,
}

impl Default for Assets {
    fn default() -> Self {
        Self {
            hashes: Mutex::default(),
            codec: crate::static_web::brotli::Brotli::load(),
        }
    }
}

pub fn resolve(root: &Path, asset: &str) -> Option<PathBuf> {
    let root = fs::canonicalize(root).ok()?;
    let relative = asset.strip_prefix('/').unwrap_or(asset);
    let requested = root.join(relative);
    let resolved = fs::canonicalize(requested).ok()?;
    (resolved.starts_with(&root) && resolved.is_file()).then_some(resolved)
}

impl Assets {
    pub fn refresh(&self, root: &Path, path: &str) -> Result<(), Error> {
        crate::static_web::compression::refresh_asset(root, path, self.codec.as_ref())
    }

    pub fn precompress(&self, root: &Path) -> Result<(), Error> {
        for directory in ["js", "css"] {
            let directory = root.join(directory);
            if directory.is_dir() {
                self.precompress_directory(root, &directory)?;
            }
        }
        Ok(())
    }

    fn precompress_directory(&self, root: &Path, directory: &Path) -> Result<(), Error> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                self.precompress_directory(root, &entry.path())?;
            } else if let Ok(relative) = entry.path().strip_prefix(root) {
                if let Err(error) = self.refresh(root, &relative.to_string_lossy()) {
                    eprintln!("static precompression: {error}");
                }
            }
        }
        Ok(())
    }

    pub fn fingerprint(&self, root: &Path, asset: &str) -> Option<String> {
        if (!asset.ends_with(".js") && !asset.ends_with(".css"))
            || asset.starts_with("//")
            || asset.starts_with("\\\\")
        {
            return None;
        }
        let resolved = resolve(root, asset)?;
        for _ in 0..3 {
            let metadata = fs::metadata(&resolved).ok()?;
            let key = (resolved.clone(), metadata.modified().ok()?, metadata.len());
            let fingerprint = {
                let mut cache = self.hashes.lock().ok()?;
                match cache.get(&key) {
                    Some(hash) => hash.clone(),
                    None => {
                        let hash = format!("{:x}", Sha256::digest(fs::read(&resolved).ok()?));
                        cache.insert(key.clone(), hash.clone());
                        hash
                    }
                }
            };
            let current = fs::metadata(&resolved).ok()?;
            if current.modified().ok()? == key.1 && current.len() == key.2 {
                return Some(fingerprint);
            }
        }
        None
    }

    pub fn immutable(&self, root: &Path, path: &str, query: Option<&str>) -> bool {
        if !["/js/", "/css/", "/assets/"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
        {
            return false;
        }
        let versions: Vec<String> =
            url::form_urlencoded::parse(query.unwrap_or_default().as_bytes())
                .filter_map(|(name, value)| (name == "v").then(|| value.into_owned()))
                .collect();
        if path.starts_with("/js/vendor/") || path.starts_with("/css/vendor/") {
            return !versions.is_empty();
        }
        versions.len() == 1 && self.fingerprint(root, path).as_ref() == versions.first()
    }
}

pub fn refresh_gzip(root: &Path, path: &str) -> Result<(), Error> {
    crate::static_web::compression::refresh_asset(
        root,
        path,
        crate::static_web::brotli::Brotli::load().as_ref(),
    )
}

pub fn etag(metadata: &fs::Metadata) -> Result<String, Error> {
    let time = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Source("asset timestamp before epoch".into()))?;
    Ok(format!("\"{:x}-{:x}\"", time.as_nanos(), metadata.len()))
}

pub(crate) fn last_modified(metadata: &fs::Metadata) -> Result<String, Error> {
    let modified = metadata.modified()?;
    let seconds = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Source("asset timestamp before epoch".into()))?;
    let rounded = if seconds.subsec_nanos() == 0 {
        modified
    } else {
        modified
            .checked_add(std::time::Duration::from_secs(1))
            .ok_or_else(|| Error::Source("asset timestamp overflow".into()))?
    };
    Ok(httpdate::fmt_http_date(rounded))
}

pub fn content_type(path: &Path) -> &'static str {
    crate::static_web::mime::content_type(path)
}
