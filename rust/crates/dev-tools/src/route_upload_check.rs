//! Composed native manager/runtime route recording and real loopback uploads.
//! Expected observations come from the historical QA controllers, not daemon policy.
mod http;
mod runtime;
use crate::{
    argparse_scan::{self, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
pub use http::Request;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub binaries: PathBuf,
    pub probe: Option<PathBuf>,
    pub output: PathBuf,
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
pub fn route_records(compressed: &[u8], service: &str) -> Result<(usize, Vec<Value>)> {
    let (_, decoded) = crate::bootlog_artifact_check::decode_zstd(compressed)?;
    let events = decoded.as_array().ok_or("route event array missing")?;
    let records = events
        .iter()
        .filter_map(|row| row.get(service))
        .map(|raw| {
            Ok(serde_json::from_str(
                raw.as_str().ok_or("route log record is not text")?,
            )?)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((events.len(), records))
}
pub fn runtime_crash(records: &[Value]) -> Result<&Value> {
    let crashes = records
        .iter()
        .filter(|row| row["msg"] == "crash")
        .collect::<Vec<_>>();
    require(
        crashes.len() == 1,
        "expected exactly one managed uploader crash",
    )?;
    let crash = crashes[0];
    let ctx = &crash["ctx"];
    require(
        ctx["runtime_language"] == "rust"
            && ctx["runtime_implementation"] == "openpilot-uploader"
            && ctx["daemon"] == "uploader"
            && ctx["dongle_id"] == "owned-device",
        "managed uploader crash identity changed",
    )?;
    let commit = ctx["source_commit"]
        .as_str()
        .ok_or("crash source commit missing")?;
    require(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid crash source commit",
    )?;
    require(
        matches!(ctx["source_tree"].as_str(), Some("clean" | "dirty")),
        "invalid crash source tree",
    )?;
    require(
        crash["exc_info"]
            .as_str()
            .is_some_and(|text| text.contains("openpilot_uploader::")),
        "native uploader error trace missing",
    )?;
    Ok(crash)
}
pub fn stats_records(records: &[Value], errors: &[Value], commit: &str) -> Result<(usize, usize)> {
    let native = records
        .iter()
        .filter(|row| row["ctx"]["daemon"] == "statsd")
        .collect::<Vec<_>>();
    require(
        !native.is_empty(),
        "managed child log absent from native route",
    )?;
    for row in &native {
        require(
            row["ctx"]["runtime_implementation"] == "statsd-rs"
                && row["ctx"]["runtime_language"] == "rust"
                && row["ctx"]["commit"] == commit,
            "managed child log identity or source changed",
        )?;
    }
    let crashes = errors
        .iter()
        .filter(|row| row["msg"] == "crash" && row["ctx"]["daemon"] == "statsd")
        .collect::<Vec<_>>();
    require(
        crashes.len() == 1,
        "expected exactly one statsd crash in qlog",
    )?;
    require(
        crashes[0]["ctx"]["runtime_language"] == "rust"
            && crashes[0]["ctx"]["runtime_implementation"] == "statsd-rs",
        "managed stats crash identity changed",
    )?;
    Ok((native.len(), crashes.len()))
}
pub fn compare_transfers(
    captures: &[Request],
    qlog_path: &str,
    qlog_bytes: &[u8],
) -> Result<Vec<Value>> {
    require(
        captures.len().is_multiple_of(2),
        "unpaired upload HTTP exchange",
    )?;
    let mut found = false;
    let mut transfers = Vec::new();
    for pair in captures.chunks_exact(2) {
        require(
            pair[0].method == "GET" && pair[1].method == "PUT",
            "upload request/transfer order changed",
        )?;
        let url = url::Url::parse(&format!("http://127.0.0.1{}", pair[0].path))?;
        let paths = url
            .query_pairs()
            .filter(|(key, _)| key == "path")
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        let path = paths.first().ok_or("upload path query missing")?;
        if path == qlog_path {
            require(pair[1].body == qlog_bytes, "uploaded qlog bytes changed")?;
            found = true;
        }
        transfers
            .push(json!({"path":path,"bytes":pair[1].body.len(),"sha256":hash(&pair[1].body)}));
    }
    require(found, "qlog was not transferred")?;
    Ok(transfers)
}
fn environment() -> BTreeMap<OsString, OsString> {
    let mut env: BTreeMap<_, _> = std::env::vars_os().collect();
    for name in [
        "FAKEUPLOAD",
        "MANAGER_DAEMON",
        "OPENPILOT_LOG_CONTEXT",
        "ZMQ",
        "CEREAL_FAKE",
    ] {
        env.remove(std::ffi::OsStr::new(name));
    }
    env
}
fn spec(argv: Vec<OsString>, env: &BTreeMap<OsString, OsString>, timeout: u64) -> CommandSpec {
    let mut command = CommandSpec::new_os(argv, Duration::from_secs(timeout));
    command.set_os_env(env.clone());
    // Remove inherited variables as well: CommandSpec overlays the caller env.
    command.env_remove = [
        "FAKEUPLOAD",
        "MANAGER_DAEMON",
        "OPENPILOT_LOG_CONTEXT",
        "ZMQ",
        "CEREAL_FAKE",
    ]
    .map(OsString::from)
    .to_vec();
    command
}
fn capture(command: &CommandSpec, path: &Path) -> Result<crate::qa_process::Captured> {
    NativeRunner.run_to_file(command, path)
}
fn qlog(routes: &Path) -> Result<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(routes)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path().join("qlog.zst");
            if path.is_file() {
                files.push(path);
            }
            for file in fs::read_dir(entry.path())? {
                require(
                    file?.path().extension().is_none_or(|ext| ext != "lock"),
                    "route lock remains after logger exit",
                )?;
            }
        }
    }
    require(files.len() == 1, "expected one sealed route qlog")?;
    Ok(files.remove(0))
}
fn binary_hashes(binaries: &Path, names: &[&str]) -> Result<Value> {
    let mut hashes = serde_json::Map::new();
    for name in names {
        hashes.insert((*name).into(), json!(digest(&binaries.join(name))?));
    }
    Ok(Value::Object(hashes))
}
fn marked(path: &Path) -> Result<()> {
    let mut bytes = [0; 8];
    let count = rustix::fs::getxattr(path, "user.upload", &mut bytes[..])?;
    require(&bytes[..count] == b"1", "qlog was not marked uploaded")
}
fn upload(
    options: &Options,
    env: &BTreeMap<OsString, OsString>,
    cycles: &str,
    seconds: u64,
    output: &Path,
    qlog: &Path,
    routes: &Path,
) -> Result<Vec<Value>> {
    let server = http::Server::new()?;
    let mut env = env.clone();
    env.insert("API_HOST".into(), server.url.clone().into());
    env.insert("FORCEWIFI".into(), "1".into());
    env.insert("UPLOADER_SLEEP".into(), "0".into());
    let mut command = spec(
        vec![
            options.binaries.join("openpilot-uploader").into_os_string(),
            "--cycles".into(),
            cycles.into(),
        ],
        &env,
        seconds,
    );
    command.cwd = Some(options.root.clone());
    let result = capture(&command, &output.join("uploader.log"))?;
    require(result.code == 0, "native uploader failed; see uploader.log")?;
    let captures = server.finish()?;
    let path = qlog
        .strip_prefix(routes)?
        .to_str()
        .ok_or("non-UTF8 synthetic qlog path")?;
    let transfers = compare_transfers(&captures, path, &fs::read(qlog)?)?;
    marked(qlog)?;
    let bodies = output.join("http-bodies");
    fs::create_dir(&bodies)?;
    let mut transcript = Vec::new();
    for (index, row) in captures.iter().enumerate() {
        let body_file = format!("http-bodies/request-{index}.bin");
        fs::write(output.join(&body_file), &row.body)?;
        transcript.push(json!({"method":row.method,"path":row.path,"bytes":row.body.len(),"sha256":hash(&row.body),"body_file":body_file}));
    }
    write_json(
        &output.join("http-captures.json"),
        &Value::Array(transcript),
    )?;
    Ok(transfers)
}
pub fn manager(options: &Options) -> Result<String> {
    crate::disk_space::inspect(&options.root, 1024 * 1024 * 1024)?.require()?;
    fs::create_dir_all(options.output.parent().ok_or("output parent missing")?)?;
    fs::create_dir(&options.output)?;
    let managed = options.output.join("manager");
    let probe = options.probe.as_ref().ok_or("manager probe missing")?;
    let argv = vec![
        probe.clone().into_os_string(),
        managed.clone().into_os_string(),
        options.root.clone().into_os_string(),
        options.binaries.clone().into_os_string(),
        "log-route".into(),
    ];
    let result = capture(
        &spec(argv.clone(), &environment(), 90),
        &options.output.join("manager-probe.log"),
    )?;
    require(
        result.code == 0,
        "installed manager probe failed; see manager-probe.log",
    )?;
    let summary: Value = serde_json::from_slice(&fs::read(managed.join("summary.json"))?)?;
    let reaped = summary["native_children_reaped"]
        .as_array()
        .ok_or("native reaped child list missing")?;
    require(reaped.len() == 4, "expected four reaped native children")?;
    let routes = managed.join("logs");
    let qlog = qlog(&routes)?;
    let (messages, records) =
        route_records(&fs::read(qlog.with_file_name("rlog.zst"))?, "logMessage")?;
    let (_, errors) = route_records(&fs::read(&qlog)?, "errorLogMessage")?;
    let commit = summary["git_commit"]
        .as_str()
        .ok_or("manager git commit missing")?;
    let (native, crashes) = stats_records(&records, &errors, commit)?;
    let prefixes = fs::read_dir(managed.join("params"))?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter(|row| {
            row.file_name()
                .to_string_lossy()
                .starts_with("rust-probe-installed-")
        })
        .collect::<Vec<_>>();
    require(
        prefixes.len() == 1,
        "manager Params prefix missing or duplicated",
    )?;
    let prefix = prefixes[0].file_name();
    fs::write(prefixes[0].path().join("DongleId"), b"0000000000000000")?;
    let mut comma = OsString::from(".comma");
    comma.push(&prefix);
    http::signing_key(&managed.join("home").join(comma).join("persist"))?;
    let mut env = environment();
    for (name, value) in [
        ("HOME", managed.join("home").into_os_string()),
        ("OPENPILOT_PREFIX", prefix.clone()),
        ("PARAMS_ROOT", managed.join("params").into_os_string()),
        ("LOG_ROOT", routes.clone().into_os_string()),
        ("OPENPILOT_ROOT", options.root.clone().into_os_string()),
        ("OPENPILOT_BASEDIR", options.root.clone().into_os_string()),
    ] {
        env.insert(name.into(), value);
    }
    let mut namespace_name = OsString::from("msgq_");
    namespace_name.push(prefix);
    let _namespace = runtime::Directory::new(&Path::new("/dev/shm").join(namespace_name))?;
    let transfers = upload(options, &env, "6", 20, &options.output, &qlog, &routes)?;
    let receipt = json!({"result":"PASS","manager_probe_sha256":digest(probe)?,"binaries":binary_hashes(&options.binaries,&["openpilot-manager","openpilot-process-child","openpilot-bootlog","openpilot-logmessaged","statsd-rs","openpilot-loggerd","openpilot-uploader"] )?,"source_commit":commit,"route_messages":messages,"managed_child_route_records":native,"managed_crashes_recorded_and_uploaded":crashes,"failed_child_restarted":true,"native_children_reaped":reaped,"transfers":transfers,"scope":"actual manager, process launcher, bootlog, collector, statsd, route logger and uploader; owned IPC/storage/HTTP; other daemons blocked"});
    let mut encoded = serde_json::to_string(&receipt)?;
    encoded.pop();
    encoded.push_str(&format!(",\"manager_command\":{}}}", command_json(&argv)?));
    fs::write(options.output.join("result.json"), format!("{encoded}\n"))?;
    Ok(encoded)
}
pub fn runtime(options: &Options) -> Result<String> {
    Ok(serde_json::to_string(&runtime::run(options)?)?)
}
pub fn cli(manager_mode: bool) -> Result<()> {
    let mut flags = vec![
        OptionSpec::value("--binaries", true),
        OptionSpec::value("--output", true),
    ];
    if manager_mode {
        flags.extend([
            OptionSpec::value("--root", true),
            OptionSpec::value("--probe", true),
        ]);
    }
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = argparse_scan::scan_os(&args, &flags)?;
    if parsed.help {
        println!(
            "usage: {} --binaries DIR --output DIR{}",
            if manager_mode {
                "check-manager-log-upload"
            } else {
                "check-runtime-log-upload"
            },
            if manager_mode {
                " --root DIR --probe FILE"
            } else {
                ""
            }
        );
        return Ok(());
    }
    let path = |name: &str| -> Result<PathBuf> {
        let value = parsed.values.get(name).ok_or("required argument missing")?;
        crate::path_resolve::resolve_non_strict(Path::new(value))
    };
    let options = Options {
        root: if manager_mode {
            path("--root")?
        } else {
            crate::path_resolve::resolve_non_strict(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            )?
        },
        binaries: path("--binaries")?,
        output: path("--output")?,
        probe: if manager_mode {
            Some(path("--probe")?)
        } else {
            None
        },
    };
    println!(
        "{}",
        if manager_mode {
            manager(&options)?
        } else {
            runtime(&options)?
        }
    );
    Ok(())
}
/// Python's filesystem JSON encoding, including Unix surrogateescape bytes.
pub fn command_json(arguments: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        arguments
            .iter()
            .map(|arg| crate::python_fs::json_string(arg))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_mark_and_route_lock_are_actual_filesystem_observations() {
        let temporary = tempfile::tempdir().unwrap();
        let route = temporary.path().join("route--0");
        fs::create_dir(&route).unwrap();
        let file = route.join("qlog.zst");
        fs::write(&file, b"fixture").unwrap();
        assert!(marked(&file).is_err());
        rustix::fs::setxattr(
            &file,
            "user.upload",
            b"wrong",
            rustix::fs::XattrFlags::empty(),
        )
        .unwrap();
        assert!(marked(&file).is_err());
        rustix::fs::setxattr(&file, "user.upload", b"1", rustix::fs::XattrFlags::empty()).unwrap();
        marked(&file).unwrap();
        assert_eq!(qlog(temporary.path()).unwrap(), file);
        fs::write(route.join("qlog.lock"), b"").unwrap();
        assert!(qlog(temporary.path()).is_err());
    }
}
