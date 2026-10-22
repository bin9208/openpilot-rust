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
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
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
const RETIREMENT_MANIFEST_SHA256: &str =
    "d598542767a0ea195ce44deceea271b4fe10b2bc874a99dc080eeb445e4d96bb";
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
type DirectoryIdentity = (u64, u64, u32);

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

fn directory_identity(meta: &fs::Metadata) -> DirectoryIdentity {
    (meta.dev(), meta.ino(), meta.mode())
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
        let closure = Held::open(base.join("retirement-manifest.json"), MAX_EXPECTATION)?;
        require(
            closure.sha256 == RETIREMENT_MANIFEST_SHA256,
            "manager original imported-source capture manifest changed",
        )?;
        let closure_meta: Value = serde_json::from_slice(&closure.bytes()?)?;
        require(
            closure_meta["checked_out_source_commit"] == "83fb19a8cfda05bd2039253ceea8e0786676274d"
                && closure_meta["run"] == 39728775823u64
                && closure_meta["source_rosters"]["repo_git_objects"]["count"] == 3266
                && closure_meta["source_rosters"]["opendbc_git_objects"]["count"] == 257
                && closure_meta["source_rosters"]["installed_provider_rows"]["count"] == 3495,
            "manager original imported-source capture provenance changed",
        )?;
        let closure_files = closure_meta["files"]
            .as_object()
            .ok_or("manager original imported-source files missing")?;
        require(
            closure_files.len() == 67,
            "manager original imported-source file roster changed",
        )?;
        held.push(closure);
        for (name, record) in closure_files {
            require(
                name.starts_with("retirement/")
                    && !name.split('/').any(|part| part == ".." || part.is_empty())
                    && !name.ends_with(".py"),
                "unsafe manager imported-source capture path",
            )?;
            let file = Held::open(base.join(name), MAX_EXPECTATION)?;
            require(
                record["sha256"].as_str() == Some(file.sha256.as_str())
                    && record["size"].as_u64() == Some(file.identity.2),
                format!("manager imported-source capture changed: {name}"),
            )?;
            held.push(file);
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

struct SourceSet {
    held: Vec<Held>,
    parent: File,
    parent_path: PathBuf,
    parent_identity: DirectoryIdentity,
    retired: bool,
}

impl SourceSet {
    fn open(root: &Path, require_retired: bool) -> Result<Self> {
        let parent_path = root.join("rust/tools");
        let parent = File::from(rustix::fs::open(
            &parent_path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let parent_identity = directory_identity(&parent.metadata()?);
        require(
            directory_identity(&fs::symlink_metadata(&parent_path)?) == parent_identity,
            "manager QA historical parent changed while opening",
        )?;
        let mut present = 0;
        for name in RETIREABLE {
            match fs::symlink_metadata(root.join(name)) {
                Ok(_) => present += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        require(
            present == 0 || present == RETIREABLE.len(),
            "manager QA historical source roster is mixed",
        )?;
        let retired = present == 0;
        require(
            !require_retired || retired,
            "manager QA historical sources still present",
        )?;
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
        let result = Self {
            held,
            parent,
            parent_path,
            parent_identity,
            retired,
        };
        result.verify(root)?;
        Ok(result)
    }

    fn verify(&self, root: &Path) -> Result<()> {
        for current in [
            self.parent.metadata()?,
            fs::symlink_metadata(&self.parent_path)?,
        ] {
            require(
                current.is_dir() && directory_identity(&current) == self.parent_identity,
                "manager QA historical parent identity changed",
            )?;
        }
        for file in &self.held {
            file.verify()?;
        }
        if self.retired {
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

// Python's dict repr quotes these environment values. Replace the complete
// named field, so a path repeated in another variable is never normalized.
fn python_repr(value: &str) -> String {
    crate::plot_samples_source::text_repr(&value.chars().map(u32::from).collect::<Vec<_>>())
}

fn replace_environment_field(environ: &str, key: &str, old: &str, new: &str) -> Result<String> {
    let before = format!("'{}': {}", key, python_repr(old));
    require(
        environ.matches(&before).count() == 1,
        format!("manager environment {key} field missing or duplicated"),
    )?;
    let after = format!("'{}': {}", key, python_repr(new));
    Ok(environ.replacen(&before, &after, 1))
}

fn project_optional_environment_field(
    environ: &str,
    key: &str,
    old: Option<&str>,
    new: Option<&str>,
) -> Result<String> {
    match (old, new) {
        (Some(old), Some(new)) => replace_environment_field(environ, key, old, new),
        (Some(old), None) => {
            let field = format!(", '{key}': {}", python_repr(old));
            require(
                environ.matches(&field).count() == 1,
                format!("manager environment {key} field missing or duplicated"),
            )?;
            Ok(environ.replacen(&field, "", 1))
        }
        (None, None) => Ok(environ.to_owned()),
        (None, Some(_)) => Err(format!("historical manager environment {key} missing").into()),
    }
}

/// Four source log packets include host/time/PID fields; compare the unchanged
/// source checker's selected fields and retain the complete native packets.
struct LogProjection<'a> {
    old_prefix: &'a str,
    new_prefix: &'a str,
    old_pythonpath: &'a str,
    new_pythonpath: &'a str,
    old_ld_library_path: Option<&'a str>,
    new_ld_library_path: Option<&'a str>,
}

fn compare_logs(
    source: &[Value],
    native: &[Value],
    projection: LogProjection<'_>,
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
            let env = replace_environment_field(
                env,
                "OPENPILOT_PREFIX",
                projection.old_prefix,
                projection.new_prefix,
            )?;
            let env = replace_environment_field(
                &env,
                "MANAGER_LOG_CAPTURE",
                &format!("ipc:///tmp/logmessage{}", projection.old_prefix),
                &format!("ipc:///tmp/logmessage{}", projection.new_prefix),
            )?;
            let env = replace_environment_field(
                &env,
                "PYTHONPATH",
                projection.old_pythonpath,
                projection.new_pythonpath,
            )?;
            expected["msg"]["environ"] = project_optional_environment_field(
                &env,
                "LD_LIBRARY_PATH",
                projection.old_ld_library_path,
                projection.new_ld_library_path,
            )?
            .into();
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
    let old_pythonpath = old_env["PYTHONPATH"]
        .as_str()
        .ok_or("original manager Python path missing")?;
    let current_pythonpath = path_string(root)?;
    let selected = compare_logs(
        original_records,
        &records,
        LogProjection {
            old_prefix,
            new_prefix: &prefix,
            old_pythonpath,
            new_pythonpath: &current_pythonpath,
            old_ld_library_path: old_env["LD_LIBRARY_PATH"].as_str(),
            new_ld_library_path: env
                .iter()
                .find(|(key, _)| *key == "LD_LIBRARY_PATH")
                .map(|(_, value)| value.as_str()),
        },
    )?;
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
    run_with_emit(options, |_| Ok(()))
}

struct OwnedReceipt {
    path: PathBuf,
    file: File,
    owner: (u64, u64),
    committed: bool,
}

impl OwnedReceipt {
    fn create(path: PathBuf) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let metadata = file.metadata()?;
        Ok(Self {
            path,
            file,
            owner: (metadata.dev(), metadata.ino()),
            committed: false,
        })
    }

    fn owned(&self) -> bool {
        fs::symlink_metadata(&self.path).is_ok_and(|meta| {
            meta.is_file()
                && !meta.file_type().is_symlink()
                && (meta.dev(), meta.ino()) == self.owner
        })
    }

    fn verify(&self, bytes: &[u8]) -> Result<()> {
        require(self.owned(), "manager QA result receipt ownership changed")?;
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut actual = Vec::new();
        reader.take(MAX_EXPECTATION + 1).read_to_end(&mut actual)?;
        require(actual == bytes, "manager QA result receipt content changed")
    }
}

impl Drop for OwnedReceipt {
    fn drop(&mut self) {
        if !self.committed && self.owned() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// Positional checkers permit an existing output directory and overwrite their
// two summaries. Stage each replacement in that directory, retaining a hard
// link to any old inode until stdout and the last input checks have completed.
struct CompatReceipt {
    path: PathBuf,
    file: File,
    owner: (u64, u64),
    old: Option<tempfile::TempDir>,
    committed: bool,
}

impl CompatReceipt {
    fn publish(path: PathBuf, bytes: &[u8]) -> Result<Self> {
        require(
            bytes.len() as u64 <= MAX_EXPECTATION,
            "manager compatibility receipt too large",
        )?;
        let parent = path.parent().ok_or("manager receipt parent missing")?;
        let old = match fs::symlink_metadata(&path) {
            Ok(meta) => {
                require(
                    meta.is_file() && !meta.file_type().is_symlink(),
                    "manager compatibility receipt is not a regular file",
                )?;
                let backup = tempfile::Builder::new()
                    .prefix(".manager-receipt-backup-")
                    .tempdir_in(parent)?;
                fs::hard_link(&path, backup.path().join("original"))?;
                let current = fs::symlink_metadata(&path)?;
                require(
                    (current.dev(), current.ino(), current.len(), current.mode())
                        == (meta.dev(), meta.ino(), meta.len(), meta.mode())
                        && file_identity(&fs::symlink_metadata(backup.path().join("original"))?)
                            == file_identity(&current),
                    "manager compatibility receipt changed before replacement",
                )?;
                Some(backup)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut staged = tempfile::Builder::new()
            .prefix(".manager-receipt-")
            .tempfile_in(parent)?;
        staged.write_all(bytes)?;
        staged.flush()?;
        let owner_meta = staged.as_file().metadata()?;
        let owner = (owner_meta.dev(), owner_meta.ino());
        let file = staged.as_file().try_clone()?;
        if let Some(ref backup) = old {
            require(
                file_identity(&fs::symlink_metadata(&path)?)
                    == file_identity(&fs::symlink_metadata(backup.path().join("original"))?),
                "manager compatibility receipt changed during replacement",
            )?;
        } else {
            require(
                fs::symlink_metadata(&path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "manager compatibility receipt appeared during replacement",
            )?;
        }
        staged.persist(&path)?;
        Ok(Self {
            path,
            file,
            owner,
            old,
            committed: false,
        })
    }

    fn owned(&self) -> bool {
        fs::symlink_metadata(&self.path).is_ok_and(|meta| {
            meta.is_file()
                && !meta.file_type().is_symlink()
                && (meta.dev(), meta.ino()) == self.owner
        })
    }

    fn verify(&self, bytes: &[u8]) -> Result<()> {
        require(
            self.owned(),
            "manager compatibility receipt ownership changed",
        )?;
        let mut reader = self.file.try_clone()?;
        reader.seek(SeekFrom::Start(0))?;
        let mut actual = Vec::new();
        reader.take(MAX_EXPECTATION + 1).read_to_end(&mut actual)?;
        require(actual == bytes, "manager compatibility receipt changed")
    }
}

impl Drop for CompatReceipt {
    fn drop(&mut self) {
        if !self.committed && self.owned() {
            if let Some(ref backup) = self.old {
                let _ = fs::rename(backup.path().join("original"), &self.path);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn finish_compat<F, V>(
    output: &Path,
    summary: &[u8],
    historical: &[u8],
    verify: V,
    emit: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
    V: Fn() -> Result<()>,
{
    verify()?;
    let mut summary_receipt = CompatReceipt::publish(output.join("summary.json"), summary)?;
    let mut historical_receipt =
        CompatReceipt::publish(output.join("historical-source.json"), historical)?;
    summary_receipt.verify(summary)?;
    historical_receipt.verify(historical)?;
    emit()?;
    verify()?;
    summary_receipt.verify(summary)?;
    historical_receipt.verify(historical)?;
    summary_receipt.committed = true;
    historical_receipt.committed = true;
    Ok(())
}

fn run_with_emit<F>(options: &Options, emit: F) -> Result<Value>
where
    F: FnOnce(&Value) -> Result<()>,
{
    let root = options.root.canonicalize()?;
    let binary = options.binary.canonicalize()?;
    require(!options.output.exists(), "manager QA output already exists")?;
    let original = Original::open(&root)?;
    let source = SourceSet::open(&root, options.require_retired)?;
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
        source.verify(&root)?;
        executable.verify()?;
    }
    let logs = logging(&binary, &root, &options.output.join("logging"), &original)?;
    original.verify()?;
    source.verify(&root)?;
    executable.verify()?;
    let result = json!({"cases":cases,"logging":logs,"binary_sha256":executable.sha256,
        "source_pin_count":SOURCE_PINS.len(),"historical_sources_absent":source.retired,
        "source_roster_complete":false,
        "imported_source_capture_manifest_sha256":RETIREMENT_MANIFEST_SHA256,
        "scope":"native manager observations; imported-source closure bound to independent hosted capture"});
    let bytes = serde_json::to_vec_pretty(&result)?;
    let mut receipt = OwnedReceipt::create(options.output.join("result.json"))?;
    receipt.file.write_all(&bytes)?;
    receipt.file.flush()?;
    receipt.verify(&bytes)?;
    emit(&result)?;
    original.verify()?;
    source.verify(&root)?;
    executable.verify()?;
    receipt.verify(&bytes)?;
    receipt.committed = true;
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
        println!(
            "check-manager-qa --binary MANAGER_TRACE --output DIR [--source-root ROOT] [--require-retired]"
        );
        return Ok(());
    }
    let root = parsed
        .values
        .get("--source-root")
        .map_or_else(std::env::current_dir, |value| Ok(PathBuf::from(value)))?;
    run_with_emit(
        &Options {
            root,
            binary: parsed.values["--binary"].clone().into(),
            output: parsed.values["--output"].clone().into(),
            require_retired: parsed.values.contains_key("--require-retired"),
        },
        |_| {
            std::io::stdout().lock().flush()?;
            Ok(())
        },
    )?;
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

fn manager_argparse_error(
    args: &[std::ffi::OsString],
    program: &str,
    scenarios: bool,
) -> Option<String> {
    let positionals = ["binary", "binding", "output"];
    let mut positional_count = 0usize;
    let mut unknown = Vec::new();
    let mut literal = false;
    let mut index = 0usize;
    while index < args.len() {
        let bytes = args[index].as_os_str().as_bytes();
        if !literal && bytes == b"--" {
            literal = true;
            index += 1;
            continue;
        }
        let is_negative_number =
            std::str::from_utf8(bytes).is_ok_and(argparse_scan::negative_number);
        if literal || !bytes.starts_with(b"-") || bytes == b"-" || is_negative_number {
            if positional_count >= positionals.len() {
                unknown.push(String::from_utf8_lossy(bytes).into_owned());
            }
            positional_count += 1;
            index += 1;
            continue;
        }
        if scenarios {
            let option = bytes.split(|byte| *byte == b'=').next().unwrap_or(bytes);
            if option.starts_with(b"--") && b"--scenario".starts_with(option) {
                if bytes.contains(&b'=') {
                    index += 1;
                    continue;
                }
                let has_value = args.get(index + 1).is_some_and(|value| {
                    let value = value.as_os_str().as_bytes();
                    !value.starts_with(b"-")
                        || value == b"-"
                        || std::str::from_utf8(value).is_ok_and(argparse_scan::negative_number)
                });
                if !has_value {
                    let usage = manager_usage(program, scenarios);
                    return Some(format!(
                        "usage: {usage}\n{program}: error: argument --scenario: expected one argument\n"
                    ));
                }
                index += 2;
                continue;
            }
        }
        if bytes == b"--help" {
            index += 1;
            continue;
        }
        unknown.push(String::from_utf8_lossy(bytes).into_owned());
        index += 1;
    }

    let missing = positionals
        .iter()
        .skip(positional_count.min(positionals.len()))
        .copied()
        .collect::<Vec<_>>();
    let detail = if !missing.is_empty() {
        format!(
            "the following arguments are required: {}",
            missing.join(", ")
        )
    } else if !unknown.is_empty() {
        format!("unrecognized arguments: {}", unknown.join(" "))
    } else {
        return None;
    };
    Some(format!(
        "usage: {}\n{program}: error: {detail}\n",
        manager_usage(program, scenarios)
    ))
}

fn manager_usage(program: &str, scenarios: bool) -> String {
    if scenarios {
        format!("{program} [-h] [--scenario SCENARIO] binary binding output")
    } else {
        format!("{program} [-h] binary binding output")
    }
}

fn scan_manager_args(
    args: &[std::ffi::OsString],
    program: &str,
    scenarios: bool,
) -> std::result::Result<argparse_scan::ParsedOs, argparse_scan::ArgumentError> {
    let specs = if scenarios {
        vec![OptionSpec::append("--scenario")]
    } else {
        Vec::new()
    };
    argparse_scan::scan_os_with_positionals(args, &specs, 3).map_err(|error| {
        argparse_scan::ArgumentError(
            manager_argparse_error(args, program, scenarios).unwrap_or_else(|| error.to_string()),
        )
    })
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
    let cwd = std::env::current_dir()?.canonicalize()?;
    let root = std::env::var_os("OPENPILOT_RUST_ROOT").map_or_else(
        || cwd.canonicalize(),
        |value| PathBuf::from(value).canonicalize(),
    )?;
    let output = PathBuf::from(&positionals[2]);
    let output = if output.is_absolute() {
        output
    } else {
        cwd.join(output)
    };
    Ok((root, PathBuf::from(&positionals[0]), output))
}

/// Native counterpart of check_manager.py's three positional arguments and
/// append-style scenario selection. The positional binding is held and checked
/// but is never imported by this native implementation.
pub fn cli_manager_compat() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_manager_args(&args, "check_manager.py", true)?;
    if parsed.help {
        std::io::stdout().lock().write_all(include_bytes!(
            "../tests/fixtures/manager-qa-original/retirement/cli-check_manager.py-help.stdout"
        ))?;
        return Ok(());
    }
    let (root, binary, output) = original_command_paths(&parsed.positionals)?;
    let (_, executable, binding) =
        positional_inputs(binary, PathBuf::from(&parsed.positionals[1]))?;
    let source = SourceSet::open(&root, false)?;
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
    let mut completed = Vec::new();
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
        source.verify(&root)?;
        executable.verify()?;
        binding.verify()?;
        writeln!(
            &mut completed,
            "PASS {case}: exact trace, Params bytes, environment"
        )?;
    }
    let summary = json!({"passed":scenarios,"source":"openpilot/system/manager/manager.py",
        "limitations":"isolated external boundaries; no device/production selection"});
    let summary_bytes = serde_json::to_vec_pretty(&summary)?;
    let historical_bytes = serde_json::to_vec_pretty(
        &json!({"source_commit":"fc0f017bd96419c81af878ef211ed15152fc2eda",
            "sealed_manifest_sha256":MANIFEST_SHA256,
            "imported_source_capture_manifest_sha256":RETIREMENT_MANIFEST_SHA256,
            "historical_sources_absent":source.retired,"fresh_original_execution":false}),
    )?;
    finish_compat(
        &output,
        &summary_bytes,
        &historical_bytes,
        || {
            original.verify()?;
            source.verify(&root)?;
            executable.verify()?;
            binding.verify()
        },
        || {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&completed)?;
            stdout.flush()?;
            Ok(())
        },
    )
}

/// Native counterpart of check_manager_logging.py's three positional paths.
pub fn cli_logging_compat() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_manager_args(&args, "check_manager_logging.py", false)?;
    if parsed.help {
        std::io::stdout().lock().write_all(include_bytes!(
            "../tests/fixtures/manager-qa-original/retirement/cli-check_manager_logging.py-help.stdout"
        ))?;
        return Ok(());
    }
    let (root, binary, output) = original_command_paths(&parsed.positionals)?;
    let (_, executable, binding) =
        positional_inputs(binary, PathBuf::from(&parsed.positionals[1]))?;
    let source = SourceSet::open(&root, false)?;
    let original = Original::open(&root)?;
    fs::create_dir_all(&output)?;
    let result = logging(&executable.path, &root, &output.join("native"), &original)?;
    let summary = json!({"records":result["selected"],"source_order":true,
        "synthetic_environment_only":true});
    let summary_bytes = serde_json::to_vec_pretty(&summary)?;
    let historical_bytes = serde_json::to_vec_pretty(
        &json!({"source_commit":"fc0f017bd96419c81af878ef211ed15152fc2eda",
            "sealed_manifest_sha256":MANIFEST_SHA256,
            "imported_source_capture_manifest_sha256":RETIREMENT_MANIFEST_SHA256,
            "historical_sources_absent":source.retired,"fresh_original_execution":false}),
    )?;
    finish_compat(
        &output,
        &summary_bytes,
        &historical_bytes,
        || {
            original.verify()?;
            source.verify(&root)?;
            executable.verify()?;
            binding.verify()
        },
        || {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(b"PASS manager lifecycle/default collector: 4 matching records, daemon context, full synthetic environment and exact transition/cleanup order\n")?;
            stdout.flush()?;
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn private_source_root() -> tempfile::TempDir {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let private = tempfile::tempdir().unwrap();
        let historical_count = RETIREABLE
            .iter()
            .filter(|name| repository.join(name).exists())
            .count();
        assert!(historical_count == 0 || historical_count == RETIREABLE.len());
        for (name, _) in SOURCE_PINS {
            if historical_count == 0 && RETIREABLE.contains(&name) {
                continue;
            }
            let target = private.path().join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(repository.join(name), target).unwrap();
        }
        private
    }

    #[test]
    fn cli_diagnostics_cover_extra_positionals_and_negative_positional_values() {
        let extra = ["bin", "binding", "output", "overflow"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        assert_eq!(
            manager_argparse_error(&extra, "check_manager_logging.py", false).as_deref(),
            Some(
                "usage: check_manager_logging.py [-h] binary binding output\ncheck_manager_logging.py: error: unrecognized arguments: overflow\n"
            )
        );

        let negative = ["-0.5"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        assert_eq!(
            manager_argparse_error(&negative, "check_manager_logging.py", false).as_deref(),
            Some(
                "usage: check_manager_logging.py [-h] binary binding output\ncheck_manager_logging.py: error: the following arguments are required: binding, output\n"
            )
        );
    }

    #[test]
    fn historical_source_state_rejects_mixed_rosters_and_reappearance() {
        let private = private_source_root();
        let initial = SourceSet::open(private.path(), false).unwrap();
        if !initial.retired {
            fs::remove_file(private.path().join(RETIREABLE[0])).unwrap();
            assert!(initial.verify(private.path()).is_err());
            assert!(SourceSet::open(private.path(), false).is_err());
            for name in &RETIREABLE[1..] {
                fs::remove_file(private.path().join(name)).unwrap();
            }
        }
        let retired = SourceSet::open(private.path(), true).unwrap();
        assert!(retired.retired);
        retired.verify(private.path()).unwrap();
        fs::write(
            private.path().join(RETIREABLE[0]),
            b"unexpected reappearance",
        )
        .unwrap();
        assert!(retired.verify(private.path()).is_err());
        assert!(SourceSet::open(private.path(), true).is_err());
        assert!(SourceSet::open(private.path(), false).is_err());
    }

    #[test]
    fn historical_parent_and_other_source_remain_pinned_when_retired() {
        let private = private_source_root();
        for name in RETIREABLE {
            let path = private.path().join(name);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        let retired = SourceSet::open(private.path(), true).unwrap();
        fs::write(
            private.path().join("openpilot/common/params.py"),
            b"same parent, changed source",
        )
        .unwrap();
        assert!(retired.verify(private.path()).is_err());

        let private = private_source_root();
        for name in RETIREABLE {
            let path = private.path().join(name);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        let retired = SourceSet::open(private.path(), true).unwrap();
        fs::create_dir(private.path().join("rust/tools/valid-output")).unwrap();
        retired.verify(private.path()).unwrap();
        fs::rename(
            private.path().join("rust/tools"),
            private.path().join("rust/old-tools"),
        )
        .unwrap();
        fs::create_dir(private.path().join("rust/tools")).unwrap();
        assert!(retired.verify(private.path()).is_err());
    }

    #[test]
    fn owned_receipt_rolls_back_only_its_own_inode() {
        let private = tempfile::tempdir().unwrap();
        let path = private.path().join("result.json");
        {
            let mut receipt = OwnedReceipt::create(path.clone()).unwrap();
            receipt.file.write_all(b"PASS").unwrap();
            receipt.file.flush().unwrap();
            receipt.verify(b"PASS").unwrap();
        }
        assert!(!path.exists());
        {
            let mut receipt = OwnedReceipt::create(path.clone()).unwrap();
            receipt.file.write_all(b"PASS").unwrap();
            receipt.file.flush().unwrap();
            fs::rename(&path, private.path().join("displaced.json")).unwrap();
            fs::write(&path, b"foreign").unwrap();
            assert!(receipt.verify(b"PASS").is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
    }

    #[test]
    fn compatibility_receipts_roll_back_late_input_and_writer_failures() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        let source_path = output.join("held-source");
        let elf_path = output.join("held-elf");
        fs::write(&source_path, b"source").unwrap();
        fs::write(&elf_path, b"elf").unwrap();
        for fail in ["writer", "source", "elf", "receipt", "historical-receipt"] {
            let source = Held::open(source_path.clone(), 64).unwrap();
            let elf = Held::open(elf_path.clone(), 64).unwrap();
            fs::write(output.join("summary.json"), b"old-summary").unwrap();
            fs::write(output.join("historical-source.json"), b"old-history").unwrap();
            let result = finish_compat(
                output,
                b"PASS summary",
                b"PASS history",
                || {
                    source.verify()?;
                    elf.verify()
                },
                || {
                    match fail {
                        "writer" => return Err("broken stdout writer".into()),
                        "source" => fs::write(&source_path, b"drifted")?,
                        "elf" => fs::write(&elf_path, b"bad")?,
                        "receipt" => {
                            fs::rename(
                                output.join("summary.json"),
                                output.join("displaced-summary.json"),
                            )?;
                            fs::write(output.join("summary.json"), b"foreign")?;
                        }
                        "historical-receipt" => {
                            fs::rename(
                                output.join("historical-source.json"),
                                output.join("displaced-history.json"),
                            )?;
                            fs::write(output.join("historical-source.json"), b"foreign")?;
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                },
            );
            assert!(result.is_err(), "late {fail} change was accepted");
            if fail == "receipt" {
                assert_eq!(fs::read(output.join("summary.json")).unwrap(), b"foreign");
                fs::remove_file(output.join("displaced-summary.json")).unwrap();
            } else {
                assert_eq!(
                    fs::read(output.join("summary.json")).unwrap(),
                    b"old-summary"
                );
            }
            if fail == "historical-receipt" {
                assert_eq!(
                    fs::read(output.join("historical-source.json")).unwrap(),
                    b"foreign"
                );
                fs::remove_file(output.join("displaced-history.json")).unwrap();
            } else {
                assert_eq!(
                    fs::read(output.join("historical-source.json")).unwrap(),
                    b"old-history"
                );
            }
            fs::write(&source_path, b"source").unwrap();
            fs::write(&elf_path, b"elf").unwrap();
        }
    }

    #[test]
    fn compatibility_receipts_replace_existing_outputs_only_after_final_checks() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path();
        fs::write(output.join("summary.json"), b"old-summary").unwrap();
        fs::write(output.join("historical-source.json"), b"old-history").unwrap();
        finish_compat(output, b"new-summary", b"new-history", || Ok(()), || Ok(())).unwrap();
        assert_eq!(
            fs::read(output.join("summary.json")).unwrap(),
            b"new-summary"
        );
        assert_eq!(
            fs::read(output.join("historical-source.json")).unwrap(),
            b"new-history"
        );
        assert_eq!(fs::read_dir(output).unwrap().count(), 2);
    }

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
        let pythonpath = original.get("logging/synthetic-environment.json").unwrap()["PYTHONPATH"]
            .as_str()
            .unwrap();
        let ld =
            original.get("logging/synthetic-environment.json").unwrap()["LD_LIBRARY_PATH"].as_str();
        let projection = || LogProjection {
            old_prefix: prefix,
            new_prefix: prefix,
            old_pythonpath: pythonpath,
            new_pythonpath: pythonpath,
            old_ld_library_path: ld,
            new_ld_library_path: ld,
        };
        assert_eq!(compare_logs(source, source, projection()).unwrap().len(), 4);
        let mut changed = source.clone();
        changed[1]["level"] = "ERROR".into();
        assert!(compare_logs(source, &changed, projection()).is_err());
        changed = source.clone();
        changed[0]["msg"] = "different".into();
        assert!(compare_logs(source, &changed, projection()).is_err());
        assert!(compare_logs(source, &source[..3], projection()).is_err());
    }

    #[test]
    fn environment_projection_changes_only_named_pythonpath_field() {
        let original = "environ({'PYTHONPATH': \"/old'path\", 'LD_LIBRARY_PATH': \"/old'path\", 'FIXTURE_LABEL': '/old\\'path', 'OPENPILOT_PREFIX': 'old', 'MANAGER_LOG_CAPTURE': 'ipc:///tmp/logmessageold'})";
        let projected =
            replace_environment_field(original, "PYTHONPATH", "/old'path", "/new'path").unwrap();
        assert!(projected.contains("'PYTHONPATH': \"/new'path\""));
        assert!(projected.contains("'LD_LIBRARY_PATH': \"/old'path\""));
        assert!(projected.contains("'FIXTURE_LABEL': '/old\\'path'"));
        assert!(!projected.contains("'PYTHONPATH': \"/old'path\""));
        assert!(replace_environment_field(&projected, "PYTHONPATH", "/old'path", "/new").is_err());
        let with_ld = project_optional_environment_field(
            &projected,
            "LD_LIBRARY_PATH",
            Some("/old'path"),
            Some("/new path/it's \\\"quoted\\\""),
        )
        .unwrap();
        assert!(with_ld.contains(&format!(
            "'LD_LIBRARY_PATH': {}",
            python_repr("/new path/it's \\\"quoted\\\"")
        )));
        assert!(with_ld.contains("'FIXTURE_LABEL': '/old\\'path'"));
        let without_ld = project_optional_environment_field(
            original,
            "LD_LIBRARY_PATH",
            Some("/old'path"),
            None,
        )
        .unwrap();
        assert!(!without_ld.contains("'LD_LIBRARY_PATH':"));
        assert!(without_ld.contains("'FIXTURE_LABEL': '/old\\'path'"));
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
