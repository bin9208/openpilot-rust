//! Same-ELF native daemon proof against original-only captured observations.
#![cfg(target_os = "linux")]
use chrono::{NaiveDateTime, TimeZone, Utc};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::{
        ffi::OsStringExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const CAPTURE_SOURCE: &str = "19e47ad98286edb1b572b146a85304e1e94e4edf";
const CAPTURE_START_NS: i128 = 1792562491512826293;
const CAPTURE_END_NS: i128 = 1792562492873864873;
const MANIFEST_SHA: &str = "214258ef372f065cf3f924e6d921cf4fd3258e801220504536c8d54e945094b3";
const CHECKER_SHA: &str = "82be3830f8e468c32abe583c197cdf2777cd5fb748a795a8c70247a7dcd00059";
const HELPER_SHA: &str = "76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60";
const ORIGINAL_STDOUT: &str = "PASS production updated: exclusive lock, SIGUSR1 check, SIGHUP fetch/finalize, Params/logs, symlink/mode and SIGTERM lifecycle\n";
const PROOF_PATH: &str = "/usr/bin:/bin";
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/updated-daemon-original-19e47ad9"
);

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned()
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now_ns() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i128
}

struct Pin {
    requested: PathBuf,
    resolved: PathBuf,
    held: File,
    identity: (u64, u64),
    digest: String,
}
impl Pin {
    fn new(requested: PathBuf) -> Self {
        let resolved = fs::canonicalize(&requested).unwrap();
        let mut held = File::from(
            rustix::fs::open(
                &resolved,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        let metadata = held.metadata().unwrap();
        assert!(
            metadata.is_file(),
            "nonregular input: {}",
            requested.display()
        );
        let identity = (metadata.dev(), metadata.ino());
        let digest = hash_held(&mut held);
        let pin = Self {
            requested,
            resolved,
            held,
            identity,
            digest,
        };
        pin.verify();
        pin
    }
    fn verify(&mut self) {
        assert_eq!(fs::canonicalize(&self.requested).unwrap(), self.resolved);
        let named = fs::metadata(&self.resolved).unwrap();
        let held = self.held.metadata().unwrap();
        assert!(named.is_file() && held.is_file());
        assert_eq!((named.dev(), named.ino()), self.identity);
        assert_eq!((held.dev(), held.ino()), self.identity);
        assert_eq!(hash_held(&mut self.held), self.digest);
    }
}
fn hash_held(file: &mut File) -> String {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut hasher = Sha256::new();
    let mut bytes = [0_u8; 65536];
    loop {
        let count = file.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    format!("{:x}", hasher.finalize())
}

fn sealed(name: &str, manifest: &BTreeMap<String, String>) -> Vec<u8> {
    let bytes = fs::read(Path::new(FIXTURE).join(name)).unwrap();
    assert_eq!(sha(&bytes), manifest[name]);
    bytes
}
fn manifest() -> BTreeMap<String, String> {
    let text = fs::read(Path::new(FIXTURE).join("SHA256SUMS")).unwrap();
    assert_eq!(sha(&text), MANIFEST_SHA);
    let result = String::from_utf8(text)
        .unwrap()
        .lines()
        .map(|line| {
            let (digest, path) = line.split_once("  ").unwrap();
            assert_eq!(digest.len(), 64);
            (path.to_owned(), digest.to_owned())
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(result.len(), 8);
    result
}
fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}
fn root_from_commands(bytes: &[u8]) -> String {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    text.lines()
        .flat_map(|line| json(line.as_bytes()).as_array().unwrap().clone())
        .filter_map(|arg| arg.as_str().map(str::to_owned))
        .find_map(|arg| {
            let start = arg.find("/tmp/updated-daemon-")?;
            let end = start + arg[start..].find("/runtime/")? + "/runtime".len();
            Some(arg[start..end].to_owned())
        })
        .expect("missing owned runtime root")
}
fn fixture_parent(root: &str) -> &str {
    let parent = root.strip_suffix("/runtime").unwrap();
    let suffix = parent.strip_prefix("/tmp/updated-daemon-").unwrap();
    assert!(!suffix.is_empty() && !suffix.contains('/'));
    parent
}
fn source_present(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            assert!(metadata.is_file(), "historical source is not regular");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("historical source admission failed: {error}"),
    }
}
fn commands(bytes: &[u8], root: &str) -> Vec<Value> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            Value::Array(
                json(line.as_bytes())
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| Value::String(arg.as_str().unwrap().replace(root, "<ROOT>")))
                    .collect(),
            )
        })
        .collect()
}
fn params(bytes: &[u8], root: &str, start: i128, end: i128) -> Value {
    assert!(start <= end);
    let mut data = json(bytes).as_object().unwrap().clone();
    assert_eq!(data.len(), 16, "complete original Params set");
    for value in data.values_mut() {
        *value = Value::String(value.as_str().unwrap().replace(root, "<ROOT>"));
    }
    for key in ["UpdaterLastFetchTime", "LastUpdateTime", "InstallDate"] {
        let text = data[key].as_str().unwrap();
        let date = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").unwrap();
        let timestamp = Utc.from_utc_datetime(&date).timestamp_nanos_opt().unwrap() as i128;
        assert!(
            (start..=end).contains(&timestamp),
            "{key} outside run: {text}"
        );
        data.insert(key.into(), Value::String("<ACTUAL_DATE>".into()));
    }
    Value::Object(data)
}
fn records(
    bytes: &[u8],
    parent: &str,
    source: &str,
    start_ns: i128,
    end_ns: i128,
) -> BTreeMap<&'static str, Vec<Value>> {
    assert!(start_ns <= end_ns);
    let data = json(bytes);
    let rows = data.as_array().unwrap();
    assert_eq!(rows.len(), 22);
    let signal_thread = rows
        .iter()
        .find(|row| row["msg"] == "caught SIGUSR1, checking for updates")
        .unwrap()["thread"]
        .as_u64()
        .unwrap();
    let mut threads = BTreeSet::new();
    let mut sequences = BTreeMap::<&'static str, Vec<Value>>::new();
    let mut fetch_paths = 0;
    let mut timed_events = 0;
    for row in rows {
        let mut record = row.as_object().unwrap().clone();
        assert!(record["created"].is_f64(), "record created is not a float");
        let created = record["created"].as_f64().unwrap();
        assert!(created.is_finite(), "record created is non-finite");
        let start_seconds = start_ns as f64 / 1_000_000_000.0;
        let end_seconds = end_ns as f64 / 1_000_000_000.0;
        assert!(
            created >= start_seconds - 0.001 && created <= end_seconds + 0.001,
            "record created outside its capture window: {created}"
        );
        let thread = record["thread"].as_u64().unwrap();
        threads.insert(thread);
        let role = if thread == signal_thread {
            "signal"
        } else {
            "main"
        };
        let context = record.get_mut("ctx").unwrap().as_object_mut().unwrap();
        assert_eq!(context["source_commit"], source);
        assert_eq!(context["source_tree"], "clean");
        context.insert(
            "source_commit".into(),
            Value::String("<SOURCE_COMMIT>".into()),
        );
        let message = record.get_mut("msg").unwrap();
        if let Some(text) = message.as_str() {
            if text.starts_with("git fetch success: From ") {
                assert!(text.contains(parent));
                *message = Value::String(text.replace(parent, "<FIXTURE_PARENT>"));
                fetch_paths += 1;
            } else {
                assert!(!text.contains(parent));
            }
        } else {
            let object = message.as_object_mut().unwrap();
            assert_eq!(object.len(), 2);
            assert_eq!(object["event"], "Done LFS cleanup");
            let duration = object["duration"].as_f64().unwrap();
            assert!(duration.is_finite() && (0.0..=15.0).contains(&duration));
            object.insert("duration".into(), Value::String("<DURATION>".into()));
            timed_events += 1;
        }
        for dynamic in ["host", "process", "thread", "created"] {
            assert!(record.remove(dynamic).is_some());
        }
        sequences
            .entry(role)
            .or_default()
            .push(Value::Object(record));
    }
    assert_eq!(threads.len(), 2);
    assert_eq!(fetch_paths, 1);
    assert_eq!(timed_events, 1);
    assert_eq!(sequences["main"].len(), 20);
    assert_eq!(sequences["signal"].len(), 2);
    sequences
}

