//! Actual inline C++ Params::getFloat versus the supplied native Params probe.
//! No calibration runtime policy or captured output supplies expected values.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{self, Read, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const HEADER: &str = "openpilot/common/params.h";
pub const CHECKER: &str = "rust/tools/check_calibration_float.py";
pub const HEADER_SHA256: &str = "47d08fa1987c9150e55d09a2a533a0a115851aadb34bd1a0c82325e0be8726b3";
pub const CHECKER_SHA256: &str = "922e055f3e9b8fda4e732e83b5332105b5203bd9f56964bf5515e3557c9ff304";
pub const SUCCESS: &str =
    "PASS: 25 original getFloat cases, exact Float32 promotion and exceptions\n";
pub const HELP: &str = "Usage: check-calibration-float --binary BINARY --output OUTPUT\n";

pub fn inputs() -> &'static [&'static [u8]] {
    &[
        b"",
        b"0",
        b"-0",
        b"-0.0",
        b"1",
        b"  -12.3456789tail",
        b"0.0001",
        b"0.000100000001",
        b"0.00010000000474974513",
        b"0x1.8p+2",
        b"nan",
        b"NAN(payload)",
        b"+INF",
        b"-infinity",
        b"1\0garbage",
        b"\0",
        b"garbage",
        b" ",
        b"1e99",
        b"1e-99",
        b"1e-45",
        b"1.17549435e-38",
        b"3.40282346e+38",
        b"3.4028236e+38",
        b"1e",
    ]
}

/// Extract at the same textual boundaries as the unchanged source checker.
pub fn source_program(header: &str) -> Result<String> {
    let start = header
        .find("  inline float getFloat(")
        .ok_or("missing getFloat source")?;
    let end = header[start..]
        .find("\n  }")
        .ok_or("unterminated getFloat source")?
        + start
        + "\n  }".len();
    Ok(format!("#include <string>\n#include <iostream>\n#include <iterator>\n#include <iomanip>\nstruct Source {{\n  std::string value;\n  std::string get(const std::string&, bool) {{ return value; }}\n{}\n}};\nint main() {{\n  Source params;\n  params.value = std::string(std::istreambuf_iterator<char>(std::cin), {{}});\n  try {{ std::cout << std::setprecision(17) << double(params.getFloat(\"CameraYawTrimDeg\")); }}\n  catch(const std::exception&) {{ std::cout << \"ERROR\"; }}\n}}\n", &header[start..end]))
}

fn strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
// Python float() accepts Unicode decimal digits and underscores between digits.
// This parses the two child responses, never a replacement getFloat policy.
fn response_float(text: &str) -> Result<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut normalized = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' {
            if i == 0
                || i + 1 == chars.len()
                || crate::argparse_scan::decimal_value(chars[i - 1]).is_none()
                || crate::argparse_scan::decimal_value(chars[i + 1]).is_none()
            {
                return Err("invalid float response underscore".into());
            }
        } else if let Some(digit) = crate::argparse_scan::decimal_value(ch) {
            normalized.push(char::from(b'0' + digit));
        } else {
            normalized.push(ch);
        }
    }
    let lower = normalized.to_ascii_lowercase();
    match lower.as_str() {
        "nan" | "+nan" | "-nan" => Ok(f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
        "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
        _ => Ok(normalized.parse()?),
    }
}
pub fn compare(source: &str, native: &str) -> Result<()> {
    if source == "ERROR" {
        if native != "ERROR" {
            return Err("source exception was not preserved".into());
        }
        return Ok(());
    }
    let expected = response_float(source)?;
    let actual = response_float(native)?;
    if !(expected == actual || expected.is_nan() && actual.is_nan()) {
        return Err(format!("getFloat mismatch: source={source:?}, native={native:?}").into());
    }
    if expected == 0. && expected.is_sign_negative() != actual.is_sign_negative() {
        return Err("getFloat signed zero mismatch".into());
    }
    Ok(())
}

