//! Dynamic original C++ Params interoperability gate.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    python_glob,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
pub trait ParamsRunner: Runner {
    fn locked_put(&mut self, writer: &Path, store: &Path) -> Result<()>;
    fn cleanup_temporary(&mut self, path: &Path) -> Result<()> {
        cleanup_temporary(path)
    }
}
fn copyfile(source: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::FileTypeExt;
    for path in [source, target] {
        match fs::metadata(path) {
            Ok(meta) if meta.file_type().is_fifo() => {
                return Err("Python copyfile rejects named pipes".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let mut input = fs::File::open(source)?;
    let mut output = fs::File::create(target)?;
    std::io::copy(&mut input, &mut output)?;
    Ok(())
}
fn xattr_buffer(
    mut call: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
) -> rustix::io::Result<Vec<u8>> {
    let mut data = vec![0; 256];
    loop {
        match call(&mut data) {
            Ok(n) => {
                data.truncate(n);
                return Ok(data);
            }
            Err(rustix::io::Errno::RANGE) => data.resize(
                data.len().checked_mul(2).ok_or(rustix::io::Errno::NOMEM)?,
                0,
            ),
            Err(e) => return Err(e),
        }
    }
}
fn metadata(source: &Path, target: &Path) -> Result<()> {
    let m = fs::metadata(source)?;
    use std::os::unix::fs::MetadataExt;
    rustix::fs::utimensat(
        rustix::fs::CWD,
        target,
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
    let ignored_list = [Errno::NOTSUP, Errno::NODATA, Errno::INVAL];
    match xattr_buffer(|buf| rustix::fs::listxattr(source, buf)) {
        Ok(names) => {
            for name in names.split(|b| *b == 0).filter(|n| !n.is_empty()) {
                let name = OsStr::from_bytes(name);
                let result =
                    xattr_buffer(|buf| rustix::fs::getxattr(source, name, buf)).and_then(|value| {
                        rustix::fs::setxattr(target, name, &value, rustix::fs::XattrFlags::empty())
                    });
                if let Err(e) = result {
                    if ![
                        Errno::PERM,
                        Errno::NOTSUP,
                        Errno::NODATA,
                        Errno::INVAL,
                        Errno::ACCESS,
                    ]
                    .contains(&e)
                    {
                        return Err(e.into());
                    }
                }
            }
        }
        Err(e) if ignored_list.contains(&e) => {}
        Err(e) => return Err(e.into()),
    }
    fs::set_permissions(target, m.permissions())?;
    Ok(())
}
fn copytree(source: &Path, target: &Path) -> Result<()> {
    // copytree eagerly scans before mkdir, follows links, and continues siblings
    // after an individual copy error before reporting the aggregate failure.
    let entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    fs::create_dir(target)?;
    let mut failure = None;
    for e in entries {
        let from = e.path();
        let to = target.join(e.file_name());
        let result = (|| {
            if fs::metadata(&from)?.is_dir() {
                copytree(&from, &to)
            } else {
                copyfile(&from, &to)?;
                metadata(&from, &to)
            }
        })();
        if let Err(e) = result {
            failure = Some(e)
        }
    }
    if let Err(e) = metadata(source, target) {
        failure = Some(e)
    }
    if let Some(e) = failure {
        Err(e)
    } else {
        Ok(())
    }
}
fn checked(runner: &mut dyn ParamsRunner, spec: &CommandSpec) -> Result<Vec<u8>> {
    let result = runner.run(spec)?;
    if result.code != 0 {
        return Err(format!("Params child exited {}", result.code).into());
    }
    Ok(result.stdout)
}
pub fn build_native(
    root: &Path,
    output: &Path,
    prefix: Option<&Path>,
    runner: &mut dyn ParamsRunner,
) -> Result<PathBuf> {
    let schema = output.join("schema");
    let generated = output.join("cereal/gen/cpp");
    fs::create_dir(&schema)?;
    fs::create_dir_all(generated.parent().ok_or("missing generated parent")?)?;
    fs::create_dir(&generated)?;
    for name in ["log", "custom", "deprecated"] {
        copyfile(
            &root.join(format!("openpilot/cereal/{name}.capnp")),
            &schema.join(format!("{name}.capnp")),
        )?;
    }
    copyfile(
        &root.join("opendbc_repo/opendbc/car/car.capnp"),
        &schema.join("car.capnp"),
    )?;
    copytree(
        &root.join("openpilot/cereal/include"),
        &schema.join("include"),
    )?;
    let joined = |flag: &str, path: &Path| {
        let mut arg = OsString::from(flag);
        arg.push(path);
        arg
    };
    let mut argv = vec![
        "capnp".into(),
        "compile".into(),
        joined("-I", &schema),
        joined("--src-prefix=", &schema),
        joined("-oc++:", &generated),
    ];
    argv.extend(
        python_glob::glob(&std::env::current_dir()?, &schema, "*.capnp")?
            .into_iter()
            .map(PathBuf::into_os_string),
    );
    let mut spec = CommandSpec::new_os(argv, Duration::MAX);
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    checked(runner, &spec)?;
    let binary = output.join("params-reference");
    let mut argv = vec![
        "g++".into(),
        "-std=c++17".into(),
        "-pthread".into(),
        "-ffunction-sections".into(),
        "-fdata-sections".into(),
        "-Wl,--gc-sections".into(),
        joined("-I", &root.join("openpilot")),
        joined("-I", output),
        joined("-I", &generated),
    ];
    if let Some(prefix) = prefix {
        argv.push(joined("-I", &prefix.join("include")));
    }
    for path in [
        "openpilot/common/params.cc",
        "openpilot/common/util.cc",
        "rust/tools/params_reference.cc",
    ] {
        argv.push(root.join(path).into_os_string());
    }
    argv.extend(["-o".into(), binary.clone().into_os_string()]);
    spec = CommandSpec::new_os(argv, Duration::MAX);
    spec.inherit_output = true;
    spec.inherit_stdin = true;
    checked(runner, &spec)?;
    Ok(binary)
}
fn run(
    runner: &mut dyn ParamsRunner,
    binary: &Path,
    store: &Path,
    operation: &str,
    key: &str,
    value: &[u8],
) -> Result<Vec<u8>> {
    let mut s = CommandSpec::new_os(
        vec![
            binary.as_os_str().to_owned(),
            store.as_os_str().to_owned(),
            "d".into(),
            operation.into(),
            key.into(),
        ],
        Duration::MAX,
    );
    s.input = Some(value.to_vec());
    s.inherit_stderr = true;
    s.stdout_limit = usize::MAX;
    s.stderr_limit = usize::MAX;
    checked(runner, &s)
}
fn require(value: bool, message: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn line_count(bytes: &[u8]) -> usize {
    let mut count = 0;
    let mut at = 0;
    while at < bytes.len() {
        count += 1;
        while at < bytes.len() && bytes[at] != b'\n' && bytes[at] != b'\r' {
            at += 1
        }
        if at < bytes.len() {
            let c = bytes[at];
            at += 1;
            if c == b'\r' && bytes.get(at) == Some(&b'\n') {
                at += 1
            }
        }
    }
    count
}
pub fn check(
    root: &Path,
    rust: &Path,
    prefix: Option<&Path>,
    runner: &mut dyn ParamsRunner,
    printed: &mut dyn Write,
) -> Result<()> {
    let temporary = create_temporary()?;
    let result = check_in(root, rust, prefix, runner, temporary.path()).and_then(|message| {
        printed.write_all(message.as_bytes())?;
        Ok(())
    });
    let path = temporary.keep();
    finish_cleanup(result, || runner.cleanup_temporary(&path))
}
fn exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error)
            if error.raw_os_error().is_some_and(|code| {
                [
                    rustix::io::Errno::NOENT,
                    rustix::io::Errno::NOTDIR,
                    rustix::io::Errno::BADF,
                    rustix::io::Errno::LOOP,
                ]
                .iter()
                .any(|errno| errno.raw_os_error() == code)
            }) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}
fn check_in(
    root: &Path,
    rust: &Path,
    prefix: Option<&Path>,
    runner: &mut dyn ParamsRunner,
    directory: &Path,
) -> Result<String> {
    let native = build_native(root, directory, prefix, runner)?;
    let store = directory.join("store");
    let catalog = run(runner, &native, &store, "catalog", "-", b"")?;
    require(
        run(runner, rust, &store, "catalog", "-", b"")? == catalog,
        "Params catalog differs",
    )?;
    let binary = (0..256 * 1024).map(|n| (n % 256) as u8).collect::<Vec<_>>();
    for (writer, reader) in [(native.as_path(), rust), (rust, native.as_path())] {
        run(runner, writer, &store, "put", "CarParams", &binary)?;
        require(
            run(runner, reader, &store, "get", "CarParams", b"")? == binary,
            "Params binary value differs",
        )?;
        run(runner, writer, &store, "put", "CarParams", b"")?;
        require(
            fs::read(store.join("d/CarParams"))?.is_empty(),
            "Params empty file differs",
        )?;
        require(
            run(runner, reader, &store, "get", "CarParams", b"")?.is_empty(),
            "Params empty get differs",
        )?;
        run(runner, reader, &store, "remove", "CarParams", b"")?;
        require(
            !exists(&store.join("d/CarParams"))?,
            "Params remove retained key",
        )?;
        run(runner, writer, &store, "put", "CarParams", b"transient")?;
        run(runner, writer, &store, "put", "IsMetric", b"1")?;
        fs::write(store.join("d/UnknownOldKey"), b"old")?;
        run(runner, reader, &store, "clear", "4", b"")?;
        require(
            !exists(&store.join("d/CarParams"))?,
            "Params clear retained transient",
        )?;
        require(
            !exists(&store.join("d/UnknownOldKey"))?,
            "Params clear retained unknown",
        )?;
        require(
            run(runner, writer, &store, "get", "IsMetric", b"")? == b"1",
            "Params clear changed persistent",
        )?;
    }
    for writer in [rust, native.as_path()] {
        runner.locked_put(writer, &store)?;
        require(
            fs::read(store.join("d/CarParams"))? == b"lock parity",
            "Params locked write differs",
        )?;
    }
    Ok(format!("PASS: {} Params keys and native/Rust binary, empty, remove, clear and lock interoperability\n",line_count(&catalog)))
}

fn absolute_lexical(path: &Path) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let bytes = path.as_os_str().as_bytes();
    let mut parts = vec![];
    for part in bytes.split(|b| *b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut result = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else {
        b"/".to_vec()
    };
    for part in parts {
        if !result.ends_with(b"/") {
            result.push(b'/')
        }
        result.extend(part)
    }
    Ok(OsString::from_vec(result).into())
}
fn create_temporary() -> Result<tempfile::TempDir> {
    let mut candidates = ["TMPDIR", "TEMP", "TMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    candidates.extend(["/tmp", "/var/tmp", "/usr/tmp"].map(PathBuf::from));
    candidates.push(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    for candidate in candidates {
        let candidate = absolute_lexical(&candidate)?;
        let probe = (|| -> Result<()> {
            let mut file = tempfile::Builder::new()
                .prefix("")
                .rand_bytes(8)
                .tempfile_in(&candidate)?;
            file.write_all(b"blat")?;
            file.close()?;
            Ok(())
        })();
        if probe.is_ok() {
            return Ok(tempfile::Builder::new()
                .prefix("rust-params-reference-")
                .tempdir_in(candidate)?);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No usable temporary directory found in original candidate list",
    )
    .into())
}
fn repair_owned_directories(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if meta.is_dir() {
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(meta.permissions().mode() | 0o700),
        )?;
        for entry in fs::read_dir(path)? {
            repair_owned_directories(&entry?.path())?;
        }
    }
    Ok(())
}
pub fn cleanup_temporary(path: &Path) -> Result<()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if !meta.is_dir() {
        return Err("owned temporary root is no longer a directory".into());
    }
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            repair_owned_directories(path)?;
            fs::remove_dir_all(path)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn wait(child: &mut Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
/// Preserve the original 100ms blocked probe and 10s release wait; unlike the
/// original unbounded Popen-context cleanup, failures kill/reap this owned child.
pub fn locked_put(writer: &Path, store: &Path) -> Result<()> {
    let lock = fs::File::open(store.join(".lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    let mut child = OwnedChild(
        Command::new(writer)
            .arg(store)
            .args(["d", "put", "CarParams"])
            .stdin(Stdio::piped())
            .spawn()?,
    );
    child
        .0
        .stdin
        .take()
        .ok_or("Params child stdin missing")?
        .write_all(b"lock parity")?;
    require(
        wait(&mut child.0, Duration::from_millis(100))?.is_none(),
        "Params child ignored native .lock",
    )?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::Unlock)?;
    require(
        wait(&mut child.0, Duration::from_secs(10))?.is_some_and(|status| status.success()),
        "Params child failed/timed out after native .lock release",
    )?;
    Ok(())
}
struct NativeParamsRunner;
impl Runner for NativeParamsRunner {
    fn run(&mut self, s: &CommandSpec) -> Result<Captured> {
        let argv = s.effective_argv();
        if argv.first().is_some_and(|a| a == "capnp" || a == "g++") {
            let directory = if argv[0] == "capnp" {
                Path::new(&argv[2])
                    .as_os_str()
                    .as_bytes()
                    .strip_prefix(b"-I")
                    .map(|b| Path::new(OsStr::from_bytes(b)))
                    .ok_or("missing schema directory")?
            } else {
                Path::new(argv.last().ok_or("missing compiler output")?)
                    .parent()
                    .ok_or("missing compiler output parent")?
            };
            openpilot_disk_space::inspect(directory, 512 * 1024 * 1024)?.require()?;
        }
        NativeRunner.run(s)
    }
}
impl ParamsRunner for NativeParamsRunner {
    fn locked_put(&mut self, writer: &Path, store: &Path) -> Result<()> {
        locked_put(writer, store)
    }
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", false),
            OptionSpec::value("--capnp-prefix", false),
        ],
    )?;
    if args.help {
        println!("usage: check-params-reference [--binary BINARY] [--capnp-prefix PREFIX]");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = resolve_non_strict(&root)?;
    let binary = args
        .values
        .get("--binary")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("rust/target/debug/examples/store"));
    let binary = resolve_non_strict(&binary)?;
    check(
        &root,
        &binary,
        args.values.get("--capnp-prefix").map(Path::new),
        &mut NativeParamsRunner,
        &mut std::io::stdout().lock(),
    )?;
    Ok(())
}

fn finish_cleanup<T>(body: Result<T>, cleanup: impl FnOnce() -> Result<()>) -> Result<T> {
    cleanup()?;
    body
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_readonly_cleanup_never_follows_external_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let owned = temp.path().join("owned");
        let external = temp.path().join("external");
        fs::create_dir_all(owned.join("readonly")).unwrap();
        fs::write(owned.join("readonly/value"), b"owned").unwrap();
        fs::create_dir(&external).unwrap();
        fs::write(external.join("sentinel"), b"external").unwrap();
        fs::set_permissions(&external, fs::Permissions::from_mode(0o500)).unwrap();
        std::os::unix::fs::symlink(&external, owned.join("readonly/link")).unwrap();
        fs::set_permissions(owned.join("readonly"), fs::Permissions::from_mode(0o500)).unwrap();
        cleanup_temporary(&owned).unwrap();
        assert!(!owned.exists());
        assert_eq!(fs::read(external.join("sentinel")).unwrap(), b"external");
        assert_eq!(
            fs::metadata(&external).unwrap().permissions().mode() & 0o777,
            0o500
        );
        fs::set_permissions(external, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    fn original_cleanup_failure_is_not_ignored_and_overrides_body_failure() {
        for body in [Ok(7), Err("body failure".into())] {
            let called = std::cell::Cell::new(false);
            let result = finish_cleanup(body, || {
                called.set(true);
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "controlled cleanup failure",
                )
                .into())
            });
            let error = result.unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .map(std::io::Error::kind),
                Some(std::io::ErrorKind::PermissionDenied)
            );
            assert!(called.get());
        }
    }
}
