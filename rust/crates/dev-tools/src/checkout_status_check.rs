//! Independent checkout development checker. All scenarios execute real peers.
use crate::{argparse_scan::ArgumentError, seal::Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
mod args;
mod git;
mod helpers;
mod metadata;
pub use metadata::{inputs as metadata_inputs, MetadataInput};
mod peer;
mod status;
use peer::{Binaries, Peer};
#[derive(Clone, Copy)]
pub enum Group {
    All,
    Metadata,
    Git,
    Status,
    Helper,
}
pub fn scenario_names(group: Group) -> &'static [&'static str] {
    match group {
        Group::All => &[
            "metadata",
            "git-sha1",
            "git-sha256",
            "git-file",
            "git-executable",
            "packaged-status",
            "git-status",
            "native-helper",
            "inherited-stdio",
        ],
        Group::Metadata => &["metadata"],
        Group::Git => &["git-sha1", "git-sha256", "git-file", "git-executable"],
        Group::Status => &["packaged-status", "git-status"],
        Group::Helper => &["native-helper", "inherited-stdio"],
    }
}
pub(crate) fn require(ok: bool, detail: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(detail.into().into())
    }
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/checkout-status/manifest.json");
pub fn validate_source(root: &Path) -> Result<Value> {
    require(
        hash(MANIFEST) == "13056a985e158f75002789fab64568ef6d28e3fa63e0db7a4f9b90729be59de1",
        "checkout original manifest changed",
    )?;
    let data: Value = serde_json::from_slice(MANIFEST)?;
    let pins = data["source_sha256"]
        .as_object()
        .ok_or("checkout source pins missing")?;
    require(pins.len() == 8, "checkout source pin set changed")?;
    for (name, expected) in pins {
        match fs::read(root.join(name)) {
            Ok(bytes) => require(
                Some(hash(&bytes).as_str()) == expected.as_str(),
                format!("checkout source changed: {name}"),
            )?,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && name.starts_with("rust/tools/") => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(data["source_sha256"].clone())
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub fixture: PathBuf,
    pub launcher: PathBuf,
    pub reference: PathBuf,
    pub captured: Option<PathBuf>,
    pub handshake: Option<PathBuf>,
    pub output: PathBuf,
    pub group: Group,
}
// Keep raw Unix paths as Python filesystem strings, including surrogateescape.
// serde_json::Value cannot represent these strings without losing their bytes.
fn executable_receipt(name: &str, path: &Path) -> Result<String> {
    // Unavailable identities must not move a child's original failure phase.
    let identity = match fs::read(path) {
        Ok(bytes) => format!("\"sha256\":{}", serde_json::to_string(&hash(&bytes))?),
        Err(error) => format!(
            "\"unavailable\":{}",
            serde_json::to_string(&error.to_string())?
        ),
    };
    Ok(format!(
        "{}:{{\"path\":{},{identity}}}",
        serde_json::to_string(name)?,
        crate::python_fs::json_string(path.as_os_str())?
    ))
}
pub fn run(options: &Options, out: &mut dyn Write) -> Result<Value> {
    let sources = validate_source(&options.root)?;
    rustix::process::set_child_subreaper(Some(rustix::process::getpid()))?;
    fs::create_dir_all(&options.output)?;
    if matches!(options.group, Group::Helper)
        && (options.captured.is_none() || options.handshake.is_none())
    {
        return Err(args::error(
            "all/helper checks require --captured-binary and --handshake-fixture",
        )
        .into());
    }
    let binaries = Binaries {
        trace: options.binary.clone(),
        fixture: options.fixture.clone(),
        launcher: options.launcher.clone(),
        reference: options.reference.clone(),
    };
    let mut receipts = Vec::new();
    for (name, path) in [
        ("checker", std::env::current_exe()?),
        ("reference", options.reference.clone()),
        ("candidate", options.binary.clone()),
        ("git_fixture", options.fixture.clone()),
        ("launcher", options.launcher.clone()),
    ] {
        receipts.push(executable_receipt(name, &path)?);
    }
    for (name, path) in [
        ("captured", &options.captured),
        ("handshake", &options.handshake),
    ] {
        if let Some(path) = path {
            receipts.push(executable_receipt(name, path)?);
        }
    }
    fs::write(
        options.output.join("provenance.json"),
        format!(
            "{{\"original_sources\":{sources},\"native_executables\":{{{}}},\"reference\":\"independent source-derived native policy; no candidate crate import\"}}\n",
            receipts.join(",")
        ),
    )?;
    let mut results = Vec::new();
    for &name in scenario_names(options.group) {
        if matches!(name, "native-helper" | "inherited-stdio") {
            let (Some(captured), Some(handshake)) = (&options.captured, &options.handshake) else {
                return Err(args::error(
                    "all/helper checks require --captured-binary and --handshake-fixture",
                )
                .into());
            };
            let tools = helpers::Helpers {
                captured: captured.clone(),
                handshake: handshake.clone(),
            };
            let value = if name == "native-helper" {
                helpers::native(&binaries, &tools, &options.output.join(name))?
            } else {
                helpers::inherited(&binaries, &tools, &options.output.join("inherited-helper"))?
            };
            results.push(json!({"scenario":name,"passed":true,"results":value}));
        } else {
            let mut pair = Vec::new();
            for implementation in ["reference", "rust"] {
                let mut peer = Peer::new(
                    &binaries,
                    implementation,
                    &options.output.join(name),
                    matches!(name, "git-executable" | "git-status"),
                )?;
                let result = match name {
                    "metadata" => metadata::run(&mut peer),
                    "git-sha1" => git::real(&mut peer, "sha1"),
                    "git-sha256" => git::real(&mut peer, "sha256"),
                    "git-file" => git::file(&mut peer),
                    "git-executable" => git::fixture(&mut peer),
                    "packaged-status" => status::packaged(&mut peer),
                    "git-status" => status::fixture(&mut peer),
                    _ => return Err("unknown checkout scenario".into()),
                };
                let close = peer.close();
                let result = result?;
                close?;
                pair.push(json!({"implementation":implementation,"result":result}));
            }
            require(
                pair[0]["result"] == pair[1]["result"],
                format!("checkout {name} source/candidate results differ: {pair:?}"),
            )?;
            results.push(json!({"scenario":name,"passed":true,"results":pair}));
            fs::write(
                options.output.join("summary.json"),
                serde_json::to_vec_pretty(&json!({"passed":false,"results":results}))?,
            )?;
        }
        writeln!(out, "PASS: {name}")?;
        out.flush()?;
    }
    let summary = json!({"passed":true,"scenarios":results.len(),"results":results});
    fs::write(
        options.output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(summary)
}
pub fn cli() -> Result<()> {
    let Some(values) = args::parse(&std::env::args_os().skip(1).collect::<Vec<_>>())? else {
        print!("{}", args::help());
        return Ok(());
    };
    let group = match values
        .get("--group")
        .and_then(|v| v.to_str())
        .unwrap_or("all")
    {
        "all" => Group::All,
        "metadata" => Group::Metadata,
        "git" => Group::Git,
        "status" => Group::Status,
        "helper" => Group::Helper,
        _ => return Err("validated group missing".into()),
    };
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    run(
        &Options {
            root,
            binary: values["--binary"].clone().into(),
            fixture: values["--fixture"].clone().into(),
            launcher: values["--launcher"].clone().into(),
            reference: std::env::current_exe()?.with_file_name("checkout-status-reference"),
            captured: values.get("--captured-binary").map(PathBuf::from),
            handshake: values.get("--handshake-fixture").map(PathBuf::from),
            output: values["--output"].clone().into(),
            group,
        },
        &mut std::io::stdout(),
    )?;
    Ok(())
}
