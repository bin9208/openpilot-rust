//! Native static AArch64 qemu/real-msgq QA controller; no device validation.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner},
    seal::Result,
};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod directory;
mod elf;
mod inputs;
mod receipt;
mod worker;
pub use elf::inspect_static;
#[derive(Debug)]
pub struct Options {
    pub binary: PathBuf,
    pub runner: PathBuf,
    pub report: PathBuf,
}
pub fn arguments(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--runner", true),
            OptionSpec::value("--report", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        binary: parsed.values["--binary"].clone().into(),
        runner: parsed.values["--runner"].clone().into(),
        report: parsed.values["--report"].clone().into(),
    }))
}
pub fn prepare(root: &Path) -> Result<()> {
    fs::write(
        root.join("stat"),
        "cpu 1 2 3 4 5 6 7\ncpu0 100 2 30 400 5 6 7\n",
    )?;
    fs::write(root.join("meminfo"), "MemTotal: 128 kB\n")?;
    crate::proclog_reference_check::make_process(root, 123, 2000)
}
pub fn check_frame(bytes: &[u8]) -> Result<()> {
    let message = capnp::serialize::read_message(bytes, Default::default())?;
    let e = message.get_root::<event::Reader<'_>>()?;
    if !e.get_valid() {
        return Err("invalid Event/procLog valid field".into());
    }
    let event::ProcLog(log) = e.which()? else {
        return Err("expected Event/procLog".into());
    };
    let log = log?;
    if log.get_mem()?.get_total() != 128 * 1024
        || log.get_cpu_times()?.len() != 1
        || log.get_procs()?.len() != 1
    {
        return Err("AArch64 procLog memory/CPU/process count differs".into());
    }
    let p = log.get_procs()?.get(0);
    if p.get_pid() != 123
        || p.get_mem_rss() != 2000 * 4096
        || p.get_mem_pss() != 50 * 1024
        || p.get_nice() != -5
        || p.get_name()?.to_str()? != "worker ) (a"
    {
        return Err("AArch64 procLog process fields differ".into());
    }
    Ok(())
}
pub fn report(binary: &Path, frames: usize) -> Result<Value> {
    Ok(
        json!({"result":"pass","execution":"qemu-aarch64","procfs":"synthetic fixture","frames":frames,"binary_sha256":format!("{:x}",Sha256::digest(fs::read(binary)?)),"elf":"static AArch64; no interpreter or shared dependencies","device_validation":"not_run"}),
    )
}
fn check(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Relaxed) {
        Err("AArch64 check interrupted".into())
    } else {
        Ok(())
    }
}
pub fn run(root: &Path, options: &Options, stop: &AtomicBool) -> Result<Value> {
    Ok(execute(root, options, stop)?.0)
}
fn execute(
    root: &Path,
    options: &Options,
    stop: &AtomicBool,
) -> Result<(Value, receipt::Published)> {
    let report_path = resolve_non_strict(&options.report)?;
    check(stop)?;
    let binary = resolve_non_strict(&options.binary)?;
    let runner = resolve_non_strict(&options.runner)?;
    let inputs = inputs::Sources::open(root)?;
    let artifact = crate::proclog_reference_check::inputs::Bound::open(&binary, None)?;
    let runner_identity = crate::proclog_reference_check::inputs::Bound::open(&runner, None)?;
    if inputs.aliases(&report_path)?
        || artifact.aliases(&report_path)?
        || runner_identity.aliases(&report_path)?
    {
        return Err("AArch64 report aliases a bound checker input".into());
    }
    inspect_static(&fs::read(&binary)?)?;
    let fixture = directory::Directory::new(
        tempfile::Builder::new()
            .prefix("rust-aarch64-proc-")
            .tempdir()?,
    )?;
    prepare(fixture.path())?;
    let namespace = directory::Directory::new(
        tempfile::Builder::new()
            .prefix("msgq_rust-probe-")
            .tempdir_in("/dev/shm")?,
    )?;
    let prefix = namespace
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("msgq_"))
        .ok_or("invalid owned namespace")?;
    let mut command = CommandSpec::new_os(
        vec![
            std::env::current_exe()?.into_os_string(),
            "--aarch64-worker".into(),
            binary.clone().into_os_string(),
            runner.clone().into_os_string(),
            fixture.path().as_os_str().to_owned(),
        ],
        Duration::from_secs(60),
    );
    command.env.insert("OPENPILOT_PREFIX".into(), prefix.into());
    command.env_remove.push("CEREAL_FAKE".into());
    command.inherit_output = true;
    command.inherit_stdin = true;
    let output =
        NativeRunner.run_controlled(&command, Instant::now() + Duration::from_secs(60), stop)?;
    if output.code != 0 {
        return Err("AArch64 owned worker failed".into());
    }
    inputs.verify()?;
    artifact.verify_path(&binary)?;
    artifact.verify_path(&options.binary)?;
    runner_identity.verify_path(&runner)?;
    runner_identity.verify_path(&options.runner)?;
    check(stop)?;
    namespace.close()?;
    fixture.close()?;
    let result = report(&binary, 3)?;
    // Only a fully validated run may replace caller data. Retain the created
    // file's inode across all post-publication checks and final stdout delivery.
    let published = receipt::Published::write(
        &report_path,
        (serde_json::to_string_pretty(&result)? + "\n").as_bytes(),
        stop,
    )?;
    let verified = (|| {
        inputs.verify()?;
        artifact.verify_path(&binary)?;
        artifact.verify_path(&options.binary)?;
        runner_identity.verify_path(&runner)?;
        runner_identity.verify_path(&options.runner)?;
        check(stop)
    })();
    if verified.is_err() {
        published.remove_owned();
    }
    verified?;
    Ok((result, published))
}

struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn cli() -> Result<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|v| v == "--aarch64-worker") {
        if args.len() != 4 {
            return Err("invalid private AArch64 worker arguments".into());
        }
        return worker::run(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        );
    }
    let Some(options) = arguments(&args)? else {
        println!("Usage: check-aarch64 --binary PATH --runner PATH --report PATH\nStatic AArch64 ELF and emulated synthetic IPC/Params checks; not device validation");
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let (value, receipt) = execute(&root, &options, &stop)?;
    let published = (|| {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&value)?)?;
        check(&stop)
    })();
    if published.is_err() {
        receipt.remove_owned();
    }
    published
}
