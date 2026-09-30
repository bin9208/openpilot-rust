//! Native build metadata and process-cached Git helpers from system/version.py and common/git.py.
#![forbid(unsafe_code)]
pub mod git;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Json(#[from] openpilot_logmessaged::JsonError),
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
    #[error("attribute operation on nonstring/nonobject value: {0}")]
    Attribute(&'static str),
    #[error("unsupported value type: {0}")]
    Type(&'static str),
    #[error("dictionary has no slice key")]
    Key,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("git exited with {0}")]
    GitExit(std::process::ExitStatus),
    #[error("version cache lock poisoned")]
    CachePoisoned,
    #[error("version header has no quoted field")]
    VersionHeader,
    #[error("invalid build metadata")]
    InvalidMetadata,
    #[error(transparent)]
    Logging(#[from] openpilot_logging::Error),
}

mod metadata;
mod python;
pub use metadata::{
    build_metadata_from_dict, BuildMetadata, OpenpilotMetadata, BUILD_METADATA_FILENAME,
    RELEASE_BRANCHES, TERMS_VERSION, TESTED_BRANCHES, TRAINING_VERSION,
};
pub use openpilot_logmessaged::JsonValue;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::Path,
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};

fn metadata_path_exists(path: &Path) -> Result<bool, Error> {
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Ok(false);
    }
    match path.try_exists() {
        // pathlib.Path.exists ignores ELOOP and EBADF in Python 3.12, as well as missing paths.
        Err(error) if matches!(error.raw_os_error(), Some(9 | 40)) => Ok(false),
        result => result.map_err(Error::Io),
    }
}
fn text(points: Vec<u32>) -> Result<JsonValue, Error> {
    JsonValue::codepoints(points).ok_or(Error::Type("invalid Unicode code point"))
}
fn read_text(path: &Path) -> Result<String, Error> {
    Ok(String::from_utf8(std::fs::read(path)?)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
/// Extract the first quoted field from `openpilot/common/version.h` using universal newlines.
///
/// # Errors
/// Returns I/O/UTF-8 errors or `VersionHeader` if the quote is absent.
pub fn get_version(path: &Path) -> Result<String, Error> {
    read_text(&path.join("openpilot/common/version.h"))?
        .split('"')
        .nth(1)
        .map(str::to_owned)
        .ok_or(Error::VersionHeader)
}
/// Read the first paragraph, ending at the first pair of newlines in `RELEASES.md`.
///
/// # Errors
/// Returns the underlying I/O or UTF-8 decoding error.
pub fn get_release_notes(path: &Path) -> Result<String, Error> {
    let text = read_text(&path.join("RELEASES.md"))?;
    Ok(text
        .split_once("\n\n")
        .map_or(text.as_str(), |(first, _)| first)
        .into())
}
static PREBUILT: OnceLock<Mutex<HashMap<OsString, bool>>> = OnceLock::new();
static DIRTY: OnceLock<Mutex<HashMap<OsString, bool>>> = OnceLock::new();
fn cached_flag(
    cache: &OnceLock<Mutex<HashMap<OsString, bool>>>,
    path: &Path,
    compute: impl FnOnce() -> Result<bool, Error>,
) -> Result<bool, Error> {
    let cache = cache.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&value) = cache
        .lock()
        .map_err(|_| Error::CachePoisoned)?
        .get(path.as_os_str())
    {
        return Ok(value);
    }
    let value = compute()?;
    cache
        .lock()
        .map_err(|_| Error::CachePoisoned)?
        .insert(path.as_os_str().to_os_string(), value);
    Ok(value)
}
/// Cache whether the `prebuilt` marker exists, including a successful false result.
///
/// # Errors
/// Returns `CachePoisoned` if another thread panicked while holding the cache lock.
pub fn is_prebuilt(path: &Path) -> Result<bool, Error> {
    cached_flag(&PREBUILT, path, || Ok(path.join("prebuilt").exists()))
}
/// Cache the source dirty policy: remote/branch checks, prebuilt bypass, and tracking-ref diff.
/// The no-cwd helpers intentionally refer to the process cwd, matching the inherited source.
/// Untracked files do not make the source dirty; a failed diff exit does.
///
/// # Errors
/// Git spawn/cwd/UTF-8 and cache errors propagate and are not cached.
pub fn is_dirty(path: &Path) -> Result<bool, Error> {
    cached_flag(&DIRTY, path, || {
        if git::get_origin_default()?.is_empty() || git::get_short_branch_default()?.is_empty() {
            return Ok(true);
        }
        if is_prebuilt(path)? {
            return Ok(false);
        }
        let branch = git::get_branch_default()?;
        if branch.is_empty() {
            return Ok(true);
        }
        let status = Command::new("git")
            .args(["--no-optional-locks", "diff", "--quiet", &branch, "--"])
            .current_dir(path)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        Ok(!status.success())
    })
}
/// Prefer build.json, then the source tree. There is no fallback after an invalid build.json.
/// Unlike Git and dirty helpers, metadata itself is reread on every call.
///
/// # Errors
/// Returns source file/JSON/Git errors, or `InvalidMetadata` after emitting the original
/// error event if neither build source exists. Native logging failures also propagate.
pub fn get_build_metadata(path: &Path) -> Result<BuildMetadata, Error> {
    let build = path.join(BUILD_METADATA_FILENAME);
    if metadata_path_exists(&build)? {
        return build_metadata_from_dict(&JsonValue::parse(&read_text(&build)?)?);
    }
    if metadata_path_exists(&path.join(".git"))? {
        return metadata::from_source(path);
    }
    let mut logger = openpilot_logging::producer::Factory::for_runtime()?.logger();
    logger.emit(
        openpilot_logging::log_site!(),
        openpilot_logging::record::Record::text(
            openpilot_logging::record::Level::Error,
            "unable to get build metadata".into(),
        )
        .with_exception("NoneType: None\n".into()),
    )?;
    Err(Error::InvalidMetadata)
}

/// Render Python str() without replacement-decoding lone surrogates.
pub fn python_str(value: &JsonValue) -> Result<Vec<u32>, Error> {
    python::join(&[value], "")
}