struct Artifact {
    file: File,
    dev: u64,
    ino: u64,
    sha: String,
}
impl Artifact {
    fn open(path: &Path) -> Result<Self> {
        Self::open_flags(path, rustix::fs::OFlags::empty())
    }
    fn open_source(path: &Path) -> Result<Self> {
        Self::open_flags(path, rustix::fs::OFlags::NOFOLLOW)
    }
    fn open_flags(path: &Path, extra: rustix::fs::OFlags) -> Result<Self> {
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC
                | extra,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("getFloat input must be a regular file".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            hash.update(&buffer[..size]);
        }
        Ok(Self {
            file,
            dev: metadata.dev(),
            ino: metadata.ino(),
            sha: format!("{:x}", hash.finalize()),
        })
    }
    fn verify(&self, path: &Path) -> Result<()> {
        let after = Self::open(path)?;
        if (self.dev, self.ino, &self.sha) != (after.dev, after.ino, &after.sha) {
            return Err("getFloat input changed during execution".into());
        }
        Ok(())
    }
    fn verify_source(&self, path: &Path) -> Result<()> {
        let after = Self::open_source(path)?;
        if (self.dev, self.ino, &self.sha) != (after.dev, after.ino, &after.sha) {
            return Err("getFloat input changed during execution".into());
        }
        Ok(())
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        let mut bytes = vec![0; usize::try_from(self.file.metadata()?.len())?];
        self.file.read_exact_at(&mut bytes, 0)?;
        Ok(bytes)
    }
}

struct SourceParent {
    path: PathBuf,
    file: File,
    dev: u64,
    ino: u64,
}
impl SourceParent {
    fn open(path: &Path) -> Result<Self> {
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
    fn verify(&self) -> Result<()> {
        let current = Self::open(&self.path)?;
        let held = self.file.metadata()?;
        if (self.dev, self.ino) != (current.dev, current.ino)
            || (self.dev, self.ino) != (held.dev(), held.ino())
        {
            return Err("getFloat historical source parent changed".into());
        }
        Ok(())
    }
}

struct Historical {
    path: PathBuf,
    parent: SourceParent,
    present: Option<Artifact>,
}
impl Historical {
    fn bind(root: &Path, proof: &Value) -> Result<Self> {
        let path = root.join(CHECKER);
        let parent = SourceParent::open(path.parent().ok_or("missing checker parent")?)?;
        let present = if proof["source_sha256"].get(CHECKER).is_some() {
            let checker = Artifact::open_source(&path)?;
            if checker.sha != CHECKER_SHA256 {
                return Err("getFloat checker source changed".into());
            }
            Some(checker)
        } else {
            Self::verify_absence(&path)?;
            None
        };
        parent.verify()?;
        Ok(Self {
            path,
            parent,
            present,
        })
    }
    fn verify_absence(path: &Path) -> Result<()> {
        SourceParent::open(path.parent().ok_or("missing checker parent")?)?;
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
            Ok(_) => Err("getFloat historical checker reappeared".into()),
        }
    }
    fn verify(&self) -> Result<()> {
        self.parent.verify()?;
        match &self.present {
            Some(checker) => checker.verify_source(&self.path),
            None => Self::verify_absence(&self.path),
        }
    }
}

struct Bindings {
    root: PathBuf,
    proof: Value,
    historical: Historical,
    header: Artifact,
    candidate_path: PathBuf,
    candidate: Artifact,
    compiler_path: PathBuf,
    compiler: Artifact,
    reference_path: PathBuf,
    reference: Artifact,
    executing_path: PathBuf,
    executing: Artifact,
}
impl Bindings {
    fn verify(&self, cancel: &AtomicBool) -> Result<()> {
        self.candidate.verify(&self.candidate_path)?;
        self.compiler.verify(&self.compiler_path)?;
        self.reference.verify(&self.reference_path)?;
        self.header.verify_source(&self.root.join(HEADER))?;
        self.executing.verify(&self.executing_path)?;
        self.historical.verify()?;
        if self.proof != sources(&self.root)? {
            return Err("getFloat source presence changed".into());
        }
        cancelled(cancel)
    }
}

struct Pending {
    report: Value,
    identity: Publication,
    result: Publication,
    bindings: Bindings,
}

