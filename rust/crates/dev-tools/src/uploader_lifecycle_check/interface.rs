//! One grouped native entrypoint; all original expectations are sealed before use.
use super::{corpus, records, require, Namespace, Options, Result};
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn path(value: &str) -> Result<PathBuf> {
    Ok(Path::new(value).canonicalize()?)
}
fn idle(args: &[String]) -> Result<()> {
    require(args.len() == 2, "idle worker arguments invalid")?;
    fs::write(&args[1], std::process::id().to_string())?;
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
fn negative_worker(args: &[String]) -> Result<()> {
    require(args.len() == 4, "negative worker arguments invalid")?;
    let prefix = &args[1];
    require(
        prefix.starts_with("rust-probe-uploader-life-")
            && prefix.len() < 90
            && prefix
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-'),
        "negative prefix escaped owned namespace",
    )?;
    let ns = Namespace::worker(prefix)?;
    let _socket = std::os::unix::net::UnixListener::bind(&ns.endpoint)?;
    if args[3] == "spawn-failure" {
        let mut unexpected =
            Command::new(Path::new(&args[2]).join("missing-native-executable")).spawn()?;
        let _ = unexpected.kill();
        let _ = unexpected.wait();
        return Err("missing provider unexpectedly spawned".into());
    }
    require(args[3] == "timeout", "negative worker mode absent")?;
    let child = Command::new(std::env::current_exe()?)
        .arg("--uploader-lifecycle-idle")
        .arg(Path::new(&args[2]).join("descendant.pid"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // The outer NativeRunner owns this child's process group, including the
    // descendant. Timeout deliberately bypasses Rust destructors in this worker.
    let _child = super::Owned(Some(child));
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
fn not_running(pid: u32) -> Result<bool> {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => Ok(stat
            .rsplit_once(") ")
            .is_some_and(|(_, fields)| fields.starts_with('Z'))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}
/// Real owned-group timeout and early-spawn cleanup with unrelated sentinels.
fn cleanup(worker: &Path, output: &Path) -> Result<Value> {
    fs::create_dir_all(output)?;
    let temp = tempfile::tempdir()?;
    let token = temp
        .path()
        .file_name()
        .ok_or("cleanup token absent")?
        .to_string_lossy()
        .replace('.', "-");
    let sentinel_prefix = format!(
        "rust-probe-uploader-life-sentinel-{}-{token}",
        std::process::id()
    );
    let sentinel = Namespace::new(&sentinel_prefix)?;
    fs::write(sentinel.shm.join("preserve"), b"sentinel")?;
    let _sentinel_socket = std::os::unix::net::UnixListener::bind(&sentinel.endpoint)?;
    let sentinel_pid = temp.path().join("sentinel.pid");
    let mut owned = super::Owned(Some(
        Command::new(worker)
            .arg("--uploader-lifecycle-idle")
            .arg(&sentinel_pid)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    ));
    let mut rows = Vec::new();
    for mode in ["spawn-failure", "timeout"] {
        let prefix = format!(
            "rust-probe-uploader-life-{}-{token}-{mode}",
            std::process::id()
        );
        let ns = Namespace::new(&prefix)?;
        let shm = ns.shm.clone();
        let endpoint = ns.ipc_root.clone();
        let mut spec = CommandSpec::new_os(
            vec![
                worker.as_os_str().to_owned(),
                "--uploader-lifecycle-negative".into(),
                prefix.into(),
                temp.path().as_os_str().to_owned(),
                mode.into(),
            ],
            Duration::from_secs(3),
        );
        spec.env.insert("RUST_BACKTRACE".into(), "0".into());
        for (key, value) in ns.environment()? {
            spec.env.insert(key, value);
        }
        let result = NativeRunner.run(&spec);
        if mode == "timeout" {
            let error = result
                .err()
                .ok_or("timeout worker unexpectedly succeeded")?;
            require(
                error.to_string().contains("timed out"),
                "negative worker failed before deadline",
            )?;
            let pid: u32 = fs::read_to_string(temp.path().join("descendant.pid"))?.parse()?;
            let start = Instant::now();
            while !not_running(pid)? {
                require(
                    start.elapsed() < Duration::from_secs(2),
                    "owned timeout descendant survived cleanup",
                )?;
                thread::sleep(Duration::from_millis(5));
            }
        } else {
            let captured = result?;
            fs::write(output.join("spawn-failure.stderr"), captured.stderr)?;
            require(captured.code != 0, "early spawn failure accepted")?;
        }
        drop(ns);
        require(
            !shm.exists() && !endpoint.exists(),
            "owned negative namespace leaked",
        )?;
        owned.alive()?;
        require(
            fs::read(sentinel.shm.join("preserve"))? == b"sentinel"
                && fs::symlink_metadata(&sentinel.endpoint)?
                    .file_type()
                    .is_socket(),
            "cleanup damaged unrelated IPC namespace",
        )?;
        rows.push(json!({"mode":mode,"owned_namespace_removed":true,"unrelated_process_and_namespace_preserved":true}));
    }
    let result = json!({"passed":true,"scenarios":rows});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(result)
}
pub fn cli() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if super::worker_cli(&args)? {
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("--uploader-source-collector") => {
            require(args.len() == 3, "source collector arguments invalid")?;
            return crate::managed_entry_collector_source::run(&args[1], Path::new(&args[2]));
        }
        Some("--uploader-records-worker") => {
            require(args.len() == 2, "records worker arguments invalid")?;
            println!("{}", records::worker(Path::new(&args[1]))?);
            return Ok(());
        }
        Some("--uploader-lifecycle-idle") => return idle(&args),
        Some("--uploader-lifecycle-negative") => return negative_worker(&args),
        _ => {}
    }
    use crate::argparse_scan::{scan, OptionSpec as S};
    let parsed = scan(
        &args,
        &[
            S::value("--root", true),
            S::value("--scope", true),
            S::value("--binary", false),
            S::value("--trace", false),
            S::value("--fault", false),
            S::value("--collector", false),
            S::value("--interrupted", false),
            S::value("--http-provider", false),
            S::value("--fixture", true),
            S::value("--fixture-sha256", true),
            S::value("--output", true),
            S::flag("--help"),
        ],
    )?;
    if parsed.flags.contains("--help") {
        println!("Usage: check-uploader-siblings --root ROOT --scope all|daemon|interrupted-send|interrupted-http|timeouts|late-timeouts|fault|params|cleanup --fixture JSON --fixture-sha256 SHA256 --output DIR [--binary PATH --trace PATH --fault PATH --collector PATH --interrupted PATH --http-provider PATH]");
        return Ok(());
    }
    require(
        parsed.positionals.is_empty(),
        "unexpected sibling positional argument",
    )?;
    let get = |key: &str| -> Result<&str> {
        parsed
            .values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("{key} required").into())
    };
    let root = path(get("--root")?)?;
    let fixture = path(get("--fixture")?)?;
    let corpus = corpus::load(&root, &fixture, get("--fixture-sha256")?)?;
    crate::managed_entry_collector_source::sources(&root)?;
    let output = PathBuf::from(get("--output")?);
    fs::create_dir_all(&output)?;
    let worker = std::env::current_exe()?;
    let scope = get("--scope")?;
    require(
        [
            "all",
            "daemon",
            "interrupted-send",
            "interrupted-http",
            "timeouts",
            "late-timeouts",
            "fault",
            "params",
            "cleanup",
        ]
        .contains(&scope),
        "unknown sibling scope",
    )?;
    let mut reports = serde_json::Map::new();
    let mut inputs = std::collections::BTreeMap::new();
    let mut provider = |key: &str| -> Result<PathBuf> {
        let p = path(get(key)?)?;
        inputs.insert(p.clone(), corpus::hash(&corpus::bytes(&p, 256 << 20)?));
        Ok(p)
    };
    if scope == "all" || scope == "daemon" {
        reports.insert(
            "daemon".into(),
            super::run_daemon(&Options {
                root: root.clone(),
                binary: provider("--binary")?,
                worker: worker.clone(),
                output: output.join("daemon"),
            })?,
        );
    }
    if scope == "all" || scope == "interrupted-send" {
        reports.insert(
            "interrupted-send".into(),
            super::run_interrupted_send(&Options {
                root: root.clone(),
                binary: provider("--interrupted")?,
                worker: worker.clone(),
                output: output.join("interrupted-send"),
            })?,
        );
    }
    if scope == "all" || scope == "interrupted-http" {
        reports.insert(
            "interrupted-http".into(),
            super::run_interrupted_http(
                &Options {
                    root: root.clone(),
                    binary: provider("--binary")?,
                    worker: worker.clone(),
                    output: output.join("interrupted-http"),
                },
                &provider("--http-provider")?,
            )?,
        );
    }
    if scope == "all" || scope == "timeouts" || scope == "late-timeouts" {
        reports.insert(
            "timeouts".into(),
            super::run_timeouts(
                &root,
                &provider("--trace")?,
                &output.join("timeouts"),
                &corpus.timeouts,
                scope == "late-timeouts",
            )?,
        );
    }
    if scope == "all" || scope == "fault" {
        reports.insert(
            "fault".into(),
            records::run(
                &root,
                &provider("--fault")?,
                &provider("--collector")?,
                &worker,
                &output.join("fault"),
                &corpus.fault,
                true,
            )?,
        );
    }
    if scope == "all" || scope == "params" {
        reports.insert(
            "params".into(),
            records::run(
                &root,
                &provider("--binary")?,
                &provider("--collector")?,
                &worker,
                &output.join("params"),
                &corpus.params,
                false,
            )?,
        );
    }
    if scope == "all" || scope == "cleanup" {
        reports.insert("cleanup".into(), cleanup(&worker, &output.join("cleanup"))?);
    }
    for (file, pin) in inputs {
        require(
            corpus::hash(&corpus::bytes(&file, 256 << 20)?) == pin,
            "native provider changed during sibling run",
        )?;
    }
    corpus::load(&root, &fixture, get("--fixture-sha256")?)?;
    let result = json!({"passed":true,"scope":scope,"reports":reports,"fixture_sha256":corpus::SHA,"all_source_expectations_unchanged":true,"fresh_python_runtime":false,"shared_helpers_retired":false});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
