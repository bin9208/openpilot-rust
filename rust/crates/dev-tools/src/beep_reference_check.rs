//! Twenty source-pinned beep policy scenarios against an independent original capture.
//! The original Python controller is never executed by this module.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const SOURCES: [(&str, &str); 7] = [
    (
        "rust/tools/check_beepd_reference.py",
        "051315d756ee2afd43e94051067283c597c2fbcf83d48d10721c62c591797177",
    ),
    (
        "rust/tools/beep_source.py",
        "ef4a4c7350eb36748eb3a1bb2abca0e3a166dc8103772d795398da95a6baa7bb",
    ),
    (
        "rust/tools/beep_fixtures.py",
        "d182570bf3a6f13522c334d6471e12734676ddd9debbfbc1065e5633b002cfe2",
    ),
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "openpilot/selfdrive/controls/beep.py",
        "6b0427638dd6cb2279d87e6456eaefaedf2992b09ca317c1d1b7c370cd2060f5",
    ),
    (
        "openpilot/common/utils.py",
        "b1cc68b63cff1386ba397c0cc527dbb38fb50b154043ffcb066ed7acce65ec83",
    ),
    (
        "openpilot/common/realtime.py",
        "6337d64508299a53c00f0795397e5043ec7b69c864d6ca8c02604244502aba5e",
    ),
];
const ORIGINAL_BINDING_PROVENANCE: &[u8] =
    include_bytes!("../tests/fixtures/beep-reference/original-binding-provenance-3835653.json");
const ORIGINAL_BINDING_PROVENANCE_SHA256: &str =
    "62a4531e93e1659885012f0b7a53df4821a6ea903ebf1ce1e5bf93187e8b3ad9";

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    /// The source-only oracle receipt remains mandatory even without rebuilding
    /// the original Python Params binding for this native-only comparison.
    pub binding: Option<PathBuf>,
    pub original: PathBuf,
    /// Hash of the independently captured `files.sha256`, pinned by the cloud gate.
    pub original_manifest_sha256: String,
    pub output: PathBuf,
    pub checker_absent: bool,
}

struct Case {
    name: &'static str,
    settings: Value,
    volume: Option<Vec<u8>>,
}

fn identity(meta: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}

struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    hash: String,
}

impl Bound {
    fn open(path: &Path, limit: u64) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err(format!(
                "beep input is not a bounded regular file: {}",
                path.display()
            )
            .into());
        }
        let mut bound = Self {
            path: path.to_owned(),
            file,
            metadata,
            hash: String::new(),
        };
        bound.hash = bound.digest(limit)?;
        bound.verify(limit)?;
        Ok(bound)
    }

    fn digest(&mut self, limit: u64) -> Result<String> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut bytes = [0; 65536];
        let mut total = 0_u64;
        loop {
            let count = self.file.read(&mut bytes)?;
            if count == 0 {
                break;
            }
            total += u64::try_from(count)?;
            if total > limit {
                return Err("beep input grew past its byte bound".into());
            }
            digest.update(&bytes[..count]);
        }
        Ok(format!("{:x}", digest.finalize()))
    }

    fn verify(&mut self, limit: u64) -> Result<()> {
        let path_meta = fs::symlink_metadata(&self.path)?;
        if !path_meta.is_file()
            || identity(&self.metadata) != identity(&self.file.metadata()?)
            || identity(&self.metadata) != identity(&path_meta)
            || self.digest(limit)? != self.hash
        {
            return Err(format!("beep input changed: {}", self.path.display()).into());
        }
        Ok(())
    }

    fn native_elf(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut header = [0_u8; 20];
        if let Err(error) = self.file.read_exact(&mut header) {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                return Err("beep candidate is not a native ELF for this host".into());
            }
            return Err(error.into());
        }
        let machine = u16::from_le_bytes([header[18], header[19]]);
        let expected_machine = if cfg!(target_arch = "x86_64") {
            62
        } else if cfg!(target_arch = "aarch64") {
            183
        } else {
            0
        };
        if &header[..4] != b"\x7fELF"
            || header[4] != 2
            || header[5] != 1
            || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
            || machine != expected_machine
        {
            return Err("beep candidate is not a native ELF for this host".into());
        }
        Ok(())
    }
}

