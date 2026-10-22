//! Native manager QA against independently captured original-only observations.
//! The original Python controllers are not invoked by this module.
use crate::{
    argparse_scan::{self, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const CASES: [&str; 7] = [
    "normal",
    "registration_failure",
    "poll_failure",
    "prepare_only",
    "interrupted",
    "default_edges",
    "reset_defaults",
];
const MANIFEST_SHA256: &str = "19fbf52a7d468ffac12a1d41b509970c4583766c966816801f5f3d07a05b4d2e";
const MAX_SOURCE: u64 = 32 * 1024 * 1024;
const MAX_EXPECTATION: u64 = 2 * 1024 * 1024;
const MAX_ELF: u64 = 512 * 1024 * 1024;
const SOURCE_PINS: [(&str, &str); 8] = [
    (
        "openpilot/system/manager/manager.py",
        "1dd9bcff0ddf4008c100f2e7c1af75e7a3085ce0ca8b3ea3346bd0bb32300cf8",
    ),
    (
        "rust/tools/manager_reference.py",
        "8a301081be0cf27d2dd32f0cf4336a0cd845e6d33545ba4583b648016b233a92",
    ),
    (
        "rust/tools/check_manager.py",
        "136c8ab3047a14686da61ea2a62460a1ff9eda272e74a341020a8d6ad8227b95",
    ),
    (
        "rust/tools/check_manager_logging.py",
        "ad609889ea500401e1ebfb6b67bda4c40dae1aca907ca115b9a4554f01b39d0c",
    ),
    (
        "rust/tools/generate_manager_cars.py",
        "a090e5ba6e30856c4b072ed2d04e682971172fd096592693a0a98756ca9f4caf",
    ),
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "openpilot/common/params.py",
        "4708c127258c0c0de9acd2d670ad9b48d4f3fd050c96deb373ea8cd348afe3a4",
    ),
    (
        "openpilot/common/swaglog.py",
        "d37b623bd9418b72ffd3c4800a1da192c49856cb63bbbc1c09bb24a5ec6f8616",
    ),
];
const RETIREABLE: [&str; 3] = [
    "rust/tools/check_manager.py",
    "rust/tools/check_manager_logging.py",
    "rust/tools/manager_reference.py",
];

pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub require_retired: bool,
}

fn require(ok: bool, detail: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(detail.into().into())
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn digest(file: &File, len: u64, limit: u64) -> Result<String> {
    require(len <= limit, "manager QA input exceeds its read bound")?;
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buf = [0; 64 * 1024];
    let mut count = 0u64;
    loop {
        let size = reader.read(&mut buf)?;
        if size == 0 {
            break;
        }
        count += size as u64;
        require(
            count <= limit,
            "manager QA input grew beyond its read bound",
        )?;
        hasher.update(&buf[..size]);
    }
    require(count == len, "manager QA input length changed")?;
    Ok(format!("{:x}", hasher.finalize()))
}

struct Held {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
    sha256: String,
    limit: u64,
}

type FileIdentity = (u64, u64, u64, u32, (i64, i64), (i64, i64));

fn file_identity(meta: &fs::Metadata) -> FileIdentity {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mode(),
        (meta.mtime(), meta.mtime_nsec()),
        (meta.ctime(), meta.ctime_nsec()),
    )
}

impl Held {
    fn open(path: PathBuf, limit: u64) -> Result<Self> {
        let before = fs::symlink_metadata(&path)?;
        require(
            before.is_file() && !before.file_type().is_symlink(),
            format!("not a regular manager QA input: {}", path.display()),
        )?;
        let file = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let meta = file.metadata()?;
        let identity = file_identity(&meta);
        require(
            file_identity(&before) == identity,
            format!("manager QA input changed while opening: {}", path.display()),
        )?;
        let sha256 = digest(&file, meta.len(), limit)?;
        let held = Self {
            path,
            file,
            identity,
            sha256,
            limit,
        };
        held.verify()?;
        Ok(held)
    }

    fn verify(&self) -> Result<()> {
        let path = fs::symlink_metadata(&self.path)?;
        let file = self.file.metadata()?;
        require(
            path.is_file() && !path.file_type().is_symlink(),
            "manager QA input replaced by nonfile",
        )?;
        for current in [&path, &file] {
            require(
                file_identity(current) == self.identity,
                format!("manager QA input identity changed: {}", self.path.display()),
            )?;
        }
        require(
            digest(&self.file, self.identity.2, self.limit)? == self.sha256,
            format!("manager QA input content changed: {}", self.path.display()),
        )
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        reader.take(self.limit + 1).read_to_end(&mut bytes)?;
        self.verify_bytes(&bytes)?;
        Ok(bytes)
    }

