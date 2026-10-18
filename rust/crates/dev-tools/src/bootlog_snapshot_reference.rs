//! Independent source-derived manager/helpers.py::save_bootlog adapter.
//! Uses unchanged C++ Params, CPython copy2/copytree semantics, and raw process
//! syscalls in params_path.cc. Does not depend on openpilot-bootlog.
use crate::{
    bootlog_snapshot_check::{sha, sources},
    params_reference_check::{build_native, ParamsRunner},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
const HELPER: &str = "rust/crates/dev-tools/tests/fixtures/bootlog-snapshot/params_path.cc";
fn buffer(
    mut operation: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
) -> rustix::io::Result<Vec<u8>> {
    let mut result = vec![0; 256];
    loop {
        match operation(&mut result) {
            Ok(n) => {
                result.truncate(n);
                return Ok(result);
            }
            Err(rustix::io::Errno::RANGE) => result.resize(
                result
                    .len()
                    .checked_mul(2)
                    .ok_or(rustix::io::Errno::NOMEM)?,
                0,
            ),
            Err(e) => return Err(e),
        }
    }
}
fn copystat(from: &Path, to: &Path) -> Result<()> {
    let m = fs::metadata(from)?;
    rustix::fs::utimensat(
        rustix::fs::CWD,
        to,
        &rustix::fs::Timestamps {
            last_access: rustix::fs::Timespec {
                tv_sec: m.atime(),
                tv_nsec: m.atime_nsec(),
            },
            last_modification: rustix::fs::Timespec {
                tv_sec: m.mtime(),
                tv_nsec: m.mtime_nsec(),
            },
        },
        rustix::fs::AtFlags::empty(),
    )?;
    use rustix::io::Errno;
    match buffer(|b| rustix::fs::listxattr(from, b)) {
        Ok(names) => {
            for name in names.split(|b| *b == 0).filter(|n| !n.is_empty()) {
                let n = OsStr::from_bytes(name);
                let result = buffer(|b| rustix::fs::getxattr(from, n, b)).and_then(|value| {
                    rustix::fs::setxattr(to, n, &value, rustix::fs::XattrFlags::empty())
                });
                match result {
                    Ok(())
                    | Err(
                        Errno::PERM | Errno::NOTSUP | Errno::NODATA | Errno::INVAL | Errno::ACCESS,
                    ) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Err(Errno::NOTSUP | Errno::NODATA | Errno::INVAL) => {}
        Err(e) => return Err(e.into()),
    }
    fs::set_permissions(to, m.permissions())?;
    Ok(())
}
fn copytree(from: &Path, to: &Path) -> Result<()> {
    // CPython eagerly scans before mkdir; following links is the default. Record
    // individual failures but finish all siblings and directory copystat first.
    let entries = fs::read_dir(from)?.collect::<std::io::Result<Vec<_>>>()?;
    fs::create_dir_all(to)?;
    let mut failure = None;
    for entry in entries {
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let result = (|| -> Result<()> {
            let meta = fs::metadata(&source)?;
            if meta.is_dir() {
                copytree(&source, &destination)
            } else {
                if meta.file_type().is_fifo() {
                    return Err("named pipe in shutil.copyfile".into());
                }
                let mut input = fs::File::open(&source)?;
                let mut output = fs::File::create(&destination)?;
                std::io::copy(&mut input, &mut output)?;
                drop(output);
                drop(input);
                copystat(&source, &destination)
            }
        })();
        if let Err(e) = result {
            failure = Some(e);
        }
    }
    if let Err(e) = copystat(from, to) {
        failure = Some(e);
    }
    match failure {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
fn path(helper: &Path) -> Result<PathBuf> {
    let output = Command::new(helper)
        .arg("path")
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err("original Params path process failed".into());
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?))
}
fn emit(value: serde_json::Value) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
pub fn reference_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("expected PARAMS_REFERENCE BASEDIR MODE".into());
    }
    let helper = PathBuf::from(&args[0]);
    let basedir = PathBuf::from(&args[1]);
    let copied = (|| -> Result<PathBuf> {
        let temporary = tempfile::Builder::new()
            .prefix("tmp")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()?
            .keep();
        let first = path(&helper)?;
        let name = first.file_name().ok_or("Params path has no basename")?;
        copytree(&path(&helper)?, &temporary.join(name))?;
        Ok(temporary)
    })();
    let temporary = match copied {
        Ok(p) => p,
        Err(_) => return emit(serde_json::json!({"phase":"copy_failed"})),
    };
    let worker = std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            let loggerd = basedir.join("openpilot/system/loggerd");
            let bootlog = loggerd.join("bootlog");
            if bootlog.exists() {
                let report = tempfile::Builder::new()
                    .prefix("bootlog-call-")
                    .tempfile_in(&basedir)?;
                let status = Command::new(helper)
                    .args([
                        OsStr::new("call"),
                        bootlog.as_os_str(),
                        report.path().as_os_str(),
                    ])
                    .current_dir(&loggerd)
                    .env("PARAMS_COPY_PATH", &temporary)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()?;
                if !status.success() {
                    return Err("source raw subprocess call failed".into());
                }
                let value: serde_json::Value = serde_json::from_slice(&fs::read(report.path())?)?;
                if value["exec_errno"].as_i64().ok_or("missing exec result")? != 0 {
                    return Err("source bootlog exec failed".into());
                }
                // subprocess.call does not check the child's exit status.
            }
            fs::remove_dir_all(temporary)?;
            Ok(())
        })();
        result.map_err(|error| error.to_string())
    });
    emit(serde_json::json!({"phase":"returned"}))?;
    if args[2] == "detached" {
        return Ok(());
    }
    let result = worker.join().map_err(|_| "source worker panicked")?;
    if let Err(e) = &result {
        eprintln!("source bootlog snapshot worker: {e}");
    }
    emit(serde_json::json!({"phase":"worker_done","success":result.is_ok()}))
}
struct Build;
impl Runner for Build {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        NativeRunner.run(s)
    }
}
impl ParamsRunner for Build {
    fn locked_put(&mut self, _: &Path, _: &Path) -> Result<()> {
        Err("no locked put in path adapter".into())
    }
}
pub fn build(root: &Path, output: &Path, prefix: Option<&Path>) -> Result<PathBuf> {
    let manifest = sources(root)?;
    if let Some(p) = output.parent() {
        fs::create_dir_all(p)?
    }
    fs::create_dir(output)?;
    openpilot_disk_space::inspect(output, 512 * 1024 * 1024)?.require()?;
    build_native(root, output, prefix, &mut Build)?;
    let binary = output.join("bootlog-params-path");
    let mut args = vec![
        OsString::from("g++"),
        "-std=c++17".into(),
        "-pthread".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wl,--gc-sections".into(),
    ];
    for p in [
        root.join("openpilot"),
        output.into(),
        output.join("cereal/gen/cpp"),
    ] {
        args.extend(["-I".into(), p.into_os_string()]);
    }
    if let Some(p) = prefix {
        args.extend(["-I".into(), p.join("include").into_os_string()]);
    }
    for p in [
        "openpilot/common/params.cc",
        "openpilot/common/util.cc",
        HELPER,
    ] {
        args.push(root.join(p).into_os_string());
    }
    args.extend(["-o".into(), binary.as_os_str().to_owned()]);
    let mut spec = CommandSpec::new_os(args, Duration::MAX);
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    if NativeRunner.run(&spec)?.code != 0 {
        return Err("original Params path build failed".into());
    }
    fs::write(
        output.join("provenance.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"kind":"bootlog-original-params-path","binary_sha256":sha(&fs::read(&binary)?),"sources":manifest["provenance"]["sources"],"helper_sha256":sha(&fs::read(root.join(HELPER))?)}),
        )? + "\n",
    )?;
    Ok(binary)
}
pub fn build_cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--prefix", false),
        ],
    )?;
    if args.help {
        println!("build-bootlog-snapshot-reference --output DIRECTORY [--prefix PREFIX]");
        return Ok(());
    }
    let root = crate::bootlog_snapshot_check::root()?;
    build(
        &root,
        Path::new(&args.values["--output"]),
        args.values.get("--prefix").map(Path::new),
    )?;
    Ok(())
}
