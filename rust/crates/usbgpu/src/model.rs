use crate::Error;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Paths {
    pub models: PathBuf,
    pub cache: PathBuf,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ModelStatus {
    pub compiled: bool,
    pub compile_pending: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub model_id: String,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
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
        let (stem, suffix) = self
            .filename
            .rsplit_once('.')
            .expect("validated manifest suffix");
        format!("{stem}-{}.{}", &self.sha256[..16], suffix)
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
    let result: Manifest = serde_json::from_value(value.clone()).ok()?;
    result.validate().then_some(result)
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
fn chunk_manifest_exists(path: &Path) -> bool {
    let mut name = path.as_os_str().to_os_string();
    name.push(".chunkmanifest");
    Path::new(&name).is_file()
}
pub fn installed(paths: &Paths, model: &Manifest) -> Option<PathBuf> {
    let root = paths.cache.join("precompiled").join(&model.sha256);
    let value = json(&root.join("installed.json"))?;
    if value["protocol"] != 1
        || value["gpu_arch"] != "gfx1200"
        || value["frame_skip"] != 4
        || value["camera_resolutions"] != serde_json::json!([[1928, 1208], [1344, 760]])
    {
        return None;
    }
    let generic = match value["format"].as_str()? {
        "comma-generic-onnx" => true,
        "comma-run-model" => false,
        _ => return None,
    };
    if value[if generic {
        "model_sha256"
    } else {
        "onnx_sha256"
    }] != model.sha256
    {
        return None;
    }
    let catalog = url::Url::parse(value["catalog_url"].as_str()?).ok()?;
    for (key, limit) in [
        ("pickle", 4 * 1024 * 1024 * 1024u64),
        ("runtime", 128 * 1024 * 1024),
    ] {
        if !sha(value[key]["sha256"].as_str()?)
            || !(1..=limit).contains(&value[key]["size"].as_u64()?)
        {
            return None;
        }
        let url = catalog.join(value[key]["url"].as_str()?).ok()?;
        if url.scheme() != "https"
            || url[url::Position::BeforeUsername..url::Position::AfterPort]
                != catalog[url::Position::BeforeUsername..url::Position::AfterPort]
        {
            return None;
        }
    }
    if generic && value["pickle"]["sha256"] != model.sha256 {
        return None;
    }
    if root.join("rejected").exists()
        || root.join("model.pkl").metadata().ok()?.len() != value["pickle"]["size"].as_u64()?
    {
        return None;
    }
    let runtime_name = format!("runtime-{}", &value["runtime"]["sha256"].as_str()?[..16]);
    if value["runtime_directory"] != runtime_name {
        return None;
    }
    let runtime = root.join(runtime_name);
    let entry = if generic {
        "examples/openpilot/compile_warp.py"
    } else {
        "model_runtime.py"
    };
    if !runtime.join(entry).is_file() || !runtime.join("tinygrad/__init__.py").is_file() {
        return None;
    }
    Some(root.join("model.pkl"))
}
pub fn status(paths: &Paths) -> Result<ModelStatus, Error> {
    let Some(model) = active_manifest(paths) else {
        return Ok(ModelStatus {
            compiled: false,
            compile_pending: false,
        });
    };
    let installed = installed(paths, &model).is_some();
    let local = chunk_manifest_exists(&local_compiled_path(paths, &model));
    Ok(ModelStatus {
        compiled: installed || (!model.precompiled_only() && local),
        compile_pending: !installed && !local,
    })
}