    fn verify_bytes(&self, bytes: &[u8]) -> Result<()> {
        require(
            bytes.len() as u64 == self.identity.2 && hash(bytes) == self.sha256,
            "manager QA returned bytes differ from pinned source",
        )?;
        self.verify()
    }

    fn require_host_elf(&self) -> Result<()> {
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 20];
        reader.read_exact(&mut header)?;
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported manager QA hosted architecture".into()),
        };
        require(
            &header[..4] == b"\x7fELF"
                && header[4] == 2
                && header[5] == 1
                && header[6] == 1
                && u16::from_le_bytes([header[18], header[19]]) == machine,
            format!(
                "manager QA input is not a current-host ELF: {}",
                self.path.display()
            ),
        )
    }
}

struct Original {
    files: Vec<Held>,
    values: BTreeMap<String, Value>,
}

impl Original {
    fn open(root: &Path) -> Result<Self> {
        let base = root.join("rust/crates/dev-tools/tests/fixtures/manager-qa-original");
        let manifest_path = base.join("manifest.json");
        let manifest = Held::open(manifest_path, MAX_EXPECTATION)?;
        require(
            manifest.sha256 == MANIFEST_SHA256,
            "manager original manifest hash changed",
        )?;
        let meta: Value = serde_json::from_slice(&manifest.bytes()?)?;
        require(
            meta["source_commit"] == "fc0f017bd96419c81af878ef211ed15152fc2eda",
            "manager original source commit changed",
        )?;
        let files = meta["files"]
            .as_object()
            .ok_or("manager original files missing")?;
        require(files.len() == 10, "manager original file roster changed")?;
        let mut held = vec![manifest];
        let mut values = BTreeMap::new();
        for (name, expected) in files {
            require(
                name.starts_with("source/") || name.starts_with("logging/"),
                "unexpected manager original path",
            )?;
            require(
                !name.split('/').any(|part| part == ".." || part.is_empty()),
                "unsafe manager original path",
            )?;
            let entry = Held::open(base.join(name), MAX_EXPECTATION)?;
            require(
                expected.as_str() == Some(entry.sha256.as_str()),
                format!("manager original hash changed: {name}"),
            )?;
            values.insert(name.clone(), serde_json::from_slice(&entry.bytes()?)?);
            held.push(entry);
        }
        for case in CASES {
            require(
                values.contains_key(&format!("source/{case}.json")),
                "manager original scenario missing",
            )?;
        }
        for name in [
            "logging/source/result.json",
            "logging/source-records.json",
            "logging/synthetic-environment.json",
        ] {
            require(
                values.contains_key(name),
                "manager original logging file missing",
            )?;
        }
        Ok(Self {
            files: held,
            values,
        })
    }

    fn get(&self, name: &str) -> Result<&Value> {
        self.values
            .get(name)
            .ok_or_else(|| format!("missing manager original observation: {name}").into())
    }

    fn verify(&self) -> Result<()> {
        for file in &self.files {
            file.verify()?;
        }
        Ok(())
    }
}

fn sources(root: &Path, retired: bool) -> Result<Vec<Held>> {
    let mut held = Vec::new();
    for (name, expected) in SOURCE_PINS {
        let path = root.join(name);
        if retired && RETIREABLE.contains(&name) {
            require(
                fs::symlink_metadata(&path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                format!("retired manager QA source reappeared: {name}"),
            )?;
            require(
                path.parent().is_some_and(Path::is_dir),
                "manager QA source parent absent",
            )?;
        } else {
            let pin = Held::open(path, MAX_SOURCE)?;
            require(
                pin.sha256 == expected,
                format!("manager QA original source changed: {name}"),
            )?;
            held.push(pin);
        }
    }
    Ok(held)
}

fn verify_sources(root: &Path, retired: bool, held: &[Held]) -> Result<()> {
    for file in held {
        file.verify()?;
    }
    if retired {
        for name in RETIREABLE {
            require(
                fs::symlink_metadata(root.join(name))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                format!("retired manager QA source reappeared: {name}"),
            )?;
        }
    }
    Ok(())
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "manager trace path is not UTF-8".into())
}