fn cases() -> Vec<Case> {
    let mut alerts = Vec::new();
    for alert in (0..37).chain([65535, 37, 0]) {
        for _ in 0..2 {
            alerts.push(json!({"kind":"alert","value":alert}));
        }
    }
    let case = |name, settings, volume: Option<&[u8]>| Case {
        name,
        settings,
        volume: volume.map(<[u8]>::to_vec),
    };
    vec![
        case(
            "every-alert",
            json!({"actions": alerts.clone()}),
            Some(b"10"),
        ),
        case("every-alert-muted", json!({"actions": alerts}), Some(b"5")),
        case(
            "absent-volume",
            json!({"actions":[{"kind":"alert","value":3}]}),
            None,
        ),
        case(
            "empty-volume",
            json!({"actions":[{"kind":"alert","value":3}]}),
            Some(b""),
        ),
        case(
            "prefix-volume",
            json!({"actions":[{"kind":"alert","value":1}]}),
            Some(b" \x0b+6suffix"),
        ),
        case(
            "mid-pulse-volume",
            json!({"actions":[{"kind":"alert","value":3}],"mutations":{"1":[53],"3":[49,48]}}),
            Some(b"10"),
        ),
        case(
            "failed-status",
            json!({"actions":[{"kind":"alert","value":3}],"status":7}),
            Some(b"10"),
        ),
        case(
            "export-spawn-failure",
            json!({"actions":[{"kind":"alert","value":1}],"command_fail":[0]}),
            Some(b"10"),
        ),
        case(
            "direction-spawn-failure",
            json!({"command_fail":[1]}),
            Some(b"10"),
        ),
        case(
            "startup-on-failure",
            json!({"command_fail":[2]}),
            Some(b"10"),
        ),
        case(
            "startup-off-failure",
            json!({"command_fail":[3]}),
            Some(b"10"),
        ),
        case(
            "worker-on-failure",
            json!({"actions":[{"kind":"alert","value":1},{"kind":"alert","value":2}],"command_fail":[4]}),
            Some(b"10"),
        ),
        case(
            "worker-off-failure",
            json!({"actions":[{"kind":"alert","value":1},{"kind":"alert","value":2}],"command_fail":[5]}),
            Some(b"10"),
        ),
        case(
            "startup-sleep-failure",
            json!({"sleep_fail":[0]}),
            Some(b"10"),
        ),
        case(
            "worker-sleep-failure",
            json!({"actions":[{"kind":"alert","value":1},{"kind":"alert","value":2}],"sleep_fail":[1]}),
            Some(b"10"),
        ),
        case("invalid-startup-volume", json!({}), Some(b"bad")),
        case("overflow-startup-volume", json!({}), Some(b"2147483648")),
        case(
            "invalid-worker-volume",
            json!({"actions":[{"kind":"volume","bytes":[98,97,100]},{"kind":"alert","value":1}]}),
            Some(b"10"),
        ),
        case(
            "invalid-off-pulse-volume",
            json!({"actions":[{"kind":"alert","value":3}],"mutations":{"1":[98,97,100]}}),
            Some(b"10"),
        ),
        case(
            "ratekeeper-catchup",
            json!({"mode":"rate","times":[10,10,10,10.001,10.1,10.14,10.15,10.16,10.16,10.16],"count":4}),
            Some(b"10"),
        ),
    ]
}

fn capture_files(original: &Path, pinned: &str) -> Result<()> {
    let mut manifest = Bound::open(&original.join("files.sha256"), 1024 * 1024)?;
    if manifest.hash != pinned {
        return Err("beep original-only capture manifest differs from pinned hash".into());
    }
    manifest.file.seek(SeekFrom::Start(0))?;
    let mut manifest_bytes = Vec::new();
    (&mut manifest.file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut manifest_bytes)?;
    if manifest_bytes.len() > 1024 * 1024 {
        return Err("beep capture manifest grew past its bound".into());
    }
    let mut names = BTreeSet::new();
    for line in std::str::from_utf8(&manifest_bytes)?.lines() {
        let (digest, file) = line
            .split_once("  ")
            .ok_or("invalid beep capture manifest row")?;
        let name = file.strip_prefix("./").ok_or("invalid beep capture path")?;
        if name.contains('/') || name == "files.sha256" || !names.insert(name.to_owned()) {
            return Err("duplicate or nested beep capture path".into());
        }
        let member = Bound::open(&original.join(name), 16 * 1024 * 1024)?;
        if member.hash != digest {
            return Err("beep capture member changed or exceeds bound".into());
        }
    }
    let expected = cases();
    for case in expected {
        for suffix in ["json", "stdout", "stderr", "trace.jsonl"] {
            if !names.contains(&format!("{}.{}", case.name, suffix)) {
                return Err("beep capture is missing an original case member".into());
            }
        }
    }
    if names.len() != 82 || !names.contains("SOURCE_COMMIT") || !names.contains("inputs.sha256") {
        return Err("beep capture file roster differs".into());
    }
    manifest.verify(1024 * 1024)?;
    Ok(())
}

