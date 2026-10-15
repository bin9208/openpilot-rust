use super::{download_fallible, DownloadKind, Error, Event};
use crate::model::Manifest;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub use crate::model::DEFAULT_MANIFEST_URL;

#[must_use]
pub fn legacy_manifest() -> Manifest {
    Manifest {
        model_id: "comma-pr38932-cinque-v3-892fc3a1-e758b96d".into(),
        filename: "big_driving_tinygrad.pkl".into(),
        size: 776_634_338,
        sha256: "e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b".into(),
        url:
            "https://upload.shind0.synology.me/models/comma4-big-cinque-v3/big_driving_tinygrad.pkl"
                .into(),
    }
}

/// Previous retrained MDM remains readable for existing native assets and explicit selection.
#[must_use]
pub fn retrained_manifest() -> Manifest {
    Manifest {
        model_id: "comma-pr39047-mdm-retrained-6f4c0faf-256f2178".into(),
        filename: "big_driving_tinygrad.pkl".into(),
        size: 799_942_179,
        sha256: "256f2178366479a211933701c03518621ed3970289e6a8a43e5a2bcb15e55b5f".into(),
        url: "https://upload.shind0.synology.me/models/comma4-big-mdm-8d736683/big_driving_tinygrad.pkl".into(),
    }
}

/// Latest upstream MD v3 selection (8864280c); internal/DM/Jetlink are unchanged.
#[must_use]
pub fn pinned_manifest() -> Manifest {
    Manifest {
        model_id: "comma-pr39087-md-v3-e86b58dc-d4c7a3fc".into(),
        filename: "big_driving_tinygrad.pkl".into(),
        size: 799_941_965,
        sha256: "d4c7a3fcfc95813826ca871a80bea18df833aacb2eb6f5333e25a01e65b1a422".into(),
        url: "https://upload.shind0.synology.me/models/comma4-big-md-v3-1365126a/big_driving_tinygrad.pkl".into(),
    }
}

#[must_use]
pub fn known_manifest(digest: &str) -> Option<Manifest> {
    [pinned_manifest(), retrained_manifest(), legacy_manifest()]
        .into_iter()
        .find(|model| model.sha256 == digest)
}
pub const RETRAINED_MANIFEST_URL: &str =
    "https://upload.shind0.synology.me/models/comma4-big-mdm-8d736683/manifest.json";
pub const LEGACY_MANIFEST_URL: &str =
    "https://upload.shind0.synology.me/models/comma4-big-cinque-v3/manifest.json";
#[must_use]
pub fn source_manifest(url: &str) -> Option<Manifest> {
    match url {
        DEFAULT_MANIFEST_URL => Some(pinned_manifest()),
        LEGACY_MANIFEST_URL => Some(legacy_manifest()),
        RETRAINED_MANIFEST_URL => Some(retrained_manifest()),
        _ => None,
    }
}

pub fn fetch_manifest(agent: &ureq::Agent, url: &str) -> Result<Manifest, Error> {
    if let Some(model) = source_manifest(url) {
        return Ok(model);
    }
    let response = agent
        .get(url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "identity")
        .header("User-Agent", "carrot-modeld/1")
        .call()?;
    if !response.status().is_success() {
        return Err(Error::HttpStatus {
            code: response.status().as_u16(),
            message: format!("manifest HTTP {}", response.status()),
        });
    }
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(65537)
        .read_to_end(&mut bytes)?;
    parse_manifest(&bytes, url)
}