fn run_case(binary: &Path, root: &Path, output: &Path, case: &str) -> Result<Value> {
    fs::create_dir_all(output)?;
    let mut spec = CommandSpec::new(
        vec![path_string(binary)?, path_string(output)?, case.to_owned()],
        Duration::from_secs(60),
    );
    spec.cwd = Some(root.into());
    spec.env.insert("PYTHONPATH".into(), path_string(root)?);
    for name in [
        "DISABLE_WIDE_ROAD",
        "DONGLE_ID",
        "GIT_ORIGIN",
        "GIT_BRANCH",
        "GIT_COMMIT",
        "CLEAN",
    ] {
        spec.env_remove.push(name.into());
    }
    let mut runner = NativeRunner;
    let captured = runner.run_to_files(
        &spec,
        &output.join("stdout.log"),
        &output.join("stderr.log"),
    )?;
    require(
        captured.code == 0,
        format!("manager native {case} exit {}", captured.code),
    )?;
    Ok(serde_json::from_slice(&fs::read(
        output.join("result.json"),
    )?)?)
}

fn selected(record: &Value) -> Result<Value> {
    let ctx = record["ctx"]
        .as_object()
        .ok_or("manager log context missing")?;
    require(record["level"].is_string(), "manager log level missing")?;
    require(
        record["msg"].is_string() || record["msg"].is_object(),
        "manager log message invalid",
    )?;
    Ok(json!({"msg":record["msg"], "daemon":ctx.get("daemon"), "level":record["level"]}))
}

/// Four source log packets include host/time/PID fields; compare the unchanged
/// source checker's selected fields and retain the complete native packets.
fn compare_logs(
    source: &[Value],
    native: &[Value],
    old_prefix: &str,
    new_prefix: &str,
) -> Result<Vec<Value>> {
    require(
        source.len() == 4 && native.len() == 4,
        "manager log packet count changed",
    )?;
    let mut rows = Vec::new();
    for (index, (source, native)) in source.iter().zip(native).enumerate() {
        let mut expected = selected(source)?;
        if index == 2 {
            let env = expected["msg"]["environ"]
                .as_str()
                .ok_or("manager environment record missing")?;
            require(
                env.contains(old_prefix),
                "historical manager log prefix absent",
            )?;
            expected["msg"]["environ"] = env.replace(old_prefix, new_prefix).into();
        }
        let actual = selected(native)?;
        require(
            actual == expected,
            format!("manager log packet {index} source selection changed"),
        )?;
        rows.push(actual);
    }
    Ok(rows)
}