fn captured_binding_hash(original: &Path) -> Result<String> {
    let mut receipt = Bound::open(&original.join("inputs.sha256"), 16 * 1024)?;
    receipt.file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    (&mut receipt.file)
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 {
        return Err("beep original input receipt exceeds bound".into());
    }
    let rows = std::str::from_utf8(&bytes)?.lines().collect::<Vec<_>>();
    if rows.len() != SOURCES.len() + 1 {
        return Err("beep original input receipt has wrong row count".into());
    }
    for ((name, digest), row) in SOURCES.into_iter().zip(&rows) {
        if *row != format!("{digest}  {name}") {
            return Err(format!("beep original input source differs: {name}").into());
        }
    }
    let (digest, path) = rows[SOURCES.len()]
        .split_once("  ")
        .ok_or("beep original binding receipt is malformed")?;
    if digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !path.starts_with('/')
        || !Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("params_pyx") && name.ends_with(".so"))
    {
        return Err("beep original binding receipt is malformed".into());
    }
    receipt.verify(16 * 1024)?;
    Ok(digest.to_owned())
}

fn original_binding_provenance(original: &Path, historical_hash: &str) -> Result<Value> {
    let hash = format!("{:x}", Sha256::digest(ORIGINAL_BINDING_PROVENANCE));
    if hash != ORIGINAL_BINDING_PROVENANCE_SHA256 {
        return Err("beep sealed original binding provenance changed".into());
    }
    let provenance: Value = serde_json::from_slice(ORIGINAL_BINDING_PROVENANCE)?;
    if provenance["result"] != "pass"
        || provenance["module_sha256"] != historical_hash
        || provenance["source_commit"] != fs::read_to_string(original.join("SOURCE_COMMIT"))?.trim()
        || provenance["sources"]
            .as_object()
            .is_none_or(|map| map.len() != 18)
    {
        return Err("beep original binding provenance disagrees with original capture".into());
    }
    Ok(provenance)
}

fn bound_binding_sources(
    root: &Path,
    original: &Value,
    current: Option<&Value>,
) -> Result<Vec<Bound>> {
    for key in ["json11_architecture", "json11_sha256", "json11_version"] {
        if original[key].as_str().is_none() {
            return Err(format!("beep sealed original binding {key} is missing").into());
        }
    }
    if let Some(current) = current {
        if current["result"] != "pass" || current["sources"] != original["sources"] {
            return Err("beep rebuilt binding source map differs from sealed original".into());
        }
        for key in ["json11_architecture", "json11_sha256", "json11_version"] {
            if current[key] != original[key] || current[key].as_str().is_none() {
                return Err(
                    format!("beep rebuilt binding {key} differs from sealed original").into(),
                );
            }
        }
    }
    let mut bound = Vec::new();
    for (name, digest) in original["sources"].as_object().unwrap() {
        let source = Bound::open(&root.join(name), 16 * 1024 * 1024)?;
        if Some(source.hash.as_str()) != digest.as_str() {
            return Err(format!("beep rebuilt binding source changed: {name}").into());
        }
        bound.push(source);
    }
    Ok(bound)
}

fn absent_checker(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root.join(SOURCES[0].0)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("retired beep Python checker reappeared".into()),
        Err(error) => Err(error.into()),
    }
}

fn source_pins(root: &Path, checker_absent: bool) -> Result<Vec<Bound>> {
    let mut sources = Vec::new();
    for (name, digest) in SOURCES {
        if checker_absent && name == SOURCES[0].0 {
            absent_checker(root)?;
            continue;
        }
        let source = Bound::open(&root.join(name), 16 * 1024 * 1024)?;
        if source.hash != digest {
            return Err(format!("beep source changed: {name}").into());
        }
        sources.push(source);
    }
    Ok(sources)
}

