//! Original application draw calls are an immutable test oracle, never policy.
use super::confirm_draw::{self, Expected};
use openpilot_dev_tools::{image_array::ImageArray, seal::Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_text(text: &str) -> Result<()> {
    if digest(text.as_bytes()) != "b0091b0149b789c6fbc86c4d38da67f7806b42c86144cbab24b867bc5b0dc72f"
    {
        return Err("immutable original application draw capture changed".into());
    }
    Ok(())
}

pub fn corpus() -> Result<&'static str> {
    static TEXT: OnceLock<std::result::Result<String, String>> = OnceLock::new();
    match TEXT.get_or_init(|| {
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            include_bytes!("../fixtures/ui-application-lifecycle/original-draw.json.gz").as_slice(),
        )
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
        verify_text(&text).map_err(|e| e.to_string())?;
        Ok(text)
    }) {
        Ok(text) => Ok(text),
        Err(error) => Err(error.clone().into()),
    }
}

fn original(filename: &str) -> Result<&'static [u8]> {
    match filename {
        "source-original.json" => Ok(include_bytes!(
            "../fixtures/ui-application-lifecycle/source-original.json"
        )),
        "environment-original.json" => Ok(include_bytes!(
            "../fixtures/ui-application-lifecycle/environment-original.json"
        )),
        _ => Err("uncaptured original application fixture".into()),
    }
}

pub fn validate(text: &str, filename: &str, case: &Value) -> Result<Value> {
    verify_text(text)?;
    let data: Value = serde_json::from_str(text)?;
    let provenance = include_bytes!("../fixtures/ui-application-lifecycle/provenance.json");
    if data["provenance_sha256"] != digest(provenance)
        || data["source_sha256"] != serde_json::from_slice::<Value>(provenance)?["sources"]
        || data["closure_sha256"]
            != digest(include_bytes!(
                "../fixtures/ui-application-lifecycle/source-closure.json"
            ))
        || data["fixtures"][filename] != digest(original(filename)?)
    {
        return Err("application original source, closure or fixture changed".into());
    }
    let original: Value = serde_json::from_slice(original(filename)?)?;
    let original = original
        .as_array()
        .ok_or("application original fixture is not a list")?
        .iter()
        .find(|row| row["name"] == case["name"])
        .ok_or("application original case missing")?;
    if case != original {
        return Err("application original case, state or environment changed".into());
    }
    let trace = data["captures"]
        .as_array()
        .ok_or("application original drawing cases missing")?
        .iter()
        .find(|row| row["fixture"] == filename && row["name"] == case["name"])
        .ok_or("application original drawing case missing")?;
    if trace["case"] != *case
        || trace["png_sha256"] != case.get("png_sha256").cloned().unwrap_or(Value::Null)
    {
        return Err("application original drawing provenance changed".into());
    }
    Ok(trace.clone())
}

/// Pin the entire mechanical dispatch and source-generation boundary. A changed
/// raw mapping must not silently alter both the reference and replay oracle.
pub fn validate_boundary(root: &Path) -> Result<()> {
    let data: Value = serde_json::from_str(corpus()?)?;
    let pins = data["mechanical_source_sha256"]
        .as_object()
        .ok_or("application mechanical source pins missing")?;
    if pins.len() != 4 {
        return Err("application mechanical source pin set changed".into());
    }
    for (name, expected) in pins {
        // Keep the original draw capture immutable. These three reviewed source
        // revisions only reject failed window initialization and update the
        // generator's exact source pin; successful rendering is unchanged.
        // Require the new bytes, rather than accepting either revision.
        let reviewed = match (name.as_str(), expected.as_str()) {
            (
                "rust/crates/dev-tools/tests/fixtures/ui-forms/raw-raylib.cc",
                Some("5a65fa3e07c61e07237427beba46d48e1576afc62acf2fde0c4557f6d35815e5"),
            ) => "216030c03d807c46ad51e2b843185038bd6ee1dc3dbc49f8246cb115823d97b1",
            (
                "rust/crates/dev-tools/tests/fixtures/ui-application-lifecycle/raw-input.inc",
                Some("8080d6c26d815ded9c7852d5ca7751b4975bfcaf7702dc6cc4a2b0ea02a9643c"),
            ) => "b0e298c8d0f12bd04a54e6aa67c43204c7e7cdbe2d0fcd5dd1912e28ce065679",
            (
                "rust/crates/dev-tools/src/ui_application_lifecycle_raw.rs",
                Some("288c4e00410caeeaa60ea8d892c41d1ea2b08e7acb6d4638b593ac24cab7a5f5"),
            ) => "a78979c376d969157eff5cb9b322b80692b48226547f47559472c9207151372f",
            _ => expected.as_str().ok_or("invalid mechanical source pin")?,
        };
        if digest(&fs::read(root.join(name))?) != reviewed {
            return Err(format!("application mechanical boundary changed: {name}").into());
        }
    }
    Ok(())
}

