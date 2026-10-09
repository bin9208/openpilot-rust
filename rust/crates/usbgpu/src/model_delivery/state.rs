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
pub fn pinned_manifest() -> Manifest {
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

pub fn fetch_manifest(agent: &ureq::Agent, url: &str) -> Result<Manifest, Error> {
    if url == DEFAULT_MANIFEST_URL {
        return Ok(pinned_manifest());
    }
    let response = agent
        .get(url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "identity")
        .header("User-Agent", "carrot-modeld/1")
        .call()?;
    if !response.status().is_success() {
        return Err(Error::Invalid(format!(
            "manifest HTTP {}",
            response.status()
        )));
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
                return Ok(Self::default())
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
