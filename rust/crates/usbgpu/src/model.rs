use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Paths {
    pub models: PathBuf,
    pub cache: PathBuf,
    pub assets: PathBuf,
}
impl Paths {
    pub fn for_runtime(root: &Path, assets: PathBuf) -> Result<Self, Error> {
        let cache = match std::env::var_os("CARROT_BIG_MODEL_DIR").filter(|path| !path.is_empty()) {
            Some(path) => PathBuf::from(path),
            None if Path::new("/TICI").is_file() => PathBuf::from("/data/media/0/carrot/models"),
            None => {
                let home = match std::env::var_os("HOME") {
                    Some(path) if path.is_empty() => PathBuf::from("/"),
                    Some(path) => PathBuf::from(path),
                    None => home::home_dir().ok_or(Error::Contract("missing home directory"))?,
                };
                home.join(".comma/models")
            }
        };
        Ok(Self {
            cache,
            models: root.join("openpilot/selfdrive/modeld/models"),
            assets,
        })
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ModelStatus {
    pub compiled: bool,
    pub compile_pending: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub model_id: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}
pub const DEFAULT_MANIFEST_URL: &str =
    "https://upload.shind0.synology.me/models/comma4-big-cinque-v3/manifest.json";

pub(crate) fn authority(value: &str) -> Option<&str> {
    let rest = value
        .split_once("://")
        .map(|(_, rest)| rest)
        .or_else(|| value.strip_prefix("//"))?;
    Some(rest.split(['/', '?', '#']).next().unwrap_or(""))
}

pub(crate) fn resolve(base: &str, relative: &str) -> Result<String, url::ParseError> {
    let resolved = url::Url::parse(base)?.join(relative)?;
    let Some(raw) = authority(relative).or_else(|| authority(base)) else {
        return Ok(resolved.to_string());
    };
    Ok(format!(
        "{}://{}{}",
        resolved.scheme(),
        raw,
        &resolved[url::Position::BeforePath..]
    ))
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Manifest {
    pub fn validate(&self) -> bool {
        name(&self.model_id)
            && name(&self.filename)
            && (self.filename.ends_with(".pkl") || self.filename.ends_with(".onnx"))
            && (1..=4 * 1024 * 1024 * 1024).contains(&self.size)
            && sha(&self.sha256)
            && !self.url.is_empty()
            && url::Url::parse(&self.url).is_ok_and(|u| u.scheme() == "https")
    }
    pub fn cache_filename(&self) -> String {
        let path = Path::new(&self.filename);
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(&self.filename);
        let suffix = path
            .extension()
            .and_then(|value| value.to_str())
            .map_or_else(String::new, |value| format!(".{value}"));
        format!("{stem}-{}{suffix}", &self.sha256[..16])
    }
    pub fn resolve_url(&mut self, base: &str) -> bool {
        if self.url.is_empty() {
            return false;
        }
        let Ok(url) = resolve(base, &self.url) else {
            return false;
        };
        self.url = url;
        self.validate()
    }
    pub fn precompiled_only(&self) -> bool {
        self.filename.ends_with(".pkl")
    }
}
fn json(path: &Path) -> Option<Value> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}
fn manifest(value: &Value) -> Option<Manifest> {
    value.as_object()?;
    let mut result: Manifest = serde_json::from_value(value.clone()).ok()?;
    result.resolve_url(DEFAULT_MANIFEST_URL).then_some(result)
}
pub fn active_manifest(paths: &Paths) -> Option<Manifest> {
    let state = json(&paths.cache.join("state.json"))?;
    let active = if state.get("active").is_some_and(|v| !v.is_null()) {
        manifest(&state["active"])
    } else {
        None
    };
    if state.get("previous").is_some_and(|v| !v.is_null()) && manifest(&state["previous"]).is_none()
    {
        return None;
    }
    let active = active?;
    let path = paths.cache.join(active.cache_filename());
    path.is_file().then_some(())?;
    (path.metadata().ok()?.len() == active.size).then_some(active)
}
pub fn local_compiled_path(paths: &Paths, model: &Manifest) -> PathBuf {
    paths
        .models
        .join(format!("big_driving_{}_tinygrad.pkl", &model.sha256[..16]))
}
pub fn installed(paths: &Paths, model: &Manifest) -> Option<PathBuf> {
    crate::model_delivery::precompiled::installed(model, &paths.cache, &paths.assets)
        .unwrap_or_default()
}
pub fn status(paths: &Paths) -> Result<ModelStatus, Error> {
    Ok(ModelStatus {
        compiled: compiled_path(paths).is_some(),
        compile_pending: active_manifest(paths).is_some() && active_compiled_path(paths).is_none(),
    })
}

pub fn active_compiled_path(paths: &Paths) -> Option<PathBuf> {
    let model = active_manifest(paths)?;
    if let Some(path) = installed(paths, &model) {
        return Some(path);
    }
    if model.precompiled_only() {
        return None;
    }
    local_ready(paths, &model)
}

fn local_ready(paths: &Paths, model: &Manifest) -> Option<PathBuf> {
    let path = local_compiled_path(paths, model);
    crate::local_artifact::admission::ready(&path, &paths.assets, &model.sha256)
        .ok()
        .map(|_| path)
}

pub fn compiled_path(paths: &Paths) -> Option<PathBuf> {
    if let Some(path) = active_compiled_path(paths) {
        return Some(path);
    }
    if active_manifest(paths).is_some_and(|model| model.precompiled_only()) {
        return None;
    }
    let previous = crate::model_delivery::State::read(&paths.cache)
        .ok()?
        .previous?;
    local_ready(paths, &previous)
}

pub fn remove_active_chunk_manifest(paths: &Paths) -> Result<bool, Error> {
    let Some(model) = active_manifest(paths) else {
        return Ok(false);
    };
    let path = match installed(paths, &model) {
        Some(path) => path,
        None if model.precompiled_only() => return Ok(false),
        None => local_compiled_path(paths, &model),
    };
    let mut manifest = path.into_os_string();
    manifest.push(".chunkmanifest");
    match fs::remove_file(Path::new(&manifest)) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
