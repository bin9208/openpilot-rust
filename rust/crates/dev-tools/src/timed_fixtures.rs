//! Safe command/Params/zoneinfo fixture; all mutations belong to one case root.
//! The command executable is immutable and is linked into each private PATH.
use crate::{python_fs, qa_process::CommandSpec, seal::Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub directory: PathBuf,
    pub params: PathBuf,
    pub prefix: String,
    pub config: Value,
    pub environment: BTreeMap<OsString, OsString>,
}
pub fn replace_root(value: &Value, from: &str, to: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(from, to)),
        Value::Array(rows) => {
            Value::Array(rows.iter().map(|v| replace_root(v, from, to)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), replace_root(v, from, to)))
                .collect(),
        ),
        value => value.clone(),
    }
}
pub fn default_config(root: &Path) -> Value {
    json!({"params_root":root,"paths":{"zoneinfo":root.join("zoneinfo"),"localtime":root.join("etc/localtime"),"systemd":root.join("systemd")},
        "endpoint":"http://127.0.0.1:1/unused","wall":1790000000000000000u64,"monotonic":0,"actions":[]})
}
impl Fixture {
    pub fn new(directory: &Path, timezone: &str, command: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let directory = fs::canonicalize(directory)?;
        let prefix = format!(
            "timed_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let params = directory.join(&prefix);
        fs::create_dir(&params)?;
        let zoneinfo = directory.join("zoneinfo");
        for zone in [
            "Asia/Seoul",
            "Etc/GMT",
            "Etc/GMT-9",
            "Etc/GMT+12",
            "Etc/GMT-14",
            "America/New_York",
        ] {
            let path = zoneinfo.join(zone);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, b"synthetic timezone file, never installed on host")?;
        }
        let bin = directory.join("bin");
        fs::create_dir(&bin)?;
        symlink(fs::canonicalize(command)?, bin.join("sudo"))?;
        let environment = BTreeMap::from([
            ("OPENPILOT_PREFIX".into(), OsString::from(&prefix)),
            ("PARAMS_ROOT".into(), directory.as_os_str().to_owned()),
            (
                "TIMED_FIXTURE_ROOT".into(),
                directory.as_os_str().to_owned(),
            ),
            ("PATH".into(), bin.into_os_string()),
            ("TZ".into(), timezone.into()),
        ]);
        let config = default_config(&directory);
        Ok(Self {
            directory,
            params,
            prefix,
            config,
            environment,
        })
    }
    pub fn prepare(&mut self, request: &Value) -> Result<()> {
        let root = self
            .directory
            .to_str()
            .ok_or("timed fixture root must be UTF-8")?;
        self.config = replace_root(&request["config"], "<root>", root);
        let initial = &request["initial"];
        if let Some(params) = initial["params_hex"].as_object() {
            for (name, bytes) in params {
                let text = bytes.as_str().ok_or("invalid initial Params bytes")?;
                if text.len() % 2 != 0 {
                    return Err("odd initial Params hex".into());
                }
                let bytes = text
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
                    .collect::<Result<Vec<_>>>()?;
                fs::write(self.params.join(name), bytes)?;
            }
        }
        if let Some(command) = initial["command_fail"].as_str() {
            self.environment
                .insert("TIMED_COMMAND_FAIL".into(), command.into());
        }
        if initial["sudo_exists"] == false {
            fs::remove_file(self.directory.join("bin/sudo"))?;
        }
        if let Some(target) = initial["localtime"].as_str() {
            let link = PathBuf::from(
                self.config["paths"]["localtime"]
                    .as_str()
                    .ok_or("missing localtime")?,
            );
            fs::create_dir_all(link.parent().ok_or("localtime parent")?)?;
            symlink(target.replace("<root>", root), link)?;
        }
        if let Some(nanos) = initial["systemd_mtime_ns"].as_i64() {
            let systemd = self.directory.join("systemd");
            fs::write(&systemd, b"")?;
            let time = rustix::fs::Timespec {
                tv_sec: nanos.div_euclid(1_000_000_000),
                tv_nsec: nanos.rem_euclid(1_000_000_000),
            };
            rustix::fs::utimensat(
                rustix::fs::CWD,
                systemd,
                &rustix::fs::Timestamps {
                    last_access: time,
                    last_modification: time,
                },
                rustix::fs::AtFlags::empty(),
            )?;
        }
        Ok(())
    }
    pub fn configure(&self, spec: &mut CommandSpec) {
        spec.set_os_env(self.environment.clone());
        spec.env_remove = ["ZMQ", "CEREAL_FAKE", "LOGPRINT"]
            .map(OsString::from)
            .to_vec();
        if !self
            .environment
            .contains_key(&OsString::from("TIMED_COMMAND_FAIL"))
        {
            spec.env_remove.push("TIMED_COMMAND_FAIL".into());
        }
    }
}
pub fn commands(root: &Path) -> Result<Vec<Value>> {
    match fs::read_to_string(root.join("commands.jsonl")) {
        Ok(text) => text
            .lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}

#[cfg(feature = "timed-tools-native")]
fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = vec![];
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            lines.push(&text[start..index]);
            start = index + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, next)| *next == '\n') {
                let (index, _) = chars.next().unwrap();
                start = index + 1;
            }
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Same subprocess decoding and artifact phases as timed_fixtures.native.
/// The collector is real ZMQ; only the candidate runner is injectable in tests.
#[cfg(feature = "timed-tools-native")]
pub fn native(
    fixture: &Fixture,
    binary: &Path,
    output: &Path,
    runner: &mut dyn crate::qa_process::Runner,
) -> Result<(
    Vec<openpilot_log_format::JsonValue>,
    Vec<openpilot_log_format::JsonValue>,
)> {
    use crate::{control_tools_policy_check::json, qa_process::CommandSpec};
    use openpilot_log_format::JsonValue;
    use std::time::Duration;
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL)?;
    collector.set_linger(0)?;
    let endpoint = format!("/tmp/logmessage{}", fixture.prefix);
    collector.bind(&format!("ipc://{endpoint}"))?;
    let result = (|| {
        let binary = lexical(binary);
        let mut spec = CommandSpec::new_os(vec![binary.into_os_string()], Duration::from_secs(30));
        fixture.configure(&mut spec);
        let input = JsonValue::parse(&serde_json::to_string(&fixture.config)?)?.to_json()?;
        spec.input = Some(format!("{input}\n").into_bytes());
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        let captured = runner.run(&spec)?;
        // subprocess.run(text=True) decodes BOTH streams before returning.
        // Invalid bytes or timeout therefore leave no output artifacts.
        let stdout = std::str::from_utf8(&captured.stdout)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        let stderr = std::str::from_utf8(&captured.stderr)?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        fs::write(output, &stdout)?;
        fs::write(output.with_extension("stderr"), &stderr)?;
        if captured.code != 0 {
            return Err(format!("timed fixture exited {}: {stderr}", captured.code).into());
        }
        let mut records = vec![];
        while collector.poll(zmq::POLLIN, 50)? != 0 {
            let raw = collector.recv_bytes(0)?;
            records.push(json::parse(raw.get(1..).unwrap_or_default())?);
        }
        let logs = if records.is_empty() {
            "[]\n".into()
        } else {
            let entries = records
                .iter()
                .map(|row| Ok(format!("  {}", json::pretty(row, 1)?)))
                .collect::<Result<Vec<_>>>()?;
            format!("[\n{}\n]\n", entries.join(",\n"))
        };
        fs::write(output.with_extension("logs.json"), logs)?;
        let rows = splitlines(&stdout)
            .into_iter()
            .map(json::parse_text)
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, records))
    })();
    drop(collector);
    drop(context);
    // This path was bound by this call under its unique fixture prefix. Never
    // remove unrelated namespaces, and finish socket ownership before unlink.
    let _ = fs::remove_file(endpoint);
    result
}