fn logging(binary: &Path, root: &Path, output: &Path, original: &Original) -> Result<Value> {
    let env_provider = Held::open("/usr/bin/env".into(), MAX_ELF)?;
    env_provider.require_host_elf()?;
    let directory = tempfile::Builder::new().prefix("manager-log-").tempdir()?;
    let prefix = path_string(Path::new(
        directory
            .path()
            .file_name()
            .ok_or("manager log prefix missing")?,
    ))?;
    let endpoint = format!("ipc:///tmp/logmessage{prefix}");
    let context = zmq::Context::new();
    let collector = context.socket(zmq::PULL)?;
    collector.bind(&endpoint)?;
    fs::create_dir_all(output)?;
    let mut env = vec![
        ("PATH", "/usr/bin:/bin".to_owned()),
        ("LC_ALL", "C.UTF-8".to_owned()),
        ("LC_CTYPE", "C.UTF-8".to_owned()),
        ("PYTHONPATH", path_string(root)?),
        ("OPENPILOT_PREFIX", prefix.clone()),
        ("MANAGER_LOG_CAPTURE", endpoint),
        ("NOBOARD", String::new()),
        ("BLOCK", "ui,,custom".to_owned()),
        (
            "FIXTURE_LABEL",
            "synthetic \"quotes\" and 'apostrophe'\nline".to_owned(),
        ),
    ];
    if let Ok(path) = std::env::var("LD_LIBRARY_PATH") {
        env.push(("LD_LIBRARY_PATH", path));
    }
    let mut argv = vec!["/usr/bin/env".to_owned(), "-i".to_owned()];
    argv.extend(env.iter().map(|(k, v)| format!("{k}={v}")));
    argv.extend([
        path_string(binary)?,
        path_string(output)?,
        "logging".to_owned(),
    ]);
    let mut spec = CommandSpec::new(argv, Duration::from_secs(60));
    spec.cwd = Some(root.into());
    let mut runner = NativeRunner;
    let captured = runner.run_to_files(
        &spec,
        &output.join("stdout.log"),
        &output.join("stderr.log"),
    )?;
    require(
        captured.code == 0,
        format!("manager native logging exit {}", captured.code),
    )?;
    let mut records = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && records.len() < 4 {
        if collector.poll(zmq::POLLIN, 100)? != 0 {
            let packet = collector.recv_bytes(0)?;
            require(!packet.is_empty(), "manager log framing changed")?;
            let record = serde_json::from_slice::<Value>(&packet[1..])?;
            require(
                record["levelnum"].as_u64() == Some(u64::from(packet[0])),
                "manager log priority prefix changed",
            )?;
            records.push(record);
        }
    }
    require(
        records.len() == 4 && collector.poll(zmq::POLLIN, 100)? == 0,
        "manager log packet roster changed",
    )?;
    fs::write(
        output.join("native-records.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    let original_records = original
        .get("logging/source-records.json")?
        .as_array()
        .ok_or("original manager logs invalid")?;
    let old_env = original.get("logging/synthetic-environment.json")?;
    let old_prefix = old_env["OPENPILOT_PREFIX"]
        .as_str()
        .ok_or("original manager prefix missing")?;
    let selected = compare_logs(original_records, &records, old_prefix, &prefix)?;
    let native_result: Value = serde_json::from_slice(&fs::read(output.join("result.json"))?)?;
    require(
        native_result == *original.get("logging/source/result.json")?,
        "manager logging lifecycle/Params result changed",
    )?;
    env_provider.verify()?;
    let environment = env
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"selected":selected,"full_packets":records,"result":native_result,"environment":environment,
        "env_provider_sha256":env_provider.sha256}),
    )
}

pub fn run(options: &Options) -> Result<Value> {
    require(
        !options.require_retired,
        "manager QA source retirement requires a fresh complete imported-source capture",
    )?;
    let root = options.root.canonicalize()?;
    let binary = options.binary.canonicalize()?;
    require(!options.output.exists(), "manager QA output already exists")?;
    let original = Original::open(&root)?;
    let source = sources(&root, options.require_retired)?;
    let executable = Held::open(binary.clone(), MAX_ELF)?;
    require(
        executable.file.metadata()?.mode() & 0o111 != 0,
        "manager trace is not executable",
    )?;
    executable.require_host_elf()?;
    fs::create_dir(&options.output)?;
    let mut cases = BTreeMap::new();
    for case in CASES {
        let candidate = run_case(&binary, &root, &options.output.join(case), case)?;
        require(
            candidate == *original.get(&format!("source/{case}.json"))?,
            format!("manager source comparison changed: {case}"),
        )?;
        cases.insert(case, json!({"status":candidate["status"],"trace_rows":candidate["trace"].as_array().map(Vec::len),
            "params":candidate["params"].as_object().map(|p|p.len())}));
        original.verify()?;
        verify_sources(&root, options.require_retired, &source)?;
        executable.verify()?;
    }
    let logs = logging(&binary, &root, &options.output.join("logging"), &original)?;
    original.verify()?;
    verify_sources(&root, options.require_retired, &source)?;
    executable.verify()?;
    let result = json!({"cases":cases,"logging":logs,"binary_sha256":executable.sha256,
        "source_pin_count":SOURCE_PINS.len(),"source_roster_complete":false,
        "scope":"candidate source comparison; imported-source closure pending hosted capture"});
    let mut receipt = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(options.output.join("result.json"))?;
    receipt.write_all(&serde_json::to_vec_pretty(&result)?)?;
    receipt.flush()?;
    Ok(result)
}

pub fn cli() -> Result<()> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = argparse_scan::scan_os(
        &raw,
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--source-root", false),
            OptionSpec::flag("--require-retired", None),
        ],
    )?;
    if parsed.help {
        println!("check-manager-qa --binary MANAGER_TRACE --output DIR [--source-root ROOT] [--require-retired]");
        return Ok(());
    }
    let root = parsed
        .values
        .get("--source-root")
        .map_or_else(std::env::current_dir, |value| Ok(PathBuf::from(value)))?;
    run(&Options {
        root,
        binary: parsed.values["--binary"].clone().into(),
        output: parsed.values["--output"].clone().into(),
        require_retired: parsed.values.contains_key("--require-retired"),
    })?;
    Ok(())
}

