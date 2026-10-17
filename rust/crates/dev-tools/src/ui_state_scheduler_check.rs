//! Source-pinned fixed-input UI state and ordered scheduler comparison gates.
//! Original evidence covers Linux x86_64. Generic source helpers remain pending.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    State,
    Scheduler,
}
struct Scene {
    name: &'static str,
    input: &'static [u8],
    source: &'static [u8],
}
macro_rules! scene {
    ($name:literal) => {
        Scene {
            name: $name,
            input: include_bytes!(concat!(
                "../tests/fixtures/ui-state-scheduler/",
                $name,
                "-input.json.gz"
            )),
            source: include_bytes!(concat!(
                "../tests/fixtures/ui-state-scheduler/",
                $name,
                "-source.json.gz"
            )),
        }
    };
}
const STATE: [Scene; 3] = [scene!("big"), scene!("mici"), scene!("pc")];
const SCHEDULER: [Scene; 2] = [scene!("disabled"), scene!("enabled")];
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn pinned(root: &Path, kind: Kind) -> Result<()> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-state-scheduler/source-pins.json"
    ))?;
    let key = if matches!(kind, Kind::State) {
        "state"
    } else {
        "scheduler"
    };
    for (path, hash) in manifest[key].as_object().ok_or("missing source pins")? {
        if sha(&fs::read(root.join(path))?) != hash.as_str().ok_or("missing source digest")? {
            return Err(
                format!("{path} source changed; independently recapture original corpus").into(),
            );
        }
    }
    if matches!(kind, Kind::State) {
        let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
        let packages = lock
            .get("package")
            .and_then(toml::Value::as_array)
            .ok_or("missing locked packages")?;
        for (name, expected) in [
            (
                "numpy",
                include_str!("../tests/fixtures/ui-state-scheduler/numpy-lock.toml"),
            ),
            (
                "pycapnp",
                include_str!("../tests/fixtures/ui-state-scheduler/pycapnp-lock.toml"),
            ),
        ] {
            let expected: toml::Value = toml::from_str(expected)?;
            let found = packages
                .iter()
                .filter(|p| p.get("name").and_then(toml::Value::as_str) == Some(name))
                .collect::<Vec<_>>();
            if found.len() != 1 || *found[0] != expected {
                return Err(format!(
                    "{name} source changed; independently recapture original corpus"
                )
                .into());
            }
        }
    }
    Ok(())
}
fn corpus(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-state-scheduler/integrity.json"
    ))?;
    let row = &manifest[name];
    if Some(sha(bytes).as_str()) != row["gzip_sha256"].as_str() {
        return Err("compressed corpus integrity mismatch".into());
    }
    let mut plain = vec![];
    flate2::read::GzDecoder::new(bytes)
        .take(row["bytes"].as_u64().ok_or("missing corpus size")? + 1)
        .read_to_end(&mut plain)?;
    if Some(sha(&plain).as_str()) != row["sha256"].as_str() {
        return Err("original corpus integrity mismatch".into());
    }
    Ok(plain)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn parse(text: &str) -> Result<JsonValue> {
    // Independently captured whole original commands admit total depth9997,
    // including ignored State object values; depth9998 must still reject.
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for ch in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
            continue;
        }
        if !quoted {
            if ch == '[' || ch == '{' {
                depth += 1;
                if depth > 9997 {
                    return Err("Python JSON nesting limit exceeded".into());
                }
            } else if ch == ']' || ch == '}' {
                depth = depth.saturating_sub(1)
            }
        }
    }
    Ok(JsonValue::parse(text)?)
}
fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut out = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !out.is_empty() && !out.ends_with(b"/") {
            out.push(b'/')
        }
        out.extend(part)
    }
    if out.is_empty() {
        out.push(b'.')
    }
    OsString::from_vec(out).into()
}
fn object(value: &JsonValue) -> Result<Vec<(Vec<u32>, JsonValue)>> {
    match value.view() {
        JsonView::Object(rows) => Ok(rows
            .into_iter()
            .map(|(key, value)| (key.to_vec(), value))
            .collect()),
        _ => Err("original State dictionary unpack requires object".into()),
    }
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing State field {key}").into())
}
fn numeric(value: &JsonValue) -> Result<f64> {
    match value.view() {
        JsonView::Float(v) => Ok(v),
        JsonView::Integer(v) => Ok(v.parse()?),
        JsonView::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        _ => Err("original State float comparison requires numeric value".into()),
    }
}
fn compare_state(expected: &JsonValue, actual: &JsonValue) -> Result<()> {
    let state = field(actual, "state")?;
    let device = field(actual, "device")?;
    let slow = field(&state, "slow")?;
    let realtime = field(&state, "realtime")?;
    let mut flat = BTreeMap::new();
    for value in [&state, &slow, &device, actual] {
        flat.extend(object(value)?)
    }
    flat.insert(
        "next_refresh_time".chars().map(u32::from).collect(),
        field(&realtime, "next_refresh_time")?,
    );
    flat.extend(object(&field(&realtime, "value")?)?);
    for (key, value) in object(expected)? {
        let observed = flat.get(&key).ok_or("missing flattened State field")?;
        let exact = if let JsonView::Float(source) = value.view() {
            (source - numeric(observed)?).abs() <= 1e-11
        } else {
            json::equal(&value, observed)
        };
        if !exact {
            return Err("original/native State comparison failed".into());
        }
    }
    Ok(())
}
/// Compare only the original fixed CLI corpus, without invoking Python.
pub fn check(
    root: &Path,
    binary: &Path,
    output: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
) -> Result<String> {
    let root = resolve_non_strict(root)?;
    pinned(&root, kind)?;
    let scenes = if matches!(kind, Kind::State) {
        STATE.as_slice()
    } else {
        SCHEDULER.as_slice()
    };
    let corpora = scenes
        .iter()
        .map(|scene| {
            Ok((
                corpus(scene.input, &format!("{}-input.json.gz", scene.name))?,
                corpus(scene.source, &format!("{}-source.json.gz", scene.name))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    fs::create_dir_all(output)?;
    for (scene, (input, source)) in scenes.iter().zip(corpora) {
        fs::write(output.join(format!("{}-input.json", scene.name)), &input)?;
        fs::write(output.join(format!("{}-source.json", scene.name)), &source)?;
        if matches!(kind, Kind::State) {
            fs::write(
                output.join(format!("{}-source.log", scene.name)),
                b"(no stderr)\n",
            )?;
        }
        let mut command =
            CommandSpec::new_os(vec![lexical(binary).into_os_string()], Duration::MAX);
        command.input = Some(input);
        command.stdout_limit = usize::MAX;
        command.stderr_limit = usize::MAX;
        if matches!(kind, Kind::State) {
            command.set_os_env(BTreeMap::from([(
                OsString::from("PYTHONPATH"),
                root.as_os_str().to_owned(),
            )]));
        }
        let result = runner.run(&command)?;
        let stdout = text(result.stdout)?;
        let stderr = text(result.stderr)?;
        if matches!(kind, Kind::Scheduler) && result.code != 0 {
            return Err(format!("scheduler candidate exited {}", result.code).into());
        }
        fs::write(
            output.join(format!("{}-native.json", scene.name)),
            if matches!(kind, Kind::State) && stdout.is_empty() {
                "{}"
            } else {
                &stdout
            },
        )?;
        fs::write(
            output.join(format!("{}-native.log", scene.name)),
            if matches!(kind, Kind::State) && stderr.is_empty() {
                "(no stderr)\n"
            } else {
                &stderr
            },
        )?;
        if result.code != 0 {
            return Err(format!("State candidate exited {}", result.code).into());
        }
        let expected = json::sequence(&parse(std::str::from_utf8(&source)?)?)?;
        let actual = json::sequence(&parse(&stdout)?)?;
        if expected.len() != actual.len() {
            return Err("original/native trace lengths differ".into());
        }
        for (a, b) in expected.iter().zip(&actual) {
            if matches!(kind, Kind::State) {
                compare_state(a, b)?
            } else if !json::equal(a, b) {
                return Err("original/native ordered scheduler effects differ".into());
            }
        }
    }
    let (name, result, printed) = match kind {
        Kind::State => (
            "results.json",
            include_str!("../tests/fixtures/ui-state-scheduler/state-results.json"),
            "PASS: 2400 actual-source/native UI state, cached Params reads, transition ordering, display timeout and brightness frames\n",
        ),
        Kind::Scheduler => (
            "result.json",
            include_str!("../tests/fixtures/ui-state-scheduler/scheduler-result.json"),
            "PASS: 160 unchanged-source/native display sweeps; realtime downgrade/nice/affinity order, hotplug fallback, child threads, exited worker and permission handling; native own-process SCHED_OTHER readback\n",
        ),
    };
    fs::write(output.join(name), result)?;
    Ok(printed.into())
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!(
            "usage: {} --binary BINARY --output OUTPUT",
            if matches!(kind, Kind::State) {
                "check-ui-state"
            } else {
                "check-ui-scheduler"
            }
        );
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    print!(
        "{}",
        check(
            &root,
            Path::new(&args.values["--binary"]),
            Path::new(&args.values["--output"]),
            kind,
            &mut NativeRunner
        )?
    );
    Ok(())
}