/// The retained header is always mandatory. Only genuine checker absence is
/// accepted for eventual retirement; a dangling symlink is never absence.
pub fn sources(root: &Path) -> Result<Value> {
    let header = Artifact::open_source(&root.join(HEADER))?;
    if header.sha != HEADER_SHA256 {
        return Err("Params header source changed".into());
    }
    let mut live = serde_json::Map::new();
    let mut historical = serde_json::Map::new();
    live.insert(HEADER.into(), json!(header.sha));
    match fs::symlink_metadata(root.join(CHECKER)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Historical::verify_absence(&root.join(CHECKER))?;
            historical.insert(CHECKER.into(), json!(CHECKER_SHA256));
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {
            let checker = Artifact::open_source(&root.join(CHECKER))?;
            if checker.sha != CHECKER_SHA256 {
                return Err("getFloat checker source changed".into());
            }
            live.insert(CHECKER.into(), json!(checker.sha));
        }
    }
    Ok(json!({"source_sha256":live,"historical_source_sha256":historical}))
}

#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
pub fn parse(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: resolve_non_strict(Path::new(&parsed.values["--binary"]))?,
        output: resolve_non_strict(Path::new(&parsed.values["--output"]))?,
    }))
}
fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        return Err(io::Error::from(io::ErrorKind::Interrupted).into());
    }
    Ok(())
}
fn child(
    runner: &mut dyn Runner,
    command: &CommandSpec,
    output: &Path,
    label: &str,
) -> Result<Captured> {
    let result = runner.run(command);
    match &result {
        Ok(captured) => {
            fs::write(output.join(format!("{label}.stdout")), &captured.stdout)?;
            fs::write(output.join(format!("{label}.stderr")), &captured.stderr)?;
            if captured.code != 0 {
                return Err(format!("getFloat {label} exited {}", captured.code).into());
            }
        }
        Err(error) => {
            if let Some(crate::qa_process::CaptureError::Timeout { stdout, stderr, .. }) =
                error.downcast_ref()
            {
                fs::write(output.join(format!("{label}.stdout")), stdout)?;
                fs::write(output.join(format!("{label}.stderr")), stderr)?;
            }
        }
    }
    result
}
struct Publication {
    path: PathBuf,
    file: File,
    committed: bool,
}
impl Publication {
    fn write(path: &Path, report: &Value) -> Result<Self> {
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().ok_or("missing report parent")?)?;
        serde_json::to_writer_pretty(&mut temporary, report)?;
        temporary.write_all(b"\n")?;
        temporary.flush()?;
        let file = temporary
            .persist_noclobber(path)
            .map_err(|error| error.error)?;
        Ok(Self {
            path: path.into(),
            file,
            committed: false,
        })
    }
    fn owns(&self) -> bool {
        let Ok(original) = self.file.metadata() else {
            return false;
        };
        fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_file() && (m.dev(), m.ino()) == (original.dev(), original.ino()))
    }
}
impl Drop for Publication {
    fn drop(&mut self) {
        if !self.committed && self.owns() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn compiler_in(search: Option<&OsStr>, cwd: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    // CPython's POSIX exec search uses os.defpath only when PATH is absent.
    // An explicitly empty PATH still searches the current working directory.
    for directory in std::env::split_paths(search.unwrap_or(OsStr::new("/bin:/usr/bin"))) {
        let path = cwd.join(directory).join("g++");
        if fs::metadata(&path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0) {
            // Compiler aliases can select the C++ link driver through argv[0].
            // Pin the followed regular inode without changing that invocation.
            return Ok(path);
        }
    }
    Err("g++ prerequisite is missing".into())
}
fn compiler() -> Result<PathBuf> {
    compiler_in(
        std::env::var_os("PATH").as_deref(),
        &std::env::current_dir()?,
    )
}

fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Pending> {
    // Exclusive admission precedes reading sources or touching any old receipt.
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    cancelled(cancel)?;
    let proof = sources(&options.root)?;
    let historical = Historical::bind(&options.root, &proof)?;
    let header = Artifact::open_source(&options.root.join(HEADER))?;
    let program = source_program(&String::from_utf8(header.bytes()?)?)?;
    let source = options.output.join("source_get_float.cc");
    fs::write(&source, &program)?;
    let reference = options.output.join("source-get-float");
    let compiler = compiler()?;
    let compiler_identity = Artifact::open(&compiler)?;
    let executing_path = std::env::current_exe()?;
    let executing_identity = Artifact::open(&executing_path)?;
    // Defer unavailable/nonregular candidate identity until its original spawn
    // phase. A successful report still requires a held regular-file identity.
    let candidate = Artifact::open(&options.binary);
    let mut compile = CommandSpec::new_os(
        vec![
            compiler.clone().into_os_string(),
            "-std=c++17".into(),
            "-O2".into(),
            source.into_os_string(),
            "-o".into(),
            reference.clone().into_os_string(),
        ],
        Duration::from_secs(60),
    );
    // Keep the original subprocess argv[0] while executing the hash-bound compiler.
    compile.arg0 = Some("g++".into());
    child(runner, &compile, &options.output, "compiler")?;
    compiler_identity.verify(&compiler)?;
    cancelled(cancel)?;
    let source_identity = Artifact::open(&reference)?;
    let mut rows = Vec::new();
    for (index, input) in inputs().iter().enumerate() {
        cancelled(cancel)?;
        let mut expected_command = CommandSpec::new_os(
            vec![reference.clone().into_os_string()],
            Duration::from_secs(5),
        );
        expected_command.input = Some(input.to_vec());
        let expected = child(
            runner,
            &expected_command,
            &options.output,
            &format!("{index:02}-source"),
        )?;
        let mut actual_command = CommandSpec::new_os(
            vec![options.binary.clone().into_os_string()],
            Duration::from_secs(5),
        );
        actual_command.input = Some(input.to_vec());
        let actual = child(
            runner,
            &actual_command,
            &options.output,
            &format!("{index:02}-native"),
        )?;
        let expected_text = String::from_utf8(expected.stdout)?;
        let actual_text = String::from_utf8(actual.stdout)?;
        let expected = strip(&expected_text);
        let actual = strip(&actual_text);
        compare(expected, actual)
            .map_err(|error| format!("case {index}, input {}: {error}", hex(input)))?;
        rows.push(json!({"input_hex":hex(input),"source":expected,"rust":actual}));
    }
    let candidate = candidate?;
    let bindings = Bindings {
        root: options.root.clone(),
        proof: proof.clone(),
        historical,
        header,
        candidate_path: options.binary.clone(),
        candidate,
        compiler_path: compiler,
        compiler: compiler_identity,
        reference_path: reference,
        reference: source_identity,
        executing_path,
        executing: executing_identity,
    };
    bindings.verify(cancel)?;
    // A broken output pipe cannot leave a successful report behind.
    stdout.write_all(SUCCESS.as_bytes())?;
    stdout.flush()?;
    let report = json!({"result":"pass","cases":rows});
    let provenance = json!({"sources":proof,"binary_sha256":bindings.candidate.sha,"compiler_sha256":bindings.compiler.sha,"reference_sha256":bindings.reference.sha,"executing_checker_sha256":bindings.executing.sha,"generated_source_sha256":format!("{:x}",Sha256::digest(program.as_bytes())),"binary_path_bytes":options.binary.as_os_str().as_bytes(),"compile_program_bytes":bindings.compiler_path.as_os_str().as_bytes(),"compile_argv_bytes":compile.child_argv().iter().map(|arg|arg.as_bytes().to_vec()).collect::<Vec<_>>(),"child_bounds_seconds":{"compiler":60,"probe":5}});
    let identity = Publication::write(&options.output.join("provenance.json"), &provenance)?;
    let result = Publication::write(&options.output.join("report.json"), &report)?;
    bindings.verify(cancel)?;
    if !identity.owns() || !result.owns() {
        return Err("getFloat report replaced before commit".into());
    }
    cancelled(cancel)?;
    Ok(Pending {
        report,
        identity,
        result,
        bindings,
    })
}
fn commit(mut pending: Pending, cancel: &AtomicBool) -> Result<Value> {
    pending.bindings.verify(cancel)?;
    if !pending.identity.owns() || !pending.result.owns() {
        return Err("getFloat report replaced before commit".into());
    }
    pending.identity.committed = true;
    pending.result.committed = true;
    Ok(pending.report)
}
pub fn check(
    options: &Options,
    runner: &mut dyn Runner,
    stdout: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<Value> {
    let pending = execute(options, runner, stdout, cancel)?;
    cancelled(cancel)?;
    commit(pending, cancel)
}

struct Controlled<'a>(&'a AtomicBool);
impl Runner for Controlled<'_> {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(command, Instant::now() + command.timeout, self.0)
    }
}
fn finish_cli(
    result: Result<Pending>,
    cancel: &AtomicBool,
    unregister: impl FnOnce(),
) -> Result<Value> {
    // Keep pending descriptors/rollback guards alive while handlers are removed.
    // A handler can latch cancellation during removal, after execute returns.
    unregister();
    cancelled(cancel)?;
    commit(result?, cancel)
}
pub fn cli() -> Result<()> {
    let Some(options) = parse(&std::env::args_os().skip(1).collect::<Vec<_>>())? else {
        io::stdout().write_all(HELP.as_bytes())?;
        return Ok(());
    };
    let flag = Arc::new(AtomicBool::new(false));
    let mut handlers = Vec::new();
    let register = (|| -> Result<()> {
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            handlers.push(signal_hook::flag::register(signal, Arc::clone(&flag))?);
        }
        Ok(())
    })();
    let result =
        register.and_then(|()| execute(&options, &mut Controlled(&flag), &mut io::stdout(), &flag));
    finish_cli(result, &flag, || {
        for handler in handlers {
            signal_hook::low_level::unregister(handler);
        }
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct ProbeRunner;
    impl Runner for ProbeRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
            if command.timeout == Duration::from_secs(60) {
                let reference = PathBuf::from(command.effective_argv().last().unwrap().clone());
                fs::write(reference, b"private lifecycle fixture")?;
            }
            Ok(Captured {
                code: 0,
                stdout: b"0".to_vec(),
                stderr: Vec::new(),
            })
        }
    }
    fn options_in(directory: &Path) -> Options {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let header = directory.join(HEADER);
        fs::create_dir_all(header.parent().unwrap()).unwrap();
        fs::copy(repository.join(HEADER), &header).unwrap();
        fs::create_dir_all(directory.join(CHECKER).parent().unwrap()).unwrap();
        let binary = directory.join("candidate");
        fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
        Options {
            root: directory.to_path_buf(),
            binary,
            output: directory.join("evidence"),
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn fingerprinting_a_fifo_never_waits_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fifo");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &path,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(Artifact::open(&path).is_err());
    }
    #[test]
    fn pending_publication_rolls_back_own_inode_but_preserves_replacements() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("report.json");
        let pending = Publication::write(&path, &json!({"result":"pass"})).unwrap();
        drop(pending);
        assert!(!path.exists());
        let pending = Publication::write(&path, &json!({"result":"pass"})).unwrap();
        fs::rename(&path, directory.path().join("held-old-report")).unwrap();
        fs::write(&path, b"foreign replacement").unwrap();
        assert!(!pending.owns());
        drop(pending);
        assert_eq!(fs::read(&path).unwrap(), b"foreign replacement");
        assert!(Publication::write(&path, &json!({"result":"pass"})).is_err());
        assert_eq!(fs::read(path).unwrap(), b"foreign replacement");
    }
    #[test]
    fn empty_compiler_path_searches_cwd_but_unset_uses_posix_default() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let local = directory.path().join("g++");
        // Lookup-only fixture; never executed or put on the global environment.
        fs::write(&local, b"private lookup control").unwrap();
        fs::set_permissions(&local, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            compiler_in(Some(OsStr::new("")), directory.path()).unwrap(),
            local
        );
        let default = compiler_in(None, directory.path()).unwrap();
        assert_eq!(
            default,
            compiler_in(Some(OsStr::new("/bin:/usr/bin")), directory.path()).unwrap()
        );
        assert_ne!(default, local);
    }
    #[test]
    fn cancellation_latched_during_handler_removal_rolls_back_only_owned_receipts() {
        for replaced in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let cancel = AtomicBool::new(false);
            let options = options_in(directory.path());
            let pending = execute(&options, &mut ProbeRunner, &mut Vec::new(), &cancel).unwrap();
            let provenance = options.output.join("provenance.json");
            let report = options.output.join("report.json");
            if replaced {
                fs::remove_file(&report).unwrap();
                fs::write(&report, b"foreign replacement").unwrap();
            }
            let error = finish_cli(Ok(pending), &cancel, || {
                cancel.store(true, Ordering::Relaxed);
            })
            .unwrap_err();
            assert_eq!(
                error.downcast_ref::<io::Error>().unwrap().kind(),
                io::ErrorKind::Interrupted
            );
            assert!(!provenance.exists());
            if replaced {
                assert_eq!(fs::read(&report).unwrap(), b"foreign replacement");
            } else {
                assert!(!report.exists());
            }
        }
    }

    #[test]
    fn historical_reappearance_during_handler_teardown_cannot_commit_receipts() {
        // #437: exercise the actual gap after execute's last verification.
        for replacement in ["file", "dangling", "fifo"] {
            let directory = tempfile::tempdir().unwrap();
            let cancel = AtomicBool::new(false);
            let options = options_in(directory.path());
            let pending = execute(&options, &mut ProbeRunner, &mut Vec::new(), &cancel).unwrap();
            let historical = options.root.join(CHECKER);
            let error = finish_cli(Ok(pending), &cancel, || match replacement {
                "file" => fs::write(&historical, b"reappeared").unwrap(),
                "dangling" => std::os::unix::fs::symlink("missing", &historical).unwrap(),
                "fifo" => rustix::fs::mkfifoat(
                    rustix::fs::CWD,
                    &historical,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                )
                .unwrap(),
                _ => unreachable!(),
            })
            .unwrap_err();
            assert!(error.to_string().contains("historical checker reappeared"));
            assert!(!options.output.join("provenance.json").exists());
            assert!(!options.output.join("report.json").exists());
        }
    }

    #[test]
    fn historical_parent_replacement_during_teardown_cannot_commit_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let options = options_in(directory.path());
        let pending = execute(&options, &mut ProbeRunner, &mut Vec::new(), &cancel).unwrap();
        let parent = options.root.join(CHECKER).parent().unwrap().to_path_buf();
        let held = options.root.join("rust/old-tools");
        let error = finish_cli(Ok(pending), &cancel, || {
            fs::rename(&parent, &held).unwrap();
            fs::create_dir(&parent).unwrap();
        })
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("historical source parent changed"));
        assert!(!options.output.join("report.json").exists());
        assert!(!options.output.join("provenance.json").exists());
    }

    #[test]
    fn finalization_rechecks_held_inputs_and_preserves_foreign_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let options = options_in(directory.path());
        let pending = execute(&options, &mut ProbeRunner, &mut Vec::new(), &cancel).unwrap();
        let report = finish_cli(Ok(pending), &cancel, || {}).unwrap();
        assert_eq!(report["cases"].as_array().unwrap().len(), inputs().len());

        for changed in [HEADER, "candidate"] {
            let directory = tempfile::tempdir().unwrap();
            let cancel = AtomicBool::new(false);
            let options = options_in(directory.path());
            let pending = execute(&options, &mut ProbeRunner, &mut Vec::new(), &cancel).unwrap();
            let changed_path = if changed == HEADER {
                options.root.join(HEADER)
            } else {
                options.binary.clone()
            };
            let receipt = options.output.join("report.json");
            let error = finish_cli(Ok(pending), &cancel, || {
                fs::write(&changed_path, b"changed after publication").unwrap();
                fs::remove_file(&receipt).unwrap();
                fs::write(&receipt, b"foreign receipt").unwrap();
            })
            .unwrap_err();
            assert!(error.to_string().contains("getFloat input changed"));
            assert!(!options.output.join("provenance.json").exists());
            assert_eq!(fs::read(receipt).unwrap(), b"foreign receipt");
        }
    }
}