/// The original checker accepts any scenario string; only these six names
/// select special branches in both manager_reference.py and manager_trace.
fn expectation_for_scenario(case: &str) -> &str {
    if CASES.contains(&case) {
        case
    } else {
        "normal"
    }
}

fn positional_inputs(binary: PathBuf, binding: PathBuf) -> Result<(PathBuf, Held, Held)> {
    let binary = binary.canonicalize()?;
    let executable = Held::open(binary.clone(), MAX_ELF)?;
    executable.require_host_elf()?;
    require(
        executable.file.metadata()?.mode() & 0o111 != 0,
        "manager trace is not executable",
    )?;
    let binding = Held::open(binding.canonicalize()?, MAX_ELF)?;
    binding.require_host_elf()?;
    Ok((binary, executable, binding))
}

fn original_command_paths(
    positionals: &[std::ffi::OsString],
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let root = std::env::current_dir()?.canonicalize()?;
    let output = PathBuf::from(&positionals[2]);
    let output = if output.is_absolute() {
        output
    } else {
        root.join(output)
    };
    Ok((root, PathBuf::from(&positionals[0]), output))
}

/// Native counterpart of check_manager.py's three positional arguments and
/// append-style scenario selection. The positional binding is held and checked
/// but is never imported by this native implementation.
pub fn cli_manager_compat() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed =
        argparse_scan::scan_os_with_positionals(&args, &[OptionSpec::append("--scenario")], 3)?;
    if parsed.help {
        std::io::stdout().lock().write_all(
            b"usage: check-manager [-h] [--scenario SCENARIO] binary binding output\n",
        )?;
        return Ok(());
    }
    let (root, binary, output) = original_command_paths(&parsed.positionals)?;
    let (_, executable, binding) =
        positional_inputs(binary, PathBuf::from(&parsed.positionals[1]))?;
    let source = sources(&root, false)?;
    let original = Original::open(&root)?;
    let scenarios = match parsed.append_values.get("--scenario") {
        Some(values) if !values.is_empty() => values
            .iter()
            .map(|value| {
                value
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| -> Box<dyn std::error::Error> { "scenario is not UTF-8".into() })
            })
            .collect::<Result<Vec<String>>>()?,
        _ => CASES.iter().map(|case| (*case).to_owned()).collect(),
    };
    fs::create_dir_all(&output)?;
    for case in &scenarios {
        let candidate = run_case(
            &executable.path,
            &root,
            &output.join(format!("{case}-native")),
            case,
        )?;
        require(
            candidate
                == *original.get(&format!("source/{}.json", expectation_for_scenario(case)))?,
            format!("manager source comparison changed: {case}"),
        )?;
        original.verify()?;
        verify_sources(&root, false, &source)?;
        executable.verify()?;
        binding.verify()?;
        let mut stdout = std::io::stdout().lock();
        writeln!(
            stdout,
            "PASS {case}: exact trace, Params bytes, environment"
        )?;
        stdout.flush()?;
    }
    let summary = json!({"passed":scenarios,"source":"openpilot/system/manager/manager.py",
        "limitations":"isolated external boundaries; no device/production selection"});
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    fs::write(
        output.join("historical-source.json"),
        serde_json::to_vec_pretty(
            &json!({"source_commit":"fc0f017bd96419c81af878ef211ed15152fc2eda",
            "sealed_manifest_sha256":MANIFEST_SHA256,"fresh_original_execution":false}),
        )?,
    )?;
    original.verify()?;
    verify_sources(&root, false, &source)?;
    executable.verify()?;
    binding.verify()?;
    Ok(())
}

