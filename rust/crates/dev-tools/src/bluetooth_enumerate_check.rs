//! Independent sysfs enumeration oracle for the retained Python 3.12 checker.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeMap,
    Deserialize, Deserializer, Serialize, Serializer,
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

// CPython 3.12 _PyUnicode_IsWhitespace (Unicode 15); deliberately independent
// of Rust char::is_whitespace and of the candidate's strip implementation.
const SPACES: [u32; 29] = [
    9, 10, 11, 12, 13, 28, 29, 30, 31, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197,
    8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288,
];
const HISTORICAL_CHECKER: &str = "rust/tools/check_bluetooth_enumerate.py";
const HISTORICAL_SHA: &str = "2adaabcdf0ed4f6ccab1e466fa095e0c32b0b956c7de2cca833fbc0ef55e9fe0";
const ORIGINAL_REPORT: &[u8] =
    include_bytes!("../tests/fixtures/bluetooth-enumerate/original-result.json");
const ORIGINAL_REPORT_SHA: &str =
    "36d25b8684cb814421dd22ada7bc75b3d9959c38433e4a5b75c70e02375ac18c";
const ORIGINAL_DEVICES: &str =
    "/home/runner/work/_temp/ui-connectivity/bluetooth-enumerate/original/dev/";
const SOURCES: [(&str, &str); 6] = [
    (HISTORICAL_CHECKER, HISTORICAL_SHA),
    (
        "openpilot/selfdrive/carrot/bluetooth/daemon.py",
        "d032d6069416bae35aea07e25f28f9930795f29cf0a9518c122bfecb6cebe4d3",
    ),
    (
        "openpilot/selfdrive/carrot/bluetooth/model.py",
        "74f65767da6459358c8809f6e0adf465a4e7007b4bbf4e160b23c98819e6a633",
    ),
    (
        "rust/tools/bluetooth_engine_source.py",
        "bcdbbe3e595306360fe78751443d0ad6b5a87cf5faff0bafa21bca542ed3053d",
    ),
    (
        "rust/crates/bluetooth/examples/input_fixture.rs",
        "e108d207066869ea8c408b177bcc219052d8406beb7f29f380c21e6d25f7e0f9",
    ),
    (
        "rust/crates/bluetooth/src/input.rs",
        "327e17059a19663b121c08c64b493f35d90be5798b7c841f04180f65d6b809e4",
    ),
];
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn identity(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn verify_held(path: &Path, file: &File, hash: &str) -> Result<()> {
    let before = file.metadata()?;
    if !before.is_file() || identity(&before) != identity(&fs::metadata(path)?) {
        return Err(format!("enumeration artifact changed: {}", path.display()).into());
    }
    let mut held = file.try_clone()?;
    held.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    held.take(before.len().saturating_add(1))
        .read_to_end(&mut bytes)?;
    if hash != sha(&bytes)
        || identity(&before) != identity(&file.metadata()?)
        || identity(&before) != identity(&fs::metadata(path)?)
    {
        return Err(format!("enumeration artifact changed: {}", path.display()).into());
    }
    Ok(())
}
struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
}
impl Bound {
    fn open(path: &Path) -> Result<Self> {
        let mut file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(format!("not a regular file: {}", path.display()).into());
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let bound = Self {
            path: path.into(),
            file,
            metadata,
            hash: sha(&bytes),
        };
        bound.verify()?;
        Ok(bound)
    }
    fn verify(&self) -> Result<()> {
        if identity(&self.metadata) != identity(&self.file.metadata()?) {
            return Err(format!("enumeration input changed: {}", self.path.display()).into());
        }
        verify_held(&self.path, &self.file, &self.hash)
    }
}
enum Historical {
    Present(Bound),
    Absent {
        path: PathBuf,
        parent: PathBuf,
        directory: File,
        device: u64,
        inode: u64,
    },
}
impl Historical {
    fn open(root: &Path) -> Result<Self> {
        let path = root.join(HISTORICAL_CHECKER);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err("historical enumeration checker is not regular".into());
                }
                let bound = Bound::open(&path)?;
                if bound.hash != HISTORICAL_SHA {
                    return Err("historical enumeration checker changed".into());
                }
                Ok(Self::Present(bound))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let parent = path
                    .parent()
                    .ok_or("historical checker parent missing")?
                    .to_owned();
                let directory = File::from(rustix::fs::open(
                    &parent,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::DIRECTORY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?);
                let metadata = directory.metadata()?;
                let absent = Self::Absent {
                    path,
                    parent,
                    directory,
                    device: metadata.dev(),
                    inode: metadata.ino(),
                };
                absent.verify()?;
                Ok(absent)
            }
            Err(error) => Err(error.into()),
        }
    }
    fn verify(&self) -> Result<()> {
        match self {
            Self::Present(bound) => {
                if !fs::symlink_metadata(&bound.path)?.is_file() {
                    return Err("historical enumeration checker changed type".into());
                }
                bound.verify()
            }
            Self::Absent {
                path,
                parent,
                directory,
                device,
                inode,
            } => {
                let held = directory.metadata()?;
                let current = fs::symlink_metadata(parent)?;
                if !held.is_dir()
                    || !current.is_dir()
                    || (held.dev(), held.ino()) != (*device, *inode)
                    || (current.dev(), current.ino()) != (*device, *inode)
                {
                    return Err("historical enumeration checker parent changed".into());
                }
                match fs::symlink_metadata(path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Ok(_) => Err("historical enumeration checker reappeared".into()),
                    Err(error) => Err(error.into()),
                }
            }
        }
    }
}
fn check(bounds: &[Bound], historical: &Historical, cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        return Err("enumeration interrupted".into());
    }
    historical.verify()?;
    for bound in bounds {
        bound.verify()?;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ordered(Vec<(String, String)>);
impl Serialize for Ordered {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct OrderedVisitor;
        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = Ordered;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("string-to-string object")
            }
            fn visit_map<M: MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Ordered, M::Error> {
                let mut rows = Vec::new();
                while let Some(row) = map.next_entry::<String, String>()? {
                    if rows.iter().any(|(key, _)| key == &row.0) {
                        return Err(serde::de::Error::custom("duplicate device path"));
                    }
                    rows.push(row);
                }
                Ok(Ordered(rows))
            }
        }
        deserializer.deserialize_map(OrderedVisitor)
    }
}
#[derive(Deserialize)]
struct OriginalReport {
    nodes: usize,
    accepted: usize,
    source: Ordered,
    native: Ordered,
    source_sha256: String,
}
fn original_rows(devices: &Path) -> Result<Ordered> {
    if sha(ORIGINAL_REPORT) != ORIGINAL_REPORT_SHA {
        return Err("original enumeration observation changed".into());
    }
    let report: OriginalReport = serde_json::from_slice(ORIGINAL_REPORT)?;
    if report.nodes != 40
        || report.accepted != 31
        || report.source_sha256 != SOURCES[1].1
        || report.source != report.native
        || report.source.0.len() != 31
    {
        return Err("original enumeration observation invalid".into());
    }
    Ok(Ordered(
        report
            .source
            .0
            .into_iter()
            .map(|(path, address)| {
                let name = path
                    .strip_prefix(ORIGINAL_DEVICES)
                    .ok_or("original enumeration device path changed")?;
                let current = devices.join(name);
                Ok((
                    current
                        .to_str()
                        .ok_or("non-UTF-8 enumeration device path")?
                        .to_owned(),
                    address,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
    ))
}
fn same_device_pairs(left: &Ordered, right: &Ordered) -> bool {
    left.0
        .iter()
        .cloned()
        .collect::<std::collections::BTreeMap<_, _>>()
        == right
            .0
            .iter()
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>()
}
fn trim_python(value: &str) -> &str {
    value.trim_matches(|c| SPACES.contains(&(c as u32)))
}
fn mac(value: &str) -> Option<String> {
    let value = trim_python(value).to_uppercase();
    let bytes = value.as_bytes();
    (bytes.len() == 17
        && bytes.iter().enumerate().all(|(i, b)| {
            if [2, 5, 8, 11, 14].contains(&i) {
                *b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        }))
    .then_some(value)
}
struct Case {
    name: String,
    bus: Option<Vec<u8>>,
    address: Option<Vec<u8>>,
    label: Option<Vec<u8>>,
}
type FixedCase = (
    &'static str,
    Option<&'static [u8]>,
    Option<&'static [u8]>,
    Option<&'static [u8]>,
);
fn cases() -> Vec<Case> {
    let fixed: [FixedCase; 11] = [
        (
            "event0",
            Some(b"0005"),
            Some(b"aa:bb:cc:dd:ee:ff"),
            Some(b"remote"),
        ),
        (
            "event9",
            Some(b"0003"),
            Some(b"AA:BB:CC:DD:EE:01"),
            Some(b"USB"),
        ),
        ("event2", Some(b"0005"), Some(b"invalid"), Some(b"remote")),
        ("event3", Some(b"0005"), Some(b"AA:BB:CC:DD:EE:02"), None),
        (
            "event4",
            Some(b"0005"),
            Some(b"AA:BB:CC:DD:EE:03"),
            Some(b"\xff"),
        ),
        (
            "mouse0",
            Some(b"0005"),
            Some(b"AA:BB:CC:DD:EE:04"),
            Some(b"remote"),
        ),
        ("event5", None, Some(b"AA:BB:CC:DD:EE:05"), Some(b"remote")),
        ("event6", Some(b"0005"), None, Some(b"remote")),
        (
            "event7",
            Some(b"\xff"),
            Some(b"AA:BB:CC:DD:EE:07"),
            Some(b"remote"),
        ),
        ("event8", Some(b"0005"), Some(b"\xff"), Some(b"remote")),
        (
            "event",
            Some(b"0005"),
            Some(b"AA:BB:CC:DD:EE:08"),
            Some(b""),
        ),
    ];
    let mut result = fixed
        .into_iter()
        .map(|(name, bus, address, label)| Case {
            name: name.into(),
            bus: bus.map(Vec::from),
            address: address.map(Vec::from),
            label: label.map(Vec::from),
        })
        .collect::<Vec<_>>();
    for (index, point) in SPACES.into_iter().enumerate() {
        let c = char::from_u32(point).expect("valid whitespace codepoint");
        result.push(Case {
            name: format!("event-space-{index}"),
            bus: Some(format!("{c}0005{c}").into_bytes()),
            address: Some(format!("{c}aa:bb:cc:dd:ee:ff{c}").into_bytes()),
            label: Some(format!("{c}remote{c}").into_bytes()),
        });
    }
    result
}
fn write_nodes(sysfs: &Path, cases: &[Case]) -> Result<Vec<Bound>> {
    fs::create_dir(sysfs)?;
    let mut held = Vec::new();
    for case in cases {
        let device = sysfs.join(&case.name).join("device");
        fs::create_dir_all(device.join("id"))?;
        for (name, bytes) in [
            ("id/bustype", &case.bus),
            ("uniq", &case.address),
            ("name", &case.label),
        ] {
            if let Some(bytes) = bytes {
                let path = device.join(name);
                fs::write(&path, bytes)?;
                held.push(Bound::open(&path)?);
            }
        }
    }
    Ok(held)
}
fn verify_tree(root: &Path, corpus: &[Case]) -> Result<()> {
    if fs::symlink_metadata(root.join("absent")).is_ok() {
        return Err("enumeration absent directory was created".into());
    }
    let sysfs = root.join("sysfs");
    let mut actual = fs::read_dir(&sysfs)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut names = corpus
        .iter()
        .map(|case| case.name.as_ref())
        .collect::<Vec<&str>>();
    actual.sort();
    names.sort();
    if actual
        .iter()
        .map(|name| name.to_str())
        .collect::<Option<Vec<_>>>()
        .as_deref()
        != Some(names.as_slice())
    {
        return Err("enumeration sysfs node set changed".into());
    }
    for case in corpus {
        let directory = sysfs.join(&case.name).join("device");
        let entries = fs::read_dir(&directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut expected = vec!["id"];
        if case.address.is_some() {
            expected.push("uniq");
        }
        if case.label.is_some() {
            expected.push("name");
        }
        let mut observed = entries
            .iter()
            .map(|name| name.to_str())
            .collect::<Option<Vec<_>>>()
            .ok_or("invalid enumeration node name")?;
        observed.sort();
        expected.sort();
        if observed != expected {
            return Err("enumeration device files changed".into());
        }
        let id = fs::read_dir(directory.join("id"))?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let id = id
            .iter()
            .map(|name| name.to_str())
            .collect::<Option<Vec<_>>>()
            .ok_or("invalid enumeration id name")?;
        if id
            != if case.bus.is_some() {
                vec!["bustype"]
            } else {
                vec![]
            }
        {
            return Err("enumeration bus files changed".into());
        }
    }
    Ok(())
}
fn expected(sysfs: &Path, devices: &Path) -> Result<Ordered> {
    let mut rows = Vec::new();
    // Directory iteration, rather than case construction order, matches Path.glob.
    for entry in fs::read_dir(sysfs)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.as_encoded_bytes().starts_with(b"event") {
            continue;
        }
        let device = entry.path().join("device");
        let Some(bus) = fs::read_to_string(device.join("id/bustype")).ok() else {
            continue;
        };
        if trim_python(&bus) != "0005" {
            continue;
        }
        let Some(address) = fs::read_to_string(device.join("uniq")).ok() else {
            continue;
        };
        let Some(address) = mac(&address) else {
            continue;
        };
        if fs::read_to_string(device.join("name")).is_err() {
            continue;
        }
        let path = devices.join(&name);
        rows.push((
            path.to_str()
                .ok_or("non-UTF-8 enumeration output path")?
                .to_owned(),
            address,
        ));
    }
    Ok(Ordered(rows))
}
#[derive(Serialize)]
struct Report<'a> {
    nodes: usize,
    accepted: usize,
    source: &'a Ordered,
    native: &'a Ordered,
    source_sha256: &'a str,
    binary_sha256: &'a str,
}
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
}
struct Pending {
    file: tempfile::NamedTempFile,
    path: PathBuf,
    hash: String,
    bounds: Vec<Bound>,
    historical: Historical,
    root: PathBuf,
}
impl Pending {
    fn verify(&self, cancel: &AtomicBool) -> Result<()> {
        check(&self.bounds, &self.historical, cancel)?;
        verify_tree(&self.root, &cases())?;
        verify_held(self.file.path(), self.file.as_file(), &self.hash)
    }
    fn commit(self, cancel: &AtomicBool) -> Result<()> {
        self.verify(cancel)?;
        let held = self
            .file
            .persist_noclobber(&self.path)
            .map_err(|e| e.error)?;
        let result = check(&self.bounds, &self.historical, cancel)
            .and_then(|_| verify_tree(&self.root, &cases()))
            .and_then(|_| verify_held(&self.path, &held, &self.hash));
        if result.is_err()
            && held.metadata().is_ok_and(|m| {
                fs::symlink_metadata(&self.path)
                    .is_ok_and(|n| (m.dev(), m.ino()) == (n.dev(), n.ino()))
            })
        {
            let _ = fs::remove_file(&self.path);
        }
        result
    }
}
fn captured_text(stdout: &Path, stderr: &Path) -> Result<String> {
    // subprocess.run(text=True) decodes both streams before check_returncode.
    let text = String::from_utf8(fs::read(stdout)?)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let _stderr = String::from_utf8(fs::read(stderr)?)?;
    Ok(text)
}
fn run_inner(
    options: &Options,
    runner: &mut dyn Runner,
    cancel: &AtomicBool,
) -> Result<(String, Pending)> {
    let mut held = Vec::new();
    let historical = Historical::open(&options.root)?;
    for (name, hash) in SOURCES.into_iter().skip(1) {
        let bound = Bound::open(&options.root.join(name))?;
        if bound.hash != hash {
            return Err(format!("enumeration source changed: {name}").into());
        }
        held.push(bound);
    }
    let binary = resolve_non_strict(&options.binary)?;
    let alias = if options.binary.is_absolute() {
        options.binary.clone()
    } else {
        std::env::current_dir()?.join(&options.binary)
    };
    let artifact = Bound::open(&binary)?;
    let binary_hash = artifact.hash.clone();
    held.push(artifact);
    held.push(Bound::open(&alias)?);
    check(&held, &historical, cancel)?;
    if let Some(parent) = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let root = fs::canonicalize(&options.output)?;
    let sysfs = root.join("sysfs");
    let devices = root.join("dev");
    let corpus = cases();
    if corpus.len() != 40 {
        return Err("enumeration corpus cardinality changed".into());
    }
    held.extend(write_nodes(&sysfs, &corpus)?);
    verify_tree(&root, &corpus)?;
    let source = expected(&sysfs, &devices)?;
    if source.0.len() != 31 {
        return Err("enumeration acceptance cardinality changed".into());
    }
    // The original artifact preserves one filesystem iteration order. Each
    // fresh run must match its candidate in that run's order, but a new host's
    // directory hash may legitimately enumerate the same nodes differently.
    if !same_device_pairs(&source, &original_rows(&devices)?) {
        return Err("enumeration result differs from sealed original observation".into());
    }
    check(&held, &historical, cancel)?;
    verify_tree(&root, &corpus)?;
    let mut command = CommandSpec::new_os(
        vec![
            binary.as_os_str().to_owned(),
            "enumerate".into(),
            sysfs.as_os_str().to_owned(),
            devices.as_os_str().to_owned(),
        ],
        Duration::from_secs(5),
    );
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    let stdout_path = root.join("native.stdout");
    let stderr_path = root.join("native.stderr");
    let code = runner
        .run_to_files(&command, &stdout_path, &stderr_path)?
        .code;
    check(&held, &historical, cancel)?;
    verify_tree(&root, &corpus)?;
    let native_text = captured_text(&stdout_path, &stderr_path)?;
    if code != 0 {
        return Err(format!("enumeration fixture exited {code}").into());
    }
    let native: Ordered = serde_json::from_str(&native_text)?;
    if source != native {
        return Err("enumeration result differs in path, value or order".into());
    }
    let mut absent = CommandSpec::new_os(
        vec![
            binary.as_os_str().to_owned(),
            "enumerate".into(),
            root.join("absent").into_os_string(),
            devices.as_os_str().to_owned(),
        ],
        Duration::from_secs(5),
    );
    absent.stdout_limit = usize::MAX;
    absent.stderr_limit = usize::MAX;
    let absent_stdout = root.join("absent.stdout");
    let absent_stderr = root.join("absent.stderr");
    let code = runner
        .run_to_files(&absent, &absent_stdout, &absent_stderr)?
        .code;
    check(&held, &historical, cancel)?;
    verify_tree(&root, &corpus)?;
    let absent_text = captured_text(&absent_stdout, &absent_stderr)?;
    if code != 0 {
        return Err(format!("enumeration absent fixture exited {code}").into());
    }
    let missing: Ordered = serde_json::from_str(&absent_text)?;
    if !missing.0.is_empty() {
        return Err("enumeration absent directory was nonempty".into());
    }
    let report = Report {
        nodes: corpus.len(),
        accepted: source.0.len(),
        source: &source,
        native: &native,
        source_sha256: SOURCES[1].1,
        binary_sha256: &binary_hash,
    };
    let report_text = serde_json::to_string_pretty(&report)?;
    let mut file = tempfile::NamedTempFile::new_in(&root)?;
    file.write_all(report_text.as_bytes())?;
    let printed = format!(
        "PASS: {} sysfs nodes, {} accepted in source order, missing directory\n",
        corpus.len(),
        source.0.len()
    );
    check(&held, &historical, cancel)?;
    Ok((
        printed,
        Pending {
            file,
            path: root.join("result.json"),
            hash: sha(report_text.as_bytes()),
            bounds: held,
            historical,
            root,
        },
    ))
}
pub fn run(
    options: &Options,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
    cancel: &AtomicBool,
) -> Result<()> {
    let (line, pending) = run_inner(options, runner, cancel)?;
    pending.verify(cancel)?;
    printed.write_all(line.as_bytes())?;
    printed.flush()?;
    pending.commit(cancel)
}
struct SignalRunner(Arc<AtomicBool>);
impl Runner for SignalRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        NativeRunner.run_controlled(command, Instant::now() + command.timeout, &self.0)
    }
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
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-bluetooth-enumerate --binary BINARY --output OUTPUT");
        return Ok(());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, cancel.clone())?);
    }
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: args.values["--binary"].clone().into(),
        output: args.values["--output"].clone().into(),
    };
    let result = run_inner(&options, &mut SignalRunner(cancel.clone()), &cancel);
    drop(signals);
    if cancel.load(Ordering::SeqCst) {
        return Err("enumeration interrupted".into());
    }
    let (line, pending) = result?;
    pending.verify(&cancel)?;
    std::io::stdout().write_all(line.as_bytes())?;
    std::io::stdout().flush()?;
    pending.commit(&cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }
    fn options(output: PathBuf, binary: PathBuf) -> Options {
        Options {
            root: root(),
            binary,
            output,
        }
    }
    fn copied_binary(folder: &Path) -> PathBuf {
        let binary = folder.join("input-fixture-copy");
        fs::copy(
            root().join("rust/crates/bluetooth/examples/input_fixture.rs"),
            &binary,
        )
        .unwrap();
        binary
    }
    fn copied_sources(folder: &Path) -> PathBuf {
        let source = folder.join("source");
        for (name, _) in SOURCES {
            let from = root().join(name);
            if name == HISTORICAL_CHECKER && !from.exists() {
                continue;
            }
            let to = source.join(name);
            fs::create_dir_all(to.parent().unwrap()).unwrap();
            fs::copy(from, to).unwrap();
        }
        source
    }
    fn remove_historical(source: &Path) {
        let path = source.join(HISTORICAL_CHECKER);
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("cannot remove historical checker: {error}"),
        }
    }
    struct Reject;
    impl Runner for Reject {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            panic!("runner must not launch")
        }
    }
    #[test]
    fn foreign_output_and_early_cancel_do_not_launch_or_publish() {
        let temp = tempfile::tempdir().unwrap();
        let binary = copied_binary(temp.path());
        let output = temp.path().join("foreign");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("result.json"), b"foreign").unwrap();
        assert!(run(
            &options(output.clone(), binary.clone()),
            &mut Reject,
            &mut Vec::new(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(output.join("result.json")).unwrap(), b"foreign");
        let cancelled = temp.path().join("cancelled");
        assert!(run(
            &options(cancelled.clone(), binary),
            &mut Reject,
            &mut Vec::new(),
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(!cancelled.join("result.json").exists());
    }
    struct Fixture {
        call: usize,
        mutate: Option<&'static str>,
        output: PathBuf,
    }
    impl Runner for Fixture {
        fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
            self.call += 1;
            assert_eq!(command.timeout, Duration::from_secs(5));
            assert_eq!(command.stdout_limit, usize::MAX);
            assert_eq!(command.stderr_limit, usize::MAX);
            let args = command.effective_argv();
            assert_eq!(args[1].to_str(), Some("enumerate"));
            if self.mutate == Some("runner") {
                return Err("runner failed".into());
            }
            if self.mutate == Some("node") {
                fs::write(self.output.join("sysfs/event0/device/id/bustype"), b"0003")?;
            }
            if self.mutate == Some("extra") {
                fs::create_dir(self.output.join("sysfs/event-extra"))?;
            }
            if self.mutate == Some("absent") && self.call == 2 {
                fs::create_dir(self.output.join("absent"))?;
            }
            let stdout = if self.call == 1 {
                serde_json::to_vec(&expected(
                    &self.output.join("sysfs"),
                    &self.output.join("dev"),
                )?)?
            } else {
                b"{}".to_vec()
            };
            Ok(Captured {
                code: if self.mutate == Some("exit") { 7 } else { 0 },
                stdout,
                stderr: if (self.mutate == Some("invalid-stderr-main") && self.call == 1)
                    || (self.mutate == Some("invalid-stderr-absent") && self.call == 2)
                {
                    vec![0xff]
                } else {
                    vec![]
                },
            })
        }
    }
    #[test]
    fn runner_exit_and_node_mutation_never_publish_receipt() {
        for mutation in [
            "runner",
            "exit",
            "node",
            "extra",
            "absent",
            "invalid-stderr-main",
            "invalid-stderr-absent",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let output = temp.path().join("run");
            let binary = copied_binary(temp.path());
            let mut fixture = Fixture {
                call: 0,
                mutate: Some(mutation),
                output: output.clone(),
            };
            assert!(run(
                &options(output.clone(), binary),
                &mut fixture,
                &mut Vec::new(),
                &AtomicBool::new(false)
            )
            .is_err());
            assert_eq!(
                fixture.call,
                if matches!(mutation, "absent" | "invalid-stderr-absent") {
                    2
                } else {
                    1
                }
            );
            assert!(!output.join("result.json").exists());
        }
    }
    #[test]
    fn real_runner_spawn_failure_never_publishes_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("spawn-failure");
        let binary = copied_binary(temp.path()); // Source file is not executable.
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(run(
            &options(output.clone(), binary),
            &mut NativeRunner,
            &mut Vec::new(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!output.join("result.json").exists());
    }
    struct Cancelling<'a>(&'a AtomicBool);
    impl Runner for Cancelling<'_> {
        fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
            self.0.store(true, Ordering::SeqCst);
            Ok(Captured {
                code: 0,
                stdout: b"{}".to_vec(),
                stderr: vec![],
            })
        }
    }
    #[test]
    fn cancellation_during_fixture_cannot_publish_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("cancelled-during-run");
        let binary = copied_binary(temp.path());
        let cancel = AtomicBool::new(false);
        assert!(run(
            &options(output.clone(), binary),
            &mut Cancelling(&cancel),
            &mut Vec::new(),
            &cancel
        )
        .is_err());
        assert!(!output.join("result.json").exists());
    }
    struct MutatingWriter(PathBuf);
    impl Write for MutatingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            fs::write(&self.0, b"changed candidate artifact")?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn successful_pair_rechecks_binary_after_stdout_and_preserves_receipt_gate() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("run");
        let binary = copied_binary(temp.path());
        let mut fixture = Fixture {
            call: 0,
            mutate: None,
            output: output.clone(),
        };
        assert!(run(
            &options(output.clone(), binary.clone()),
            &mut fixture,
            &mut MutatingWriter(binary),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fixture.call, 2);
        assert!(!output.join("result.json").exists());
    }
    #[test]
    fn complete_pair_publishes_original_report_shape() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("run");
        let binary = copied_binary(temp.path());
        let mut fixture = Fixture {
            call: 0,
            mutate: None,
            output: output.clone(),
        };
        let mut stdout = Vec::new();
        run(
            &options(output.clone(), binary.clone()),
            &mut fixture,
            &mut stdout,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(fixture.call, 2);
        assert_eq!(
            stdout,
            b"PASS: 40 sysfs nodes, 31 accepted in source order, missing directory\n"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
        assert_eq!(report["nodes"], 40);
        assert_eq!(report["accepted"], 31);
        assert_eq!(report["source"], report["native"]);
        assert_eq!(report["source_sha256"], SOURCES[1].1);
        assert_eq!(report["binary_sha256"], sha(&fs::read(binary).unwrap()));
    }
    #[test]
    fn all_python_whitespace_cases_are_present() {
        let corpus = cases();
        assert_eq!(corpus.len(), 40);
        assert_eq!(SPACES.len(), 29);
        for (index, point) in SPACES.iter().enumerate() {
            let case = &corpus[11 + index];
            assert_eq!(case.name, format!("event-space-{index}"));
            let c = char::from_u32(*point).unwrap();
            assert_eq!(trim_python(&format!("{c}0005{c}")), "0005");
        }
    }

    #[test]
    fn historical_checker_absence_preserves_original_pairs_and_current_order() {
        let temp = tempfile::tempdir().unwrap();
        let source = copied_sources(temp.path());
        remove_historical(&source);
        let output = temp.path().join("run");
        let binary = copied_binary(temp.path());
        let mut fixture = Fixture {
            call: 0,
            mutate: None,
            output: output.clone(),
        };
        let mut options = options(output.clone(), binary);
        options.root = source;
        let mut stdout = Vec::new();
        run(&options, &mut fixture, &mut stdout, &AtomicBool::new(false)).unwrap();
        assert_eq!(fixture.call, 2);
        assert_eq!(
            stdout,
            b"PASS: 40 sysfs nodes, 31 accepted in source order, missing directory\n"
        );
        let report: OriginalReport =
            serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
        assert!(same_device_pairs(
            &report.source,
            &original_rows(&output.join("dev")).unwrap()
        ));
        assert_eq!(report.native, report.source);
    }

    #[test]
    fn only_historical_checker_may_be_absent_and_nonregular_entries_fail() {
        use std::os::unix::fs::symlink;
        for state in [
            "missing-parent",
            "missing-other",
            "wrong",
            "symlink",
            "fifo",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let source = copied_sources(temp.path());
            let checker = source.join(HISTORICAL_CHECKER);
            match state {
                "missing-parent" => fs::remove_dir_all(checker.parent().unwrap()).unwrap(),
                "missing-other" => {
                    fs::remove_file(source.join(SOURCES[1].0)).unwrap();
                }
                "wrong" => fs::write(&checker, b"wrong original checker").unwrap(),
                "symlink" => {
                    remove_historical(&source);
                    symlink(source.join(SOURCES[1].0), &checker).unwrap();
                }
                "fifo" => {
                    remove_historical(&source);
                    assert!(std::process::Command::new("mkfifo")
                        .arg(&checker)
                        .status()
                        .unwrap()
                        .success());
                }
                _ => unreachable!(),
            }
            let output = temp.path().join("run");
            let binary = copied_binary(temp.path());
            let mut options = options(output.clone(), binary);
            options.root = source;
            assert!(
                run(
                    &options,
                    &mut Reject,
                    &mut Vec::new(),
                    &AtomicBool::new(false)
                )
                .is_err(),
                "accepted historical state {state}"
            );
            assert!(!output.join("result.json").exists());
        }
    }

    struct ReappearWriter(PathBuf);
    impl Write for ReappearWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            fs::write(&self.0, b"historical checker reappeared")?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn historical_reappearance_during_final_stdout_cannot_publish_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let source = copied_sources(temp.path());
        let checker = source.join(HISTORICAL_CHECKER);
        remove_historical(&source);
        let output = temp.path().join("run");
        let binary = copied_binary(temp.path());
        let mut options = options(output.clone(), binary);
        options.root = source;
        let mut fixture = Fixture {
            call: 0,
            mutate: None,
            output: output.clone(),
        };
        assert!(run(
            &options,
            &mut fixture,
            &mut ReappearWriter(checker),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fixture.call, 2);
        assert!(!output.join("result.json").exists());
    }

    #[test]
    fn absent_historical_checker_does_not_clobber_foreign_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let source = copied_sources(temp.path());
        remove_historical(&source);
        let output = temp.path().join("foreign");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("result.json"), b"foreign receipt\n").unwrap();
        let binary = copied_binary(temp.path());
        let mut options = options(output.clone(), binary);
        options.root = source;
        assert!(run(
            &options,
            &mut Reject,
            &mut Vec::new(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(
            fs::read(output.join("result.json")).unwrap(),
            b"foreign receipt\n"
        );
    }
}
