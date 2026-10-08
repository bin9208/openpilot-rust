use crate::Error;
use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct Assets {
    hashes: Mutex<HashMap<(PathBuf, SystemTime, u64), String>>,
}

pub fn resolve(root: &Path, asset: &str) -> Option<PathBuf> {
    let root = fs::canonicalize(root).ok()?;
    let relative = asset.trim_start_matches('/');
    let requested = root.join(relative);
    let resolved = fs::canonicalize(requested).ok()?;
    (resolved.starts_with(&root) && resolved.is_file()).then_some(resolved)
}

pub(crate) fn missing_inside(root: &Path, asset: &str) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    let requested = root.join(asset.trim_start_matches('/'));
    !requested.exists()
        && requested
            .ancestors()
            .find_map(|path| fs::canonicalize(path).ok())
            .is_some_and(|path| path.starts_with(&root))
}

impl Assets {
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
    if !path.ends_with(".js") && !path.ends_with(".css") {
        return Ok(());
    }
    let Some(source) = resolve(root, path) else {
        return Ok(());
    };
    for _ in 0..3 {
        let before = fs::metadata(&source)?;
        let bytes = fs::read(&source)?;
        let after = fs::metadata(&source)?;
        if before.modified()? != after.modified()? || before.len() != after.len() {
            continue;
        }
        let destination = PathBuf::from(format!("{}.gz", source.display()));
        if let Ok(file) = fs::File::open(&destination) {
            let mut decoded = flate2::read::GzDecoder::new(file);
            let mut old = Vec::new();
            if std::io::Read::read_to_end(&mut decoded, &mut old).is_ok() && old == bytes {
                return Ok(());
            }
        }
        let mtime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;
        let mut compressor = GzBuilder::new()
            .mtime(mtime)
            .write(Vec::new(), Compression::best());
        compressor.write_all(&bytes)?;
        let encoded = compressor.finish()?;
        let parent = source
            .parent()
            .ok_or_else(|| Error::Source("invalid static asset directory".into()))?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".carrot_asset_")
            .tempfile_in(parent)?;
        temporary.write_all(&encoded)?;
        temporary
            .persist(&destination)
            .map_err(|error| Error::Io(error.error))?;
        let current = fs::metadata(&source)?;
        if current.modified()? == before.modified()? && current.len() == before.len() {
            return Ok(());
        }
        let _ = fs::remove_file(destination);
    }
    let _ = fs::remove_file(format!("{}.gz", source.display()));
    let _ = fs::remove_file(format!("{}.br", source.display()));
    Ok(())
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
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "html" => "text/html",
        "json" | "map" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/vnd.microsoft.icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/x-wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}