/// pathlib.Path lexical construction, without resolving links or '..'.
pub(crate) fn lexical(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let mut output = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !output.is_empty() && !output.ends_with(b"/") {
            output.push(b'/');
        }
        output.extend_from_slice(part);
    }
    if output.is_empty() {
        output.push(b'.');
    }
    OsString::from_vec(output).into()
}
pub fn command_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let root = crate::path_resolve::resolve_non_strict(Path::new(
        &std::env::var_os("TIMED_FIXTURE_ROOT").ok_or("missing TIMED_FIXTURE_ROOT")?,
    ))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("commands.jsonl"))?;
    let values = args
        .iter()
        .map(|a| python_fs::json_string(a))
        .collect::<Result<Vec<_>>>()?;
    let record = format!("[{}]\n", values.join(", "));
    file.write_all(record.as_bytes())?;
    drop(file);
    let command = args.first().ok_or("IndexError: list index out of range")?;
    if Some(command) == std::env::var_os("TIMED_COMMAND_FAIL").as_ref() {
        std::process::exit(7);
    }
    let assert = || -> Result<()> { Err("AssertionError".into()) };
    let argument = |index: usize| -> Result<&OsString> {
        args.get(index)
            .ok_or_else(|| "IndexError: list index out of range".into())
    };
    let inside = |path: &OsString| {
        use std::os::unix::ffi::OsStrExt;
        // PurePosixPath retains an exactly-double-leading-slash anchor, which
        // differs from the resolved root's single '/' anchor.
        let bytes = path.as_bytes();
        (!bytes.starts_with(b"//") || bytes.starts_with(b"///"))
            && Path::new(path).starts_with(&root)
    };
    match command.to_str() {
        Some("date") => {
            use std::os::unix::ffi::OsStrExt;
            // Python indexes only the first two operands; trailing args are
            // ignored. Short-circuit the assertion before accessing args[2].
            if argument(1)? != "-s" || !argument(2)?.as_bytes().starts_with(b"@") {
                return assert();
            }
            Ok(())
        }
        Some("rm") => {
            let target = argument(2)?;
            if !inside(target) || argument(1)? != "-f" {
                return assert();
            }
            match fs::remove_file(target) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
        Some("ln") => {
            if args.len() != 4 {
                return Err("ValueError: expected exactly two ln paths".into());
            }
            if !inside(&args[2]) || !inside(&args[3]) || argument(1)? != "-s" {
                return assert();
            }
            // pathlib.Path construction removes redundant separators/dots but
            // deliberately preserves '..'; no target resolution is performed.
            let target = lexical(Path::new(&args[2]));
            let link = lexical(Path::new(&args[3]));
            symlink(target, link)?;
            Ok(())
        }
        _ => assert(),
    }
}
