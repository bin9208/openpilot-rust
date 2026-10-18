//! Original save_bootlog/native snapshot QA through actual files, xattrs,
//! threads and children. No Python execution or candidate snapshot oracle.
use crate::{path_resolve::resolve_non_strict, seal::Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    io::{BufRead, Write},
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};
#[path = "bootlog_snapshot_process.rs"]
mod process;
pub use process::Process;
const CORPUS: &[u8] = include_bytes!("../tests/fixtures/bootlog-snapshot/original.json");
const CORPUS_SHA: &str = "af18f39ad3e83c9c823a4beca7917a32c69452a172dfc465e91b6b91d50a6743";
const RETIRED: [&str; 2] = [
    "rust/tools/check_bootlog_snapshot.py",
    "rust/tools/bootlog_snapshot_source.py",
];
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
pub fn root() -> Result<PathBuf> {
    resolve_non_strict(
        &std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
    )
}
pub fn sources(root: &Path) -> Result<Value> {
    require(
        sha(CORPUS) == CORPUS_SHA,
        "bootlog original capture changed",
    )?;
    let value: Value = serde_json::from_slice(CORPUS)?;
    for (name, pin) in value["provenance"]["sources"]
        .as_object()
        .ok_or("missing snapshot source pins")?
    {
        let bytes = match fs::read(root.join(name)) {
            Err(e)
                if RETIRED.contains(&name.as_str()) && e.kind() == std::io::ErrorKind::NotFound =>
            {
                continue
            }
            result => result?,
        };
        require(
            pin.as_str() == Some(&sha(&bytes)),
            format!("{name} changed; independently recapture snapshot source"),
        )?;
    }
    Ok(value)
}
#[derive(Clone, Debug)]
pub struct Options {
    pub binary: PathBuf,
    pub reference: PathBuf,
    pub params_reference: PathBuf,
    pub child: PathBuf,
    pub launcher: PathBuf,
    pub output: PathBuf,
    pub runner: Vec<OsString>,
}
fn enabled(config: &Value, key: &str) -> bool {
    config.get(key).is_some_and(|v| match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    })
}
fn entries(path: &Path) -> Result<Vec<PathBuf>> {
    Ok(fs::read_dir(path)?
        .map(|e| e.map(|v| v.path()))
        .collect::<std::io::Result<_>>()?)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn tree(base: &Path, path: &Path, rows: &mut Vec<Value>) -> Result<()> {
    for p in entries(path)? {
        let relative = p
            .strip_prefix(base)?
            .to_str()
            .ok_or("nonUTF8 snapshot tree")?;
        rows.push(json!([
            relative,
            if p.is_dir() {
                "<directory>".into()
            } else {
                hex(&fs::read(&p)?)
            }
        ]));
        if p.is_dir() {
            tree(base, &p, rows)?
        }
    }
    Ok(())
}
fn write_pretty(path: &Path, v: &Value) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(v)?)?;
    Ok(())
}
/// Original invocation schema, with Python filesystem surrogateescape text.
pub fn invocation_json(
    argv: &[OsString],
    binary_sha256: Option<&str>,
    mode: &str,
    config: &Value,
) -> Result<String> {
    let arguments = argv
        .iter()
        .map(|v| crate::python_fs::json_string(v))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let raw = format!(
        "{{\"argv\":[{arguments}],\"binary_sha256\":{},\"exit\":0,\"mode\":{},\"inputs\":{}}}",
        serde_json::to_string(&binary_sha256)?,
        serde_json::to_string(mode)?,
        config
    );
    let value = crate::control_tools_policy_check::json::parse_text(&raw)?;
    crate::control_tools_policy_check::json::pretty(&value, 0)
}
fn write_provenance(root: &Path, options: &Options) -> Result<()> {
    let checker = std::env::current_exe()?;
    let mut binaries = vec![];
    for (role, path) in [
        ("checker", checker.as_path()),
        ("reference", options.reference.as_path()),
        ("child", options.child.as_path()),
        ("candidate", options.binary.as_path()),
        ("params_reference", options.params_reference.as_path()),
        ("launcher", options.launcher.as_path()),
    ] {
        binaries.push(format!(
            "{{\"role\":{},\"path\":{},\"sha256\":{}}}",
            serde_json::to_string(role)?,
            crate::python_fs::json_string(path.as_os_str())?,
            serde_json::to_string(&sha(&fs::read(path)?))?
        ));
    }
    let manifest = sources(root)?;
    let encoded=format!("{{\"kind\":\"bootlog-native-comparison\",\"status\":\"PASS\",\"executables\":[{}],\"sources\":{}}}",binaries.join(","),manifest["provenance"]["sources"]);
    let parsed = crate::control_tools_policy_check::json::parse_text(&encoded)?;
    fs::write(
        options.output.join("provenance.json"),
        crate::control_tools_policy_check::json::pretty(&parsed, 0)?,
    )?;
    Ok(())
}
pub fn run_case(options: &Options, side: &str, name: &str, config: &Value) -> Result<Value> {
    let output = options.output.join(name).join(side);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&output)?;
    let temporary = output.join("temporary");
    fs::create_dir(&temporary)?;
    let params = output.join("params/d");
    fs::create_dir_all(&params)?;
    let dongle = params.join("DongleId");
    fs::write(&dongle, b"before\0snapshot")?;
    fs::set_permissions(&dongle, fs::Permissions::from_mode(0o640))?;
    let time = rustix::fs::Timespec {
        tv_sec: 1_600_000_000,
        tv_nsec: 0,
    };
    rustix::fs::utimensat(
        rustix::fs::CWD,
        &dongle,
        &rustix::fs::Timestamps {
            last_access: time,
            last_modification: time,
        },
        rustix::fs::AtFlags::empty(),
    )?;
    rustix::fs::setxattr(
        &dongle,
        "user.fixture",
        b"attribute",
        rustix::fs::XattrFlags::empty(),
    )?;
    symlink("DongleId", params.join("link"))?;
    if enabled(config, "broken") {
        symlink("missing", params.join("broken"))?
    }
    if enabled(config, "fifo") {
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            params.join("fifo"),
            rustix::fs::Mode::from_raw_mode(0o666),
        )?
    }
    if enabled(config, "special_file") {
        symlink("/dev/null", params.join("null-link"))?
    }
    if enabled(config, "unreadable_directory") {
        fs::set_permissions(&params, fs::Permissions::from_mode(0o0))?
    }
    let basedir = output.join("basedir");
    let loggerd = basedir.join("openpilot/system/loggerd");
    fs::create_dir_all(&loggerd)?;
    if !enabled(config, "missing_binary") {
        if enabled(config, "nonexec") {
            fs::copy(&options.child, loggerd.join("bootlog"))?;
            fs::set_permissions(loggerd.join("bootlog"), fs::Permissions::from_mode(0o644))?;
        } else {
            symlink(&options.child, loggerd.join("bootlog"))?
        }
    }
    let mode = if enabled(config, "detached") {
        "detached"
    } else {
        "joined"
    };
    let mut argv = if side == "source" {
        vec![
            options.reference.as_os_str().to_owned(),
            options.params_reference.as_os_str().to_owned(),
            basedir.as_os_str().to_owned(),
            mode.into(),
        ]
    } else {
        let mut a = options.runner.clone();
        a.extend([
            options.binary.as_os_str().to_owned(),
            params.as_os_str().to_owned(),
            loggerd.as_os_str().to_owned(),
            mode.into(),
            options.launcher.as_os_str().to_owned(),
        ]);
        a
    };
    let first = argv.remove(0);
    let mut command = Command::new(&first);
    command.args(&argv);
    command
        .env("PARAMS_ROOT", params.parent().unwrap())
        .env("OPENPILOT_PREFIX", "d")
        .env("TMPDIR", &temporary)
        .env("PARAMS_COPY_PATH", "must-be-overridden")
        .env("BOOTLOG_TEST_MARKER", "inherited-marker")
        .env(
            "BOOTLOG_CHILD_EXIT",
            config
                .get("exit")
                .map(Value::to_string)
                .unwrap_or_else(|| "0".into()),
        );
    if enabled(config, "cleanup_failure") {
        command.env("BOOTLOG_CLEANUP_FAILURE", "1");
    }
    if enabled(config, "special_file") {
        command.env("BOOTLOG_SPECIAL_COPY", "1");
    }
    let marker = tempfile::Builder::new().prefix("bootlog-fd-").tempfile()?;
    command.env("BOOTLOG_FD_MARKER", marker.path());
    let mut process = Process::start(
        command,
        fs::File::create(output.join("stderr.log"))?,
        marker.as_file(),
    )?;
    let result = (|| -> Result<Vec<Value>> {
        let mut rows = vec![];
        if enabled(config, "broken")
            || enabled(config, "fifo")
            || enabled(config, "unreadable_directory")
        {
            let row = process.line()?;
            require(row["phase"] == "copy_failed", "copy failure phase differs")?;
            rows.push(row);
        } else if enabled(config, "missing_binary") || enabled(config, "nonexec") {
            rows.push(process.line()?);
            rows.push(process.line()?);
        } else if enabled(config, "detached") {
            let row = process.line()?;
            require(row["phase"] == "returned", "detached return phase differs")?;
            rows.push(row);
            process.wait(5)?;
            require(
                !entries(&temporary)?.is_empty(),
                "detached parent removed pending snapshot",
            )?;
            process.finish_input()?;
            process.drain()?;
        } else {
            loop {
                let phases = rows
                    .iter()
                    .filter_map(|r: &Value| r["phase"].as_str())
                    .collect::<BTreeSet<_>>();
                if phases == BTreeSet::from(["returned", "child_ready"]) {
                    break;
                }
                rows.push(process.line()?);
            }
            require(process.running()?, "snapshot parent exited before release")?;
            fs::write(&dongle, b"live-after-return")?;
            process.finish_input()?;
            rows.push(process.line()?);
            rows.push(process.line()?);
        }
        process.wait(15)?;
        Ok(rows)
    })();
    // Restore permission only after the actual process outcome is observed.
    fs::set_permissions(&params, fs::Permissions::from_mode(0o700))?;
    let mut rows = result?;
    if rows
        .iter()
        .any(|r| r["phase"] == "worker_done" && r["success"] == false)
    {
        require(
            fs::metadata(output.join("stderr.log"))?.len() > 0,
            "worker exception was silent",
        )?;
    }
    let snapshots = entries(&temporary)?;
    for p in &snapshots {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700))?
    }
    for row in &mut rows {
        if row["phase"] == "child_ready" {
            require(
                row["extra_fd_inherited"] == false,
                "bootlog inherited extra file descriptor",
            )?;
            require(row["snapshot_mode"] == 0o700, "snapshot mode differs")?;
            if enabled(config, "special_file") {
                require(row["special_copy"] == true, "special file was not copied")?;
            }
            require(
                row["cwd"].as_str().map(Path::new) == Some(loggerd.as_path()),
                "bootlog cwd differs",
            )?;
            require(
                row["snapshot"].as_str().and_then(|s| Path::new(s).parent())
                    == Some(temporary.as_path()),
                "snapshot temporary path differs",
            )?;
            row.as_object_mut()
                .ok_or("child row not object")?
                .remove("cwd");
            row.as_object_mut().unwrap().remove("snapshot");
        }
    }
    rows.sort_by(|a, b| a["phase"].as_str().cmp(&b["phase"].as_str()));
    let mut retained = vec![];
    for snapshot in &snapshots {
        tree(snapshot, snapshot, &mut retained)?;
    }
    retained.sort_by_key(Value::to_string);
    let value = json!({"rows":rows,"retained_snapshots":snapshots.len(),"retained_tree":retained});
    write_pretty(&output.join("result.json"), &value)?;
    let mut command = vec![first];
    command.extend(argv);
    let binary_hash = if side == "native" {
        Some(sha(&fs::read(&options.binary)?))
    } else {
        None
    };
    fs::write(
        output.join("invocation.json"),
        invocation_json(&command, binary_hash.as_deref(), mode, config)?,
    )?;
    Ok(value)
}
pub fn check(root: &Path, options: &Options, printed: &mut dyn Write) -> Result<()> {
    let corpus = sources(root)?;
    validate_reference(root, &options.params_reference)?;
    let mut results = vec![];
    for c in corpus["cases"].as_array().ok_or("missing original cases")? {
        let name = c["name"].as_str().ok_or("missing case name")?;
        let source = run_case(options, "source", name, &c["config"])?;
        let native = run_case(options, "native", name, &c["config"])?;
        require(
            source == native,
            format!("{name}: source/native snapshot behavior differs"),
        )?;
        results.push(json!({"scenario":name,"result":"PASS"}));
        writeln!(printed, "{name} PASS")?;
        printed.flush()?;
    }
    write_pretty(
        &options.output.join("result.json"),
        &json!({"result":"PASS","cases":results}),
    )?;
    write_provenance(root, options)?;
    Ok(())
}
fn validate_reference(root: &Path, reference: &Path) -> Result<()> {
    let r: Value = serde_json::from_slice(&fs::read(reference.with_file_name("provenance.json"))?)?;
    let original = sources(root)?;
    require(
        r["kind"] == "bootlog-original-params-path"
            && r["binary_sha256"] == sha(&fs::read(reference)?)
            && r["sources"] == original["provenance"]["sources"],
        "original Params path reference receipt differs",
    )?;
    require(
        r["helper_sha256"]
            == sha(&fs::read(root.join(
                "rust/crates/dev-tools/tests/fixtures/bootlog-snapshot/params_path.cc",
            ))?),
        "Params path raw helper source changed",
    )
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os_with_positionals, OptionSpec};
    let a = scan_os_with_positionals(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--launcher", true),
            OptionSpec::value("--child", true),
            OptionSpec::value("--params-reference", true),
            OptionSpec::append("--runner"),
        ],
        3,
    )?;
    if a.help {
        println!("check-bootlog-snapshot BINARY REFERENCE OUTPUT --launcher EXECUTABLE --child EXECUTABLE --params-reference EXECUTABLE [--runner ARG]");
        return Ok(());
    }
    if a.positionals.len() != 3 {
        return Err(
            crate::argparse_scan::ArgumentError("expected BINARY REFERENCE OUTPUT".into()).into(),
        );
    }
    let o = Options {
        binary: resolve_non_strict(Path::new(&a.positionals[0]))?,
        reference: resolve_non_strict(Path::new(&a.positionals[1]))?,
        output: resolve_non_strict(Path::new(&a.positionals[2]))?,
        launcher: resolve_non_strict(Path::new(&a.values["--launcher"]))?,
        child: resolve_non_strict(Path::new(&a.values["--child"]))?,
        params_reference: resolve_non_strict(Path::new(&a.values["--params-reference"]))?,
        runner: a.append_values.get("--runner").cloned().unwrap_or_default(),
    };
    check(&root()?, &o, &mut std::io::stdout())
}
pub fn child_cli() -> Result<()> {
    let snapshot =
        PathBuf::from(std::env::var_os("PARAMS_COPY_PATH").ok_or("missing PARAMS_COPY_PATH")?);
    let marker =
        PathBuf::from(std::env::var_os("BOOTLOG_FD_MARKER").ok_or("missing BOOTLOG_FD_MARKER")?);
    let inherited = entries(Path::new("/proc/self/fd"))?
        .iter()
        .any(|p| fs::canonicalize(p).is_ok_and(|p| p == marker));
    let dongle = snapshot.join("d/DongleId");
    let meta = fs::metadata(&dongle)?;
    let mut xattr = vec![0; 65536];
    let n = rustix::fs::getxattr(&dongle, "user.fixture", xattr.as_mut_slice())?;
    xattr.truncate(n);
    let ready = json!({"phase":"child_ready","cwd":std::env::current_dir()?,"snapshot":snapshot,"marker":std::env::var("BOOTLOG_TEST_MARKER").ok(),"extra_fd_inherited":inherited,"special_copy":if std::env::var_os("BOOTLOG_SPECIAL_COPY").is_some_and(|v|!v.is_empty()){Some(snapshot.join("d/null-link").is_file())}else{None},"snapshot_mode":fs::metadata(&snapshot)?.mode()&0o777,"before":hex(&fs::read(&dongle)?),"link_is_symlink":snapshot.join("d/link").is_symlink(),"linked":hex(&fs::read(snapshot.join("d/link"))?),"xattr":hex(&xattr),"mode":meta.mode()&0o777,"mtime":meta.mtime()as i128*1_000_000_000+meta.mtime_nsec()as i128});
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &ready)?;
    writeln!(out)?;
    out.flush()?;
    drop(out);
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let done = json!({"phase":"child_done","snapshot_exists":snapshot.exists(),"after":hex(&fs::read(&dongle)?),"live":hex(&fs::read(PathBuf::from(std::env::var_os("PARAMS_ROOT").ok_or("missing PARAMS_ROOT")?).join("d/DongleId"))?)});
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &done)?;
    writeln!(out)?;
    out.flush()?;
    if std::env::var_os("BOOTLOG_CLEANUP_FAILURE").is_some_and(|v| !v.is_empty()) {
        fs::set_permissions(snapshot, fs::Permissions::from_mode(0o0))?
    }
    let status = std::env::var("BOOTLOG_CHILD_EXIT")
        .unwrap_or_else(|_| "0".into())
        .parse::<i32>()?;
    std::process::exit(status)
}