pub fn replay(
    filename: &str,
    case: &Value,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
    validate_boundary(root)?;
    let text = corpus()?;
    confirm_draw::replay_trace(
        validate(text, filename, case)?,
        text,
        root,
        helper,
        command,
        directory,
    )
}

pub fn exact_pixels(actual: &ImageArray, expected: &ImageArray) -> Result<()> {
    if actual.mode != expected.mode
        || actual.shape != expected.shape
        || actual.values != expected.values
    {
        return Err("application pixels differ from independent original draw replay".into());
    }
    Ok(())
}

/// Retain only failed-case data while unwinding, before its TempDir disappears.
/// Errors here are diagnostic and never replace the original assertion failure.
pub struct FailureEvidence<'a> {
    pub destination: Option<PathBuf>,
    pub case: &'a Value,
    pub directory: &'a Path,
    pub command: &'a Command,
    pub result: &'a Output,
    pub replay: Option<(Vec<u8>, Vec<u8>)>,
}
impl FailureEvidence<'_> {
    fn retain(&self) -> Result<()> {
        let Some(base) = &self.destination else {
            return Ok(());
        };
        let name = self.case["name"]
            .as_str()
            .ok_or("application failure case missing")?;
        let path = base.join(format!("application-lifecycle-{name}"));
        let bytes = fs::read_dir(self.directory)?.try_fold(0_u64, |total, entry| {
            let size = entry?.metadata()?.len();
            total
                .checked_add(size)
                .ok_or_else(|| std::io::Error::other("diagnostic size overflow"))
        })?;
        let logs = self.result.stdout.len()
            + self.result.stderr.len()
            + self
                .replay
                .as_ref()
                .map_or(0, |(out, err)| out.len() + err.len())
            + corpus()?.len()
            + 16384;
        let bytes = bytes
            .checked_add(u64::try_from(logs)?)
            .ok_or("diagnostic size overflow")?;
        openpilot_dev_tools::disk_space::inspect(&path, bytes)?.require()?;
        fs::create_dir_all(&path)?;
        fs::write(
            path.join("case.json"),
            serde_json::to_vec_pretty(self.case)?,
        )?;
        fs::write(path.join("native.stdout.log"), &self.result.stdout)?;
        fs::write(path.join("native.stderr.log"), &self.result.stderr)?;
        if let Some((stdout, stderr)) = &self.replay {
            fs::write(path.join("replay.stdout.log"), stdout)?;
            fs::write(path.join("replay.stderr.log"), stderr)?;
        }
        for (input, output) in [
            ("out.png", "actual.png"),
            ("out.json", "actual.json"),
            ("original-draw-replay.png", "original-draw-replay.png"),
        ] {
            let from = self.directory.join(input);
            if from.exists() {
                fs::copy(from, path.join(output))?;
            }
        }
        fs::write(path.join("original-corpus.json"), corpus()?)?;
        let env = self
            .command
            .get_envs()
            .filter_map(|(key, value)| {
                let key = key.to_str()?;
                matches!(
                    key,
                    "GRID"
                        | "PROFILE_RENDER"
                        | "PROFILE_STATS"
                        | "RECORD"
                        | "RECORD_OUTPUT"
                        | "OFFSCREEN"
                        | "BURN_IN"
                        | "PROFILE_STARTUP"
                        | "ENABLE_VSYNC"
                        | "SHOW_FPS"
                        | "SHOW_TOUCHES"
                        | "STRICT_MODE"
                        | "STARTUP_UI_RAYLIB_LIBRARY"
                )
                .then(|| {
                    (
                        key.to_owned(),
                        value.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let inherited = [
            "DISPLAY",
            "LIBGL_ALWAYS_SOFTWARE",
            "GALLIUM_DRIVER",
            "GALLIUM_OVERRIDE_CPU_CAPS",
            "LP_NATIVE_VECTOR_WIDTH",
            "LP_NUM_THREADS",
            "LANG",
            "LC_ALL",
            "LC_NUMERIC",
        ]
        .into_iter()
        .map(|key| {
            (
                key,
                std::env::var_os(key).map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
        fs::write(
            path.join("context.json"),
            serde_json::to_vec_pretty(&json!({
                "exit":self.result.status.code(),"program":self.command.get_program().to_string_lossy(),
                "args":self.command.get_args().map(|v|v.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                "case_environment":env,"renderer_environment":inherited,
                "capture_sha256":digest(corpus()?.as_bytes()),
                "scope":"Failure diagnostics only; artifact hashes are observed after failure, not an executing-ELF attestation"
            }))?,
        )?;
        Ok(())
    }
}
impl Drop for FailureEvidence<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        if let Err(error) = self.retain() {
            eprintln!("could not retain application failure diagnostics: {error}");
        }
    }
}