fn text(bytes: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(bytes)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

fn trace(path: &Path) -> Result<Vec<Value>> {
    let fd = match rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let file = fs::File::from(fd);
    if !file.metadata()?.is_file() {
        return Err("beep trace is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("beep trace exceeds bound".into());
    }
    let mut rows = Vec::new();
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if line.last() == Some(&b'\n') {
            rows.push(serde_json::from_slice(line)?);
        }
    }
    Ok(rows)
}

fn original_row(original: &Path, case: &Case) -> Result<Value> {
    let row: Value =
        serde_json::from_slice(&fs::read(original.join(format!("{}.json", case.name)))?)?;
    if row["name"] != case.name || row["original"]["implementation"] != "python" {
        return Err("beep capture case name or implementation differs".into());
    }
    let original_result = &row["original"];
    if original_result["stdout"]
        != fs::read_to_string(original.join(format!("{}.stdout", case.name)))?
        || original_result["stderr"]
            != fs::read_to_string(original.join(format!("{}.stderr", case.name)))?
        || original_result["trace"]
            != json!(trace(&original.join(format!("{}.trace.jsonl", case.name)))?)
        || original_result["log_packets"] != json!([])
    {
        return Err("beep capture raw original files differ from original result".into());
    }
    Ok(original_result.clone())
}

fn compare(name: &str, original: &Value, native: &Value) -> Result<()> {
    if original["trace"] != native["trace"] || original["stdout"] != native["stdout"] {
        return Err(format!("beep policy trace/stdout differs: {name}").into());
    }
    if original["log_packets"] != json!([]) || native["log_packets"] != json!([]) {
        return Err(format!("beep policy emitted a log packet: {name}").into());
    }
    if original["exit_code"] == -6 {
        if native["exit_code"] != 1
            || !native["stderr"]
                .as_str()
                .is_some_and(|v| v.contains("Integer") || v.contains("stoi"))
        {
            return Err(format!("beep integer abort exception differs: {name}").into());
        }
    } else if original["exit_code"] != native["exit_code"]
        || original["stderr"].as_str().is_some_and(|s| !s.is_empty())
            != native["stderr"].as_str().is_some_and(|s| !s.is_empty())
    {
        return Err(format!("beep exit/stderr presence differs: {name}").into());
    }
    Ok(())
}

fn run_case(case: &Case, binary: &Path, output: &Path, runner: &mut dyn Runner) -> Result<Value> {
    let temporary = tempfile::Builder::new().prefix("beep-policy-").tempdir()?;
    let root = temporary.path();
    let prefix = format!("beep_{}", case.name);
    let params = root.join(&prefix);
    fs::create_dir(&params)?;
    if let Some(volume) = &case.volume {
        fs::write(params.join("SoundVolumeAdjust"), volume)?;
    }
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL)?;
    collector.bind(&format!("ipc://{}", root.join("swaglog").display()))?;
    let trace_path = output.join(format!("{}-rust.trace.jsonl", case.name));
    let mut config = json!({"root":root,"binding":"unused-native","trace":trace_path,"mode":"policy","actions":[]});
    let settings = case
        .settings
        .as_object()
        .ok_or("beep case settings are not an object")?;
    for (key, value) in settings {
        config[key] = value.clone();
    }
    let mut spec =
        CommandSpec::new_os(vec![binary.as_os_str().to_owned()], Duration::from_secs(10));
    spec.input = Some(format!("{}\n", serde_json::to_string(&config)?).into_bytes());
    // The fixed corpus emits much less than this; fail loudly on unexpected
    // growth instead of silently truncating a subprocess stream.
    spec.stdout_limit = 16 * 1024 * 1024;
    spec.stderr_limit = 16 * 1024 * 1024;
    spec.env = BTreeMap::from([
        (
            "PARAMS_ROOT".to_owned(),
            root.to_str()
                .ok_or("non-UTF8 beep fixture root")?
                .to_owned(),
        ),
        ("OPENPILOT_PREFIX".to_owned(), prefix),
        (
            "PATH".to_owned(),
            root.join("bin")
                .to_str()
                .ok_or("non-UTF8 beep fixture PATH")?
                .to_owned(),
        ),
        ("PYTHONUNBUFFERED".to_owned(), "1".to_owned()),
    ]);
    spec.env_remove = ["ZMQ", "CEREAL_FAKE", "SIMULATION"]
        .into_iter()
        .map(Into::into)
        .collect();
    fs::create_dir(root.join("bin"))?;
    let child = runner.run(&spec)?;
    let stdout = text(child.stdout)?;
    let stderr = text(child.stderr)?;
    fs::write(output.join(format!("{}-rust.stdout", case.name)), &stdout)?;
    fs::write(output.join(format!("{}-rust.stderr", case.name)), &stderr)?;
    let mut packets = Vec::new();
    while collector.poll(zmq::POLLIN, 20)? != 0 {
        if packets.len() == 1024 {
            return Err("beep collector packet count exceeds bound".into());
        }
        let payload = collector.recv_bytes(0)?;
        packets.push(
            payload
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        );
    }
    let trace = trace(&trace_path)?;
    Ok(
        json!({"implementation":"rust","exit_code":child.code,"stdout":stdout,"stderr":stderr,"trace":trace,"log_packets":packets}),
    )
}