#[test]
fn record_creation_requires_numeric_in_window_time() {
    let expected = manifest();
    let commands = sealed("original/overlay-commands.jsonl", &expected);
    let root = root_from_commands(&commands);
    let mut rows = json(&sealed("original/records.json", &expected));
    let bytes = serde_json::to_vec(&rows).unwrap();
    let parent = fixture_parent(&root);
    records(
        &bytes,
        parent,
        CAPTURE_SOURCE,
        CAPTURE_START_NS,
        CAPTURE_END_NS,
    );
    rows[0]["created"] = Value::Null;
    assert!(std::panic::catch_unwind(|| {
        records(
            &serde_json::to_vec(&rows).unwrap(),
            parent,
            CAPTURE_SOURCE,
            CAPTURE_START_NS,
            CAPTURE_END_NS,
        )
    })
    .is_err());
    rows[0]["created"] = Value::from((CAPTURE_END_NS / 1_000_000_000 + 86_400) as i64);
    assert!(std::panic::catch_unwind(|| {
        records(
            &serde_json::to_vec(&rows).unwrap(),
            parent,
            CAPTURE_SOURCE,
            CAPTURE_START_NS,
            CAPTURE_END_NS,
        )
    })
    .is_err());
}

fn exec_attempts(trace: &str) -> Vec<(String, Vec<String>)> {
    let start = regex::Regex::new(r#"^(\d+)\s+execve\("((?:[^"\\]|\\.)*)", (\[.*\]), "#).unwrap();
    let resumed = regex::Regex::new(r"^(\d+)\s+<\.\.\. execve resumed>\)\s+= 0$").unwrap();
    let mut pending = HashMap::<String, (String, Vec<String>)>::new();
    let mut result = Vec::new();
    for line in trace.lines().filter(|line| line.contains("execve")) {
        assert!(!line.contains("execveat("), "opaque execution: {line}");
        if let Some(fields) = start.captures(line) {
            let pid = fields[1].to_owned();
            let program: String = serde_json::from_str(&format!("\"{}\"", &fields[2])).unwrap();
            let argv: Vec<String> = serde_json::from_str(&fields[3]).unwrap();
            assert!(!argv.is_empty());
            if line.ends_with("<unfinished ...>") {
                assert!(pending.insert(pid, (program, argv)).is_none());
            } else {
                assert!(line.ends_with(") = 0"), "failed exec attempt: {line}");
                result.push((program, argv));
            }
        } else if let Some(fields) = resumed.captures(line) {
            result.push(pending.remove(&fields[1]).expect("orphan resumed execve"));
        } else {
            panic!("unclassified exec attempt: {line}");
        }
    }
    assert!(pending.is_empty(), "unfinished exec attempts");
    result
}

#[test]
fn executable_trace_parser_rejects_incomplete_or_failed_calls() {
    let valid = "7 execve(\"/usr/bin/git\", [\"git\"], 0x1) = 0\n8 execve(\"/bin/sh\", [\"sh\"], 0x2 <unfinished ...>\n8 <... execve resumed>) = 0";
    assert_eq!(exec_attempts(valid).len(), 2);
    for invalid in [
        "8 <... execve resumed>) = 0",
        "8 execve(\"/bin/sh\", [\"sh\"], 0x1 <unfinished ...>",
        "8 execve(\"/bin/sh\", [\"sh\"], 0x1) = -1 ENOENT",
        "8 execveat(AT_FDCWD, \"/bin/sh\", [\"sh\"], 0x1, 0) = 0",
    ] {
        assert!(std::panic::catch_unwind(|| exec_attempts(invalid)).is_err());
    }
}

const SYSTEM_PROGRAMS: &[&str] = &[
    "/usr/bin/git",
    "/usr/bin/git-lfs",
    "/usr/lib/git-core/git",
    "/usr/lib/git-core/git-submodule",
    "/usr/lib/git-core/git-upload-pack",
    "/bin/sh",
    "/usr/bin/bash",
    "/usr/bin/ionice",
    "/usr/bin/find",
    "/usr/bin/basename",
    "/usr/bin/envsubst",
    "/usr/bin/gettext",
    "/usr/bin/sed",
    "/usr/bin/uname",
];

fn path_executable(name: &str) -> PathBuf {
    for directory in env::split_paths(std::ffi::OsStr::new(PROOF_PATH)) {
        let candidate = directory.join(name);
        if fs::metadata(&candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        {
            return candidate;
        }
    }
    panic!("missing PATH executable: {name}");
}

fn admitted_system_interpreters(pins: &mut [Pin]) {
    let shell_pinned = pins.iter().any(|pin| pin.requested == Path::new("/bin/sh"));
    let machine = match std::env::consts::ARCH {
        "x86_64" => 62_u16,
        "aarch64" => 183_u16,
        other => panic!("unsupported host architecture: {other}"),
    };
    for pin in pins
        .iter_mut()
        .enumerate()
        .filter(|(index, pin)| {
            *index == 3 || SYSTEM_PROGRAMS.contains(&pin.requested.to_str().unwrap())
        })
        .map(|(_, pin)| pin)
    {
        pin.held.seek(SeekFrom::Start(0)).unwrap();
        let mut header = [0_u8; 64];
        let count = pin.held.read(&mut header).unwrap();
        if pin.requested == Path::new("/usr/lib/git-core/git-submodule")
            && header.starts_with(b"#!/bin/sh\n")
        {
            assert!(shell_pinned);
        } else {
            assert!(count >= 20 && header.starts_with(b"\x7fELF"));
            assert_eq!(header[4], 2);
            assert_eq!(header[5], 1);
            assert_eq!(u16::from_le_bytes([header[18], header[19]]), machine);
        }
    }
}

fn classify_exec(
    program: &str,
    runtime_root: &str,
    owned: &[PathBuf; 4],
    aliases: &BTreeMap<String, Value>,
) -> String {
    for (index, kind) in ["checker", "daemon", "launcher", "fixture"]
        .iter()
        .enumerate()
    {
        if program == owned[index].to_str().unwrap() {
            return (*kind).into();
        }
    }
    for name in ["git", "sudo", "bash", "ionice", "find"] {
        if program == format!("{runtime_root}/bin/{name}") {
            assert_eq!(aliases[name]["path"], program, "unbound private executable");
            return format!("private-{name}");
        }
    }
    assert!(
        SYSTEM_PROGRAMS.contains(&program),
        "unknown exec: {program}"
    );
    program.into()
}

fn aliases(output: &Path, runtime_root: &str, pins: &[Pin]) -> BTreeMap<String, Value> {
    let before = fs::read(output.join("runtime-aliases-before.json")).unwrap();
    let after = fs::read(output.join("runtime-aliases-after.json")).unwrap();
    assert_eq!(
        before, after,
        "runtime alias targets changed during the run"
    );
    let rows = json(&before).as_array().unwrap().clone();
    assert_eq!(rows.len(), 5);
    let mut result = BTreeMap::new();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        assert!(["git", "bash", "ionice", "find", "sudo"].contains(&name));
        assert_eq!(
            row["path"],
            format!("{runtime_root}/bin/{name}"),
            "alias path must belong to the private runtime"
        );
        assert_eq!(row["native_host_elf"], true);
        let target = Path::new(row["target_resolved"].as_str().unwrap());
        assert!(target.is_absolute());
        let pin = pins
            .iter()
            .find(|pin| pin.resolved == target)
            .expect("alias target is not a held fixture or system executable");
        if name == "sudo" {
            assert_eq!(
                pin.resolved, pins[3].resolved,
                "sudo must be the native fixture"
            );
        } else {
            assert_eq!(
                pin.resolved,
                fs::canonicalize(path_executable(name)).unwrap(),
                "system alias changed its admitted executable"
            );
        }
        assert_eq!(row["target_sha256"], pin.digest);
        assert_eq!(row["target_dev"].as_u64(), Some(pin.identity.0));
        assert_eq!(row["target_ino"].as_u64(), Some(pin.identity.1));
        assert!(row["link_dev"].as_u64().is_some());
        assert!(row["link_ino"].as_u64().is_some());
        assert!(Path::new(row["target"].as_str().unwrap()).is_absolute());
        assert!(result.insert(name.to_owned(), row).is_none());
    }
    assert_eq!(result.len(), 5);
    result
}

fn assert_observation(
    output: &Path,
    stdout: &[u8],
    stderr: &[u8],
    started_ns: i128,
    ended_ns: i128,
    source: &str,
    expected: &BTreeMap<String, String>,
) -> String {
    assert_eq!(stdout, sealed("original.stdout", expected).as_slice());
    assert_eq!(stdout, ORIGINAL_STDOUT.as_bytes());
    assert_eq!(stderr, sealed("original.stderr", expected).as_slice());
    let report = fs::read(output.join("result.json")).unwrap();
    assert_eq!(
        json(&report),
        json(&sealed("original/result.json", expected))
    );
    for name in ["daemon.log", "duplicate.log"] {
        assert_eq!(
            fs::read(output.join(name)).unwrap(),
            sealed(&format!("original/{name}"), expected),
            "raw daemon observation differs: {name}"
        );
    }
    let original_commands = sealed("original/overlay-commands.jsonl", expected);
    let candidate_commands = fs::read(output.join("overlay-commands.jsonl")).unwrap();
    let original_root = root_from_commands(&original_commands);
    let native_root = root_from_commands(&candidate_commands);
    assert_eq!(
        commands(&candidate_commands, &native_root),
        commands(&original_commands, &original_root)
    );
    assert_eq!(
        params(
            &fs::read(output.join("params.json")).unwrap(),
            &native_root,
            started_ns,
            ended_ns
        ),
        params(
            &sealed("original/params.json", expected),
            &original_root,
            CAPTURE_START_NS,
            CAPTURE_END_NS
        )
    );
    assert_eq!(
        records(
            &fs::read(output.join("records.json")).unwrap(),
            fixture_parent(&native_root),
            source,
            started_ns,
            ended_ns
        ),
        records(
            &sealed("original/records.json", expected),
            fixture_parent(&original_root),
            CAPTURE_SOURCE,
            CAPTURE_START_NS,
            CAPTURE_END_NS
        )
    );
    native_root
}

fn assert_raw_original_pair(
    original: &Path,
    native: &Path,
    original_stdout: &[u8],
    original_stderr: &[u8],
    native_stdout: &[u8],
    native_stderr: &[u8],
    original_window: (i128, i128),
    native_window: (i128, i128),
    original_source: &str,
    native_source: &str,
) {
    assert_eq!(native_stdout, original_stdout);
    assert_eq!(native_stdout, ORIGINAL_STDOUT.as_bytes());
    assert_eq!(native_stderr, original_stderr);
    assert!(native_stderr.is_empty());
    assert_eq!(
        json(&fs::read(native.join("result.json")).unwrap()),
        json(&fs::read(original.join("result.json")).unwrap())
    );
    for name in ["daemon.log", "duplicate.log"] {
        assert_eq!(
            fs::read(native.join(name)).unwrap(),
            fs::read(original.join(name)).unwrap()
        );
    }
    let original_commands = fs::read(original.join("overlay-commands.jsonl")).unwrap();
    let native_commands = fs::read(native.join("overlay-commands.jsonl")).unwrap();
    let original_root = root_from_commands(&original_commands);
    let native_root = root_from_commands(&native_commands);
    assert_eq!(
        commands(&native_commands, &native_root),
        commands(&original_commands, &original_root)
    );
    assert_eq!(
        params(
            &fs::read(native.join("params.json")).unwrap(),
            &native_root,
            native_window.0,
            native_window.1
        ),
        params(
            &fs::read(original.join("params.json")).unwrap(),
            &original_root,
            original_window.0,
            original_window.1
        )
    );
    assert_eq!(
        records(
            &fs::read(native.join("records.json")).unwrap(),
            fixture_parent(&native_root),
            native_source,
            native_window.0,
            native_window.1
        ),
        records(
            &fs::read(original.join("records.json")).unwrap(),
            fixture_parent(&original_root),
            original_source,
            original_window.0,
            original_window.1
        )
    );
}

fn raw_original_manifest(original: &Path) -> Value {
    let manifest_path = required("UPDATED_DAEMON_RAW_ORIGINAL_MANIFEST");
    let expected = env::var("UPDATED_DAEMON_RAW_ORIGINAL_MANIFEST_SHA256")
        .expect("original raw-path manifest SHA required");
    let bytes = fs::read(manifest_path).unwrap();
    assert_eq!(sha(&bytes), expected);
    let manifest: Value = json(&bytes);
    assert_eq!(manifest["checker_sha256"], CHECKER_SHA);
    assert_eq!(manifest["raw_output_suffix_hex"], "ff");
    assert!(manifest["raw_output_path_hex"]
        .as_str()
        .unwrap()
        .ends_with("ff"));
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), 8);
    for name in [
        "result.json",
        "params.json",
        "records.json",
        "overlay-commands.jsonl",
        "daemon.log",
        "duplicate.log",
        "original.stdout",
        "original.stderr",
    ] {
        let path = if name.starts_with("original.") {
            original.parent().unwrap().join(name)
        } else {
            original.join(name)
        };
        assert_eq!(sha(&fs::read(path).unwrap()), files[name].as_str().unwrap());
    }
    manifest
}