/// Native counterpart of check_manager_logging.py's three positional paths.
pub fn cli_logging_compat() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = argparse_scan::scan_os_with_positionals(&args, &[], 3)?;
    if parsed.help {
        std::io::stdout()
            .lock()
            .write_all(b"usage: check-manager-logging [-h] binary binding output\n")?;
        return Ok(());
    }
    let (root, binary, output) = original_command_paths(&parsed.positionals)?;
    let (_, executable, binding) =
        positional_inputs(binary, PathBuf::from(&parsed.positionals[1]))?;
    let source = sources(&root, false)?;
    let original = Original::open(&root)?;
    fs::create_dir_all(&output)?;
    let result = logging(&executable.path, &root, &output.join("native"), &original)?;
    let summary = json!({"records":result["selected"],"source_order":true,
        "synthetic_environment_only":true});
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    fs::write(
        output.join("historical-source.json"),
        serde_json::to_vec_pretty(
            &json!({"source_commit":"fc0f017bd96419c81af878ef211ed15152fc2eda",
            "sealed_manifest_sha256":MANIFEST_SHA256,"fresh_original_execution":false}),
        )?,
    )?;
    original.verify()?;
    verify_sources(&root, false, &source)?;
    executable.verify()?;
    binding.verify()?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(b"PASS manager lifecycle/default collector: 4 matching records, daemon context, full synthetic environment and exact transition/cleanup order\n")?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn held_file_rejects_rapid_mode_content_and_path_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("provider.json");
        fs::write(&path, b"before").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let held = Held::open(path.clone(), 64).unwrap();
        assert_eq!(held.bytes().unwrap(), b"before");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(held.verify().is_err());

        let held = Held::open(path.clone(), 64).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(held.verify().is_err());

        let held = Held::open(path.clone(), 64).unwrap();
        fs::rename(&path, directory.path().join("old-provider.json")).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(held.verify().is_err());
    }

    #[test]
    fn returned_buffer_digest_is_checked_independently_of_held_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("original.json");
        fs::write(&path, b"sealed").unwrap();
        let held = Held::open(path, 64).unwrap();
        let mut returned = held.bytes().unwrap();
        returned[0] ^= 1;
        assert!(held.verify().is_ok());
        assert!(held.verify_bytes(&returned).is_err());
    }

    #[test]
    fn original_fixture_has_full_seven_result_roster_and_four_raw_logs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let original = Original::open(&root).unwrap();
        for case in CASES {
            let result = original.get(&format!("source/{case}.json")).unwrap();
            assert!(result["status"].is_string());
            assert!(result["trace"].is_array());
            assert!(result["params"].as_object().is_some_and(|p| p.len() >= 200));
            assert!(result["environment"].is_object());
        }
        let records = original
            .get("logging/source-records.json")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(records.len(), 4);
        for record in records {
            assert!(record["created"].is_number());
            assert!(record["process"].is_number());
            assert!(record["thread"].is_number());
            assert!(selected(record).is_ok());
        }
        original.verify().unwrap();
    }

    #[test]
    fn logging_comparison_rejects_mutated_level_message_or_missing_packet() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let original = Original::open(&root).unwrap();
        let source = original
            .get("logging/source-records.json")
            .unwrap()
            .as_array()
            .unwrap();
        let prefix = original.get("logging/synthetic-environment.json").unwrap()
            ["OPENPILOT_PREFIX"]
            .as_str()
            .unwrap();
        assert_eq!(
            compare_logs(source, source, prefix, prefix).unwrap().len(),
            4
        );
        let mut changed = source.clone();
        changed[1]["level"] = "ERROR".into();
        assert!(compare_logs(source, &changed, prefix, prefix).is_err());
        changed = source.clone();
        changed[0]["msg"] = "different".into();
        assert!(compare_logs(source, &changed, prefix, prefix).is_err());
        assert!(compare_logs(source, &source[..3], prefix, prefix).is_err());
    }

    #[test]
    fn original_observation_corruption_is_rejected_before_candidate_execution() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let source = root.join("rust/crates/dev-tools/tests/fixtures/manager-qa-original");
        let private = tempfile::tempdir().unwrap();
        let target = private
            .path()
            .join("rust/crates/dev-tools/tests/fixtures/manager-qa-original");
        fs::create_dir_all(target.join("source")).unwrap();
        fs::create_dir_all(target.join("logging/source")).unwrap();
        fs::copy(source.join("manifest.json"), target.join("manifest.json")).unwrap();
        let metadata: Value =
            serde_json::from_slice(&fs::read(source.join("manifest.json")).unwrap()).unwrap();
        for name in metadata["files"].as_object().unwrap().keys() {
            fs::copy(source.join(name), target.join(name)).unwrap();
        }
        let case = target.join("source/normal.json");
        let mut bytes = fs::read(&case).unwrap();
        bytes[0] = b'!';
        fs::write(case, bytes).unwrap();
        assert!(Original::open(private.path()).is_err());
    }
}