pub fn parse_manifest(bytes: &[u8], url: &str) -> Result<Manifest, Error> {
    if bytes.len() > 65536 {
        return Err(Error::Invalid("model manifest is too large".into()));
    }
    let raw: serde_json::Value = serde_json::from_slice(bytes)?;
    if !raw.is_object() {
        return Err(Error::Invalid(
            "model manifest must be a JSON object".into(),
        ));
    }
    let mut value: Manifest = serde_json::from_value(raw)?;
    if !value.resolve_url(url) {
        return Err(Error::Invalid("invalid model manifest".into()));
    }
    Ok(value)
}

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub active: Option<Manifest>,
    pub previous: Option<Manifest>,
}
impl State {
    pub fn read(cache: &Path) -> Result<Self, Error> {
        let bytes = match fs::read(cache.join("state.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        let result = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .filter(|raw| {
                raw.is_object()
                    && ["active", "previous"]
                        .iter()
                        .all(|key| raw[key].is_null() || raw[key].is_object())
            })
            .and_then(|raw| serde_json::from_value::<Self>(raw).ok())
            .map(|mut state| {
                let active = state
                    .active
                    .as_mut()
                    .is_none_or(|value| value.resolve_url(DEFAULT_MANIFEST_URL));
                let previous = state
                    .previous
                    .as_mut()
                    .is_none_or(|value| value.resolve_url(DEFAULT_MANIFEST_URL));
                if active && previous {
                    state
                } else {
                    Self::default()
                }
            });
        match result {
            Some(state)
                if state.active.as_ref().is_none_or(Manifest::validate)
                    && state.previous.as_ref().is_none_or(Manifest::validate) =>
            {
                Ok(state)
            }
            Some(_) | None => Ok(Self::default()),
        }
    }
    fn write(&self, cache: &Path) -> Result<(), Error> {
        let mut staging = tempfile::Builder::new()
            .prefix(".state-")
            .suffix(".json")
            .tempfile_in(cache)?;
        serde_json::to_writer_pretty(&mut staging, self)?;
        staging.write_all(b"\n")?;
        staging.flush()?;
        staging.as_file().sync_all()?;
        staging
            .persist(cache.join("state.json"))
            .map_err(|error| error.error)?;
        Ok(())
    }
}

/// Bind a verified selection to its explicit catalog, even when content is unchanged.
pub fn bind_manifest(cache: &Path, model: &Manifest, manifest_url: &str) -> Result<(), Error> {
    if !model.validate() {
        return Err(Error::Invalid("invalid model manifest".into()));
    }
    let mut state = State::read(cache)?;
    state.active = Some(model.clone());
    let mut value = serde_json::to_value(state)?;
    value["manifest_url"] = serde_json::Value::String(manifest_url.to_owned());
    let mut staging = tempfile::Builder::new()
        .prefix(".state-")
        .suffix(".json")
        .tempfile_in(cache)?;
    serde_json::to_writer_pretty(&mut staging, &value)?;
    staging.write_all(b"\n")?;
    staging.flush()?;
    staging.as_file().sync_all()?;
    staging
        .persist(cache.join("state.json"))
        .map_err(|error| error.error)?;
    Ok(())
}

/// Download and atomically select a verified model, retaining the previous model.
pub fn ensure(
    agent: &ureq::Agent,
    model: &Manifest,
    cache: &Path,
    progress: &mut impl FnMut(u64, u64),
) -> Result<(PathBuf, bool), Error> {
    ensure_observed(agent, model, cache, &mut |event| match event {
        Event::Progress { downloaded, total } => progress(downloaded, total),
        Event::Verifying => {}
    })
}

pub fn ensure_observed(
    agent: &ureq::Agent,
    model: &Manifest,
    cache: &Path,
    observe: &mut impl FnMut(Event),
) -> Result<(PathBuf, bool), Error> {
    ensure_fallible(agent, model, cache, &mut |event| {
        observe(event);
        Ok(())
    })
}

pub fn ensure_fallible(
    agent: &ureq::Agent,
    model: &Manifest,
    cache: &Path,
    observe: &mut impl FnMut(Event) -> Result<(), Error>,
) -> Result<(PathBuf, bool), Error> {
    if !model.validate() {
        return Err(Error::Invalid("invalid model manifest".into()));
    }
    fs::create_dir_all(cache)?;
    let state = State::read(cache)?;
    let target = cache.join(model.cache_filename());
    let changed = state
        .active
        .as_ref()
        .is_none_or(|active| active.sha256 != model.sha256);
    if !changed && target.is_file() && target.metadata()?.len() == model.size {
        return Ok((target, false));
    }
    let target = download_fallible(
        agent,
        &model.url,
        &target,
        model.size,
        &model.sha256,
        DownloadKind::Model,
        observe,
    )?;
    if changed {
        let previous = match state.active {
            Some(active) if cache.join(active.cache_filename()).is_file() => Some(active),
            Some(_) | None => state.previous,
        };
        let next = State {
            active: Some(model.clone()),
            previous,
        };
        next.write(cache)?;
        for entry in fs::read_dir(cache)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("big_driving_supercombo-")
                && name.ends_with(".onnx")
                && name != model.cache_filename()
                && next
                    .previous
                    .as_ref()
                    .is_none_or(|item| name != item.cache_filename())
            {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok((target, changed))
}