#[test]
fn identical_native_elves_match_original_with_present_and_absent_checker_roots() {
    let evidence = required("UPDATED_DAEMON_RETIREMENT_EVIDENCE");
    assert!(!evidence.exists(), "fresh evidence directory required");
    fs::create_dir_all(&evidence).unwrap();
    let manifest = manifest();
    let source = env::var("UPDATED_DAEMON_SOURCE_COMMIT").expect("checked-out source required");
    assert_eq!(source.len(), 40);
    assert!(source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let require_absent = match env::var("UPDATED_DAEMON_REQUIRE_ABSENT")
        .expect("explicit original-source state required")
        .as_str()
    {
        "0" => false,
        "1" => true,
        other => panic!("invalid UPDATED_DAEMON_REQUIRE_ABSENT: {other}"),
    };
    let checker = required("UPDATED_DAEMON_CHECKER");
    let daemon = required("UPDATED_DAEMON_BINARY");
    let launcher = required("UPDATED_DAEMON_LAUNCHER");
    let fixture = required("UPDATED_DAEMON_FIXTURE");
    let owned = [checker, daemon, launcher, fixture];
    let strace = PathBuf::from("/usr/bin/strace");
    let checkout = repo();
    let checkout_tools = checkout.join("rust/tools");
    let checker_source = checkout_tools.join("check_updated_daemon.py");
    let helper_source = checkout_tools.join("updated_fixtures.py");
    let parent_before = fs::metadata(&checkout_tools).unwrap();
    let parent_identity = (parent_before.dev(), parent_before.ino());
    let checker_present = source_present(&checker_source);
    assert_eq!(
        checker_present, !require_absent,
        "checkout checker source state"
    );
    assert!(source_present(&helper_source));
    let mut pins = owned
        .iter()
        .cloned()
        .chain(std::iter::once(strace.clone()))
        .chain(std::iter::once(helper_source.clone()))
        .chain(SYSTEM_PROGRAMS.iter().map(PathBuf::from))
        .chain(
            ["git", "bash", "ionice", "find"]
                .into_iter()
                .map(path_executable),
        )
        .map(Pin::new)
        .collect::<Vec<_>>();
    admitted_system_interpreters(&mut pins);
    assert_eq!(pins[5].digest, HELPER_SHA);
    if checker_present {
        let historical = Pin::new(checker_source.clone());
        assert_eq!(historical.digest, CHECKER_SHA);
        pins.push(historical);
    }
    let absent = evidence.join("absent-root");
    let private_tools = absent.join("rust/tools");
    fs::create_dir_all(&private_tools).unwrap();
    fs::copy(&helper_source, private_tools.join("updated_fixtures.py")).unwrap();
    let mut private_helper = Pin::new(private_tools.join("updated_fixtures.py"));
    assert_eq!(private_helper.digest, HELPER_SHA);
    assert!(!source_present(
        &private_tools.join("check_updated_daemon.py")
    ));
    fs::write(
        evidence.join("input-identities-before.json"),
        serde_json::to_vec_pretty(
            &pins
                .iter()
                .map(|pin| (pin.requested.display().to_string(), &pin.digest))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut rosters = Vec::new();
    for (lane, root) in [("checkout", &checkout), ("absent", &absent)] {
        let output = evidence.join(format!("{lane}-output"));
        let trace = evidence.join(format!("{lane}.exec"));
        let args = vec![
            owned[0].clone().into_os_string(),
            "--binary".into(),
            owned[1].clone().into_os_string(),
            "--launcher".into(),
            owned[2].clone().into_os_string(),
            "--fixture".into(),
            owned[3].clone().into_os_string(),
            "--output".into(),
            output.clone().into_os_string(),
            "--root".into(),
            root.clone().into_os_string(),
        ];
        let mut trace_args = vec![
            strace.clone().into_os_string(),
            "-f".into(),
            "-qq".into(),
            "-s65535".into(),
            "-e".into(),
            "trace=execve,execveat".into(),
            "-o".into(),
            trace.clone().into_os_string(),
        ];
        trace_args.extend(args.iter().cloned());
        let started_ns = now_ns();
        let mut command = CommandSpec::new_os(trace_args, Duration::from_secs(180));
        command.env.insert("PATH".into(), PROOF_PATH.into());
        let captured = NativeRunner.run(&command).unwrap();
        let ended_ns = now_ns();
        fs::write(evidence.join(format!("{lane}.stdout")), &captured.stdout).unwrap();
        fs::write(evidence.join(format!("{lane}.stderr")), &captured.stderr).unwrap();
        assert_eq!(captured.code, 0, "{lane} native controller failed");
        let runtime_root = assert_observation(
            &output,
            &captured.stdout,
            &captured.stderr,
            started_ns,
            ended_ns,
            &source,
            &manifest,
        );
        let identity: Value = json(&fs::read(output.join("input-identities.json")).unwrap());
        for (key, index) in [("binary", 1), ("launcher", 2), ("fixture", 3)] {
            assert_eq!(identity[key].as_str(), Some(pins[index].digest.as_str()));
        }
        assert_eq!(
            identity["historical_source_present"],
            lane == "checkout" && checker_present
        );
        assert_eq!(identity["unchanged_after_run"], true);
        let attempts = exec_attempts(&fs::read_to_string(&trace).unwrap());
        assert!(!attempts.is_empty(), "empty execution trace");
        let bound_aliases = aliases(&output, &runtime_root, &pins);
        let expected_first = args
            .iter()
            .map(|arg| arg.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(attempts[0].0, owned[0].to_str().unwrap());
        assert_eq!(attempts[0].1, expected_first);
        let mut roster = BTreeMap::<String, usize>::new();
        for (program, argv) in &attempts {
            assert!(!argv.is_empty());
            let kind = classify_exec(program, &runtime_root, &owned, &bound_aliases);
            *roster.entry(kind).or_default() += 1;
        }
        assert_eq!(roster["checker"], 1);
        assert_eq!(roster["daemon"], 2, "main and duplicate daemon");
        assert!(roster["private-sudo"] > 0);
        fs::write(
            evidence.join(format!("{lane}-exec-roster.json")),
            serde_json::to_vec_pretty(&roster).unwrap(),
        )
        .unwrap();
        rosters.push(roster);
        for pin in &mut pins {
            pin.verify();
        }
        private_helper.verify();
    }
    assert_eq!(rosters[0], rosters[1], "same ELF execution roster changed");
    assert_eq!(
        (
            fs::metadata(&checkout_tools).unwrap().dev(),
            fs::metadata(&checkout_tools).unwrap().ino()
        ),
        parent_identity
    );
    assert_eq!(source_present(&checker_source), checker_present);
    assert!(!source_present(
        &private_tools.join("check_updated_daemon.py")
    ));
    fs::write(
        evidence.join("input-identities-after.json"),
        serde_json::to_vec_pretty(
            &pins
                .iter()
                .map(|pin| (pin.requested.display().to_string(), &pin.digest))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn actual_non_utf8_output_path_matches_fresh_original_full_observation() {
    let original = required("UPDATED_DAEMON_RAW_ORIGINAL_OUTPUT");
    let manifest = raw_original_manifest(&original);
    let evidence = required("UPDATED_DAEMON_RAW_NATIVE_EVIDENCE");
    assert!(!evidence.exists(), "fresh raw-path evidence required");
    fs::create_dir_all(&evidence).unwrap();
    let raw_output = evidence.join(OsString::from_vec(b"native-raw-\xff".to_vec()));
    assert!(raw_output.to_str().is_none());
    let source = env::var("UPDATED_DAEMON_SOURCE_COMMIT").expect("source commit required");
    assert_eq!(source.len(), 40);
    let checker_source = repo().join("rust/tools/check_updated_daemon.py");
    let checker_present = source_present(&checker_source);
    let require_absent =
        env::var("UPDATED_DAEMON_REQUIRE_ABSENT").expect("original-source state required");
    assert!(matches!(require_absent.as_str(), "0" | "1"));
    assert_eq!(checker_present, require_absent == "0");
    let checker = required("UPDATED_DAEMON_CHECKER");
    let binary = required("UPDATED_DAEMON_BINARY");
    let launcher = required("UPDATED_DAEMON_LAUNCHER");
    let fixture = required("UPDATED_DAEMON_FIXTURE");
    let mut pins = [
        checker.clone(),
        binary.clone(),
        launcher.clone(),
        fixture.clone(),
    ]
    .into_iter()
    .chain(SYSTEM_PROGRAMS.iter().map(PathBuf::from))
    .chain(
        ["git", "bash", "ionice", "find"]
            .into_iter()
            .map(path_executable),
    )
    .map(Pin::new)
    .collect::<Vec<_>>();
    admitted_system_interpreters(&mut pins);
    for (name, index) in [("binary", 1), ("launcher", 2), ("fixture", 3)] {
        assert_eq!(
            manifest["native_inputs_sha256"][name], pins[index].digest,
            "fresh original raw-path input differs from native input"
        );
    }
    let mut helper = Pin::new(repo().join("rust/tools/updated_fixtures.py"));
    assert_eq!(helper.digest, HELPER_SHA);
    let original_pin = checker_present.then(|| {
        let pin = Pin::new(checker_source);
        assert_eq!(pin.digest, CHECKER_SHA);
        pin
    });
    let args = vec![
        checker.into_os_string(),
        "--binary".into(),
        binary.into_os_string(),
        "--launcher".into(),
        launcher.into_os_string(),
        "--fixture".into(),
        fixture.into_os_string(),
        "--output".into(),
        raw_output.clone().into_os_string(),
        "--root".into(),
        repo().into_os_string(),
    ];
    let started_ns = now_ns();
    let mut command = CommandSpec::new_os(args, Duration::from_secs(180));
    command.env.insert("PATH".into(), PROOF_PATH.into());
    let captured = NativeRunner.run(&command).unwrap();
    let ended_ns = now_ns();
    fs::write(evidence.join("native.stdout"), &captured.stdout).unwrap();
    fs::write(evidence.join("native.stderr"), &captured.stderr).unwrap();
    assert_eq!(captured.code, 0, "native raw-path controller failed");
    let start = manifest["start_ns"].as_i64().unwrap() as i128;
    let end = manifest["end_ns"].as_i64().unwrap() as i128;
    assert!(start <= end);
    let original_source = manifest["source_commit"].as_str().unwrap();
    assert_eq!(original_source.len(), 40);
    assert_raw_original_pair(
        &original,
        &raw_output,
        &fs::read(original.parent().unwrap().join("original.stdout")).unwrap(),
        &fs::read(original.parent().unwrap().join("original.stderr")).unwrap(),
        &captured.stdout,
        &captured.stderr,
        (start, end),
        (started_ns, ended_ns),
        original_source,
        &source,
    );
    let raw_root =
        root_from_commands(&fs::read(raw_output.join("overlay-commands.jsonl")).unwrap());
    let _bound_aliases = aliases(&raw_output, &raw_root, &pins);
    let identity: Value = json(&fs::read(raw_output.join("input-identities.json")).unwrap());
    for (key, index) in [("binary", 1), ("launcher", 2), ("fixture", 3)] {
        assert_eq!(identity[key].as_str(), Some(pins[index].digest.as_str()));
    }
    assert_eq!(identity["historical_source_present"], checker_present);
    for pin in &mut pins {
        pin.verify();
    }
    helper.verify();
    if let Some(mut pin) = original_pin {
        pin.verify();
    }
}