pub fn check(options: &Options, runner: &mut dyn Runner, printed: &mut dyn Write) -> Result<()> {
    let mut sources = source_pins(&options.root, options.checker_absent)?;
    capture_files(&options.original, &options.original_manifest_sha256)?;
    let original_binding_hash = captured_binding_hash(&options.original)?;
    let original_binding = original_binding_provenance(&options.original, &original_binding_hash)?;
    let (mut binding, mut receipt, current_provenance) = if let Some(path) = &options.binding {
        let binding = Bound::open(path, 64 * 1024 * 1024)?;
        let mut receipt = Bound::open(&path.with_file_name("provenance.json"), 1024 * 1024)?;
        receipt.file.seek(SeekFrom::Start(0))?;
        let mut receipt_bytes = Vec::new();
        (&mut receipt.file)
            .take(1024 * 1024 + 1)
            .read_to_end(&mut receipt_bytes)?;
        if receipt_bytes.len() > 1024 * 1024 {
            return Err("beep binding provenance grew past its bound".into());
        }
        let provenance: Value = serde_json::from_slice(&receipt_bytes)?;
        receipt.verify(1024 * 1024)?;
        if provenance["module_sha256"] != binding.hash {
            return Err("beep original binding differs from provenance".into());
        }
        (Some(binding), Some(receipt), Some(provenance))
    } else {
        (None, None, None)
    };
    let mut binding_sources = bound_binding_sources(
        &options.root,
        &original_binding,
        current_provenance.as_ref(),
    )?;
    // The original CLI resolves valid relative executable paths before spawn.
    let selected_binary = fs::canonicalize(&options.binary)?;
    let mut binary = Bound::open(&selected_binary, 128 * 1024 * 1024)?;
    binary.native_elf()?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    for case in cases() {
        let original = original_row(&options.original, &case)?;
        binary.verify(128 * 1024 * 1024)?;
        let native = run_case(&case, &binary.path, &options.output, runner)?;
        compare(case.name, &original, &native)?;
        fs::write(
            options
                .output
                .join(format!("{}.comparison.json", case.name)),
            serde_json::to_vec_pretty(&json!([original, native]))?,
        )?;
    }
    for source in &mut sources {
        source.verify(16 * 1024 * 1024)?;
    }
    for source in &mut binding_sources {
        source.verify(16 * 1024 * 1024)?;
    }
    if options.checker_absent {
        absent_checker(&options.root)?;
    }
    capture_files(&options.original, &options.original_manifest_sha256)?;
    if captured_binding_hash(&options.original)? != original_binding_hash {
        return Err("beep original binding receipt changed".into());
    }
    binary.verify(128 * 1024 * 1024)?;
    if let Some(binding) = &mut binding {
        binding.verify(64 * 1024 * 1024)?;
    }
    if let Some(receipt) = &mut receipt {
        receipt.verify(1024 * 1024)?;
    }
    let summary = json!({"passed":true,"scenarios":20,"alerts":(0..37).chain([65535,37]).collect::<Vec<_>>()});
    fs::write(
        options.output.join("summary.json"),
        format!("{}\n", serde_json::to_string_pretty(&summary)?),
    )?;
    writeln!(printed, "PASS: 20 unchanged-source/native startup, alert, volume, worker-error and cadence scenarios")?;
    Ok(())
}

pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--binding", false),
            OptionSpec::flag("--checker-absent", None),
            OptionSpec::value("--original", true),
            OptionSpec::value("--original-manifest-sha256", true),
            OptionSpec::value("--output", true),
        ],
    )?;
    if args.help {
        println!("usage: check-beep-reference --binary EXAMPLE [--binding MODULE] [--checker-absent] --original CAPTURE --original-manifest-sha256 SHA256 --output DIRECTORY");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let checker_absent = args.values.contains_key("--checker-absent")
        || fs::symlink_metadata(root.join(SOURCES[0].0))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    check(
        &Options {
            root,
            binary: PathBuf::from(&args.values["--binary"]),
            binding: args.values.get("--binding").map(PathBuf::from),
            original: PathBuf::from(&args.values["--original"]),
            original_manifest_sha256: args.values["--original-manifest-sha256"]
                .to_str()
                .ok_or("non-UTF8 beep original manifest hash")?
                .to_owned(),
            output: PathBuf::from(&args.values["--output"]),
            checker_absent,
        },
        &mut NativeRunner,
        &mut std::io::stdout(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_source_schedule_is_fixed() {
        let cases = cases();
        assert_eq!(cases.len(), 20);
        assert_eq!(cases[0].name, "every-alert");
        let alerts = cases[0].settings["actions"].as_array().unwrap();
        assert_eq!(alerts.len(), 80);
        assert_eq!(alerts[0], json!({"kind":"alert","value":0}));
        assert_eq!(alerts[72], json!({"kind":"alert","value":36}));
        assert_eq!(alerts[74], json!({"kind":"alert","value":65535}));
        assert_eq!(alerts[76], json!({"kind":"alert","value":37}));
        assert_eq!(cases[19].settings["count"], 4);
        assert_eq!(cases[19].settings["times"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn comparison_rejects_trace_stdout_packets_and_exit_drift() {
        let source = json!({"trace":[{"kind":"sleep","seconds":0.1}],"stdout":"ok\n","stderr":"","exit_code":0,"log_packets":[]});
        assert!(compare("control", &source, &source).is_ok());
        for mutation in [
            json!({"trace":[],"stdout":"ok\n","stderr":"","exit_code":0,"log_packets":[]}),
            json!({"trace":[{"kind":"sleep","seconds":0.1}],"stdout":"changed\n","stderr":"","exit_code":0,"log_packets":[]}),
            json!({"trace":[{"kind":"sleep","seconds":0.1}],"stdout":"ok\n","stderr":"","exit_code":0,"log_packets":["00"]}),
            json!({"trace":[{"kind":"sleep","seconds":0.1}],"stdout":"ok\n","stderr":"","exit_code":1,"log_packets":[]}),
            json!({"trace":[{"kind":"sleep","seconds":0.1}],"stdout":"ok\n","stderr":"new","exit_code":0,"log_packets":[]}),
        ] {
            assert!(compare("mutation", &source, &mutation).is_err());
        }
    }

    #[test]
    fn original_integer_abort_has_only_the_documented_native_exception() {
        let original =
            json!({"trace":[],"stdout":"","stderr":"aborted","exit_code":-6,"log_packets":[]});
        let accepted = json!({"trace":[],"stdout":"","stderr":"Integer conversion failed","exit_code":1,"log_packets":[]});
        assert!(compare("integer", &original, &accepted).is_ok());
        let wrong =
            json!({"trace":[],"stdout":"","stderr":"other failure","exit_code":1,"log_packets":[]});
        assert!(compare("integer", &original, &wrong).is_err());
    }

    #[test]
    fn original_lines_contract_keeps_missing_and_only_complete_rows() {
        let dir = tempfile::tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");
        assert!(trace(&trace_path).unwrap().is_empty());
        fs::write(&trace_path, b"{\"value\":1}\n{\"incomplete\":").unwrap();
        assert_eq!(trace(&trace_path).unwrap(), vec![json!({"value":1})]);
        let link = dir.path().join("alias.jsonl");
        std::os::unix::fs::symlink(&trace_path, &link).unwrap();
        assert!(trace(&link).is_err());
    }
}
