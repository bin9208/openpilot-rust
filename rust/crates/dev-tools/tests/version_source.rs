//! Independent original Version policy expectations; no runtime-version candidate oracle.
use openpilot_dev_tools::version_source::{sources, Session};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

#[path = "support/version_source.rs"]
mod fixtures;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/version-tools")
}
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest() -> Value {
    let bytes = fs::read(fixture().join("provenance.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "f8c11250379f04b3c3e59ceee69375e4c30128bc42da632270a64c0b68b480ba"
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for (name, expected) in value["artifacts"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(fixture().join(name)).unwrap()),
            expected.as_str().unwrap(),
            "immutable original {name}"
        );
    }
    value
}

/// Match the original recursive normal(): only NaN becomes the string <NaN>.
/// API responses additionally preserve original JSON kinds, since arbitrary
/// metadata fields survive and their later properties depend on those kinds.
fn equal(expected: JsonValue, actual: JsonValue, label: &str) {
    let mut stack = vec![(expected, actual)];
    while let Some((expected, actual)) = stack.pop() {
        match (expected.view(), actual.view()) {
            (JsonView::Null, JsonView::Null) => {}
            (JsonView::Bool(a), JsonView::Bool(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Integer(a), JsonView::Integer(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Float(a), JsonView::Float(b)) => {
                assert!(a == b || (a.is_nan() && b.is_nan()), "{label}: {a} != {b}")
            }
            (JsonView::Text(a), JsonView::Text(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Float(a), JsonView::Text(b)) | (JsonView::Text(b), JsonView::Float(a))
                if a.is_nan() && b == [60, 78, 97, 78, 62] => {}
            (JsonView::Array(a), JsonView::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{label}");
                stack.extend(a.into_iter().zip(b));
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                assert_eq!(a.len(), b.len(), "{label}");
                for (key, value) in a {
                    let other = b
                        .iter()
                        .find(|(name, _)| *name == key)
                        .unwrap_or_else(|| panic!("{label}: missing object key"));
                    stack.push((value, other.1.clone()));
                }
            }
            _ => panic!(
                "{label}: {} != {}",
                expected.to_json().unwrap(),
                actual.to_json().unwrap()
            ),
        }
    }
}

struct RestorePermission(PathBuf);
impl Drop for RestorePermission {
    fn drop(&mut self) {
        if self.0.is_dir() {
            fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
}

#[test]
fn all_171_original_requests_preserve_real_git_filesystem_caches_properties_and_logs() {
    let pins = manifest();
    let rows: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture().join("reference-original.json")).unwrap())
            .unwrap();
    assert_eq!(rows.len(), 171);
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestorePermission(temp.path().join("unreadable"));
    let root = temp.path().to_str().unwrap();
    let parent_cwd = std::env::current_dir().unwrap();
    let parent_env: BTreeMap<_, _> = std::env::vars_os().collect();
    let mut source: Option<Session> = None;
    let mut previous = String::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        let name = row["scenario"].as_str().unwrap();
        if previous != name {
            let (cwd, environment) = fixtures::scenario(temp.path(), name);
            source = Some(Session::new(&cwd, &environment).unwrap());
            previous = name.into();
        }
        let index = counts.entry(name.into()).or_default();
        fixtures::before_call(temp.path(), name, *index);
        let request = row["request"]
            .as_str()
            .unwrap()
            .replace("@FIXTURE_ROOT@", root);
        let expected = JsonValue::parse(row["response"].as_str().unwrap()).unwrap();
        let actual = source
            .as_mut()
            .unwrap()
            .request(request.as_bytes())
            .unwrap_or_else(|e| panic!("{name}[{index}]: {e}"));
        equal(expected, actual, &format!("{name}[{index}]"));
        *index += 1;
    }
    assert_eq!(
        serde_json::to_value(counts).unwrap(),
        pins["source_schedule_counts"]
    );
    assert_eq!(
        std::env::current_dir().unwrap(),
        parent_cwd,
        "virtual source chdir must not mutate the test process"
    );
    assert_eq!(
        std::env::vars_os().collect::<BTreeMap<_, _>>(),
        parent_env,
        "Git policy belongs only to owned children"
    );
}

#[test]
fn original_13_protocol_controls_preserve_fatal_vs_caught_errors_and_worker_recovery() {
    manifest();
    let protocol: Value =
        serde_json::from_slice(&fs::read(fixture().join("protocol-original.json")).unwrap())
            .unwrap();
    let rows = protocol["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 13);
    let temp = tempfile::tempdir().unwrap();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let mut source = Session::new(temp.path(), &fixtures::environment()).unwrap();
        let requests: Vec<_> = row["request"].as_str().unwrap().lines().collect();
        let expected: Vec<_> = row["stdout"].as_str().unwrap().lines().collect();
        if row["exit"] == 1 {
            let result = source.request(requests[0].as_bytes());
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("{name}: uncaught assertion must terminate source protocol"),
            };
            assert_eq!(error.kind(), "AssertionError", "{name}");
        } else {
            assert_eq!(requests.len(), expected.len(), "{name}");
            for (request, expected) in requests.iter().zip(expected) {
                equal(
                    JsonValue::parse(expected).unwrap(),
                    source.request(request.as_bytes()).unwrap(),
                    name,
                );
            }
        }
    }
}

#[test]
fn all_live_source_pins_are_required_and_changed_present_files_are_rejected() {
    let pins = manifest();
    let root = repository();
    let expected: BTreeMap<String, String> =
        serde_json::from_value(pins["source_sha256"].clone()).unwrap();
    assert_eq!(expected.len(), 5);
    assert_eq!(sources(&root).unwrap(), expected);
    let temp = tempfile::tempdir().unwrap();
    for name in expected.keys() {
        if name == "rust/tools/check_version_collector.py" && !root.join(name).exists() {
            continue;
        }
        let file = temp.path().join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::copy(root.join(name), file).unwrap();
    }
    for name in expected.keys() {
        if name == "rust/tools/check_version_collector.py" {
            continue;
        }
        let file = temp.path().join(name);
        let original = fs::read(&file).unwrap();
        fs::write(&file, b"changed version source\n").unwrap();
        assert!(sources(temp.path()).is_err(), "changed {name}");
        fs::remove_file(&file).unwrap();
        assert!(sources(temp.path()).is_err(), "missing {name}");
        fs::write(file, original).unwrap();
    }
    assert_eq!(sources(temp.path()).unwrap(), expected);
}

#[test]
fn retired_collector_absence_is_explicit_and_present_or_dangling_changes_fail() {
    let expected: BTreeMap<String, String> =
        serde_json::from_value(manifest()["source_sha256"].clone()).unwrap();
    let root = repository();
    let temp = tempfile::tempdir().unwrap();
    let retired = "rust/tools/check_version_collector.py";
    for name in expected.keys().filter(|name| name.as_str() != retired) {
        let file = temp.path().join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::copy(root.join(name), file).unwrap();
    }
    // Only the exact retired file may be absent. The historical hash/key remains
    // fixed in native policy, while retained originals must still be read.
    assert_eq!(sources(temp.path()).unwrap(), expected);
    let file = temp.path().join(retired);
    fs::write(&file, b"changed retired collector\n").unwrap();
    assert!(sources(temp.path()).is_err());
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink("absent-collector-source", &file).unwrap();
    assert!(
        sources(temp.path()).is_err(),
        "dangling is not retired absence"
    );
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(
        sources(temp.path()).is_err(),
        "directory is not retired absence"
    );
    fs::remove_dir(&file).unwrap();
    assert_eq!(sources(temp.path()).unwrap(), expected);
}

#[test]
fn source_equality_gate_rejects_structural_error_log_and_property_corruption() {
    let actual = JsonValue::parse("{\"error\":\"Exception\",\"logs\":[{\"msg\":\"unable to get build metadata\",\"levelnum\":40,\"exc_info\":\"NoneType: None\\n\"}]}").unwrap();
    let original = fs::read_to_string(fixture().join("collector-source-original.json")).unwrap();
    equal(
        JsonValue::parse(&original).unwrap(),
        actual,
        "original collector policy event",
    );
    for corrupt in [
        "{\"error\":\"ValueError\",\"logs\":[]}",
        "{\"error\":\"Exception\",\"logs\":[]}",
        "{\"error\":\"Exception\",\"logs\":[{\"msg\":\"unable to get build metadata\",\"levelnum\":20,\"exc_info\":\"NoneType: None\\n\"}]}",
        "{\"value\":null,\"logs\":[]}",
    ] {
        assert!(std::panic::catch_unwind(|| equal(JsonValue::parse(&original).unwrap(), JsonValue::parse(corrupt).unwrap(), "deliberate corruption")).is_err());
    }
    let rows: Vec<Value> =
        serde_json::from_slice(&fs::read(fixture().join("reference-original.json")).unwrap())
            .unwrap();
    let expected = rows[0]["response"].as_str().unwrap();
    let mut corrupt: Value = serde_json::from_str(expected).unwrap();
    corrupt["value"]["properties"]["short_version"]["value"] = "corrupt".into();
    assert!(std::panic::catch_unwind(|| equal(
        JsonValue::parse(expected).unwrap(),
        JsonValue::parse(&corrupt.to_string()).unwrap(),
        "deliberate property corruption"
    ))
    .is_err());
}

fn admission() -> Value {
    let raw = include_bytes!("fixtures/version-tools/path-admission-original.json");
    assert_eq!(
        digest(raw),
        "8893dab9703177dbfa20d095064259bb29b4a7b8da5f728a9cd146fa7a5e08ce"
    );
    let value: Value = serde_json::from_slice(raw).unwrap();
    for (name, hash) in value["source_sha256"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(repository().join(name)).unwrap()),
            hash.as_str().unwrap()
        );
    }
    assert_eq!(
        value["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["cases"].as_array().unwrap().len())
            .sum::<usize>(),
        21
    );
    value
}

#[test]
fn original_empty_paths_keep_popen_and_chdir_errors_without_rejecting_pathlib_neighbors() {
    let capture = admission();
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fixtures::repo(&repo, true, true);
    let parent_cwd = std::env::current_dir().unwrap();
    let mut source = Session::new(&repo, &fixtures::environment()).unwrap();
    let group = capture["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["group"] == "empty-paths")
        .unwrap();
    assert_eq!(group["cases"].as_array().unwrap().len(), 8);
    for row in group["cases"].as_array().unwrap() {
        equal(
            JsonValue::parse(row["response"].as_str().unwrap()).unwrap(),
            source
                .request(row["request"].as_str().unwrap().as_bytes())
                .unwrap(),
            row["name"].as_str().unwrap(),
        );
    }
    assert_eq!(std::env::current_dir().unwrap(), parent_cwd);
}

#[test]
fn original_chdir_real_directory_file_pipe_and_bool_fds_keep_kernel_admission_and_owned_cwd() {
    use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
    use std::{ffi::OsString, os::fd::AsRawFd, time::Duration};
    // The captured bool False means stdin FD0. Use an isolated real test child
    // with piped stdin rather than changing any shared descriptor/global state.
    const MARKER: &str = "VERSION_SOURCE_FD_REGRESSION_CHILD";
    if std::env::var_os(MARKER).is_none() {
        let mut command = CommandSpec::new_os(vec![std::env::current_exe().unwrap().into_os_string(), "--exact".into(), "original_chdir_real_directory_file_pipe_and_bool_fds_keep_kernel_admission_and_owned_cwd".into(), "--nocapture".into()], Duration::from_secs(60));
        command.input = Some(Vec::new());
        command.set_os_env(BTreeMap::from([(OsString::from(MARKER), "1".into())]));
        let result = NativeRunner.run(&command).unwrap();
        assert_eq!(
            result.code,
            0,
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let capture = admission();
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fixtures::repo(&repo, true, true);
    let directory = temp.path().join("directory");
    let denied = temp.path().join("denied");
    fs::create_dir(&directory).unwrap();
    fs::create_dir(&denied).unwrap();
    let mut directory_fd = Some(fs::File::open(&directory).unwrap());
    let regular_fd = fs::File::open(repo.join("tracked")).unwrap();
    let (pipe_fd, _pipe_writer) = std::io::pipe().unwrap();
    let denied_fd = fs::File::open(&denied).unwrap();
    let _restore = RestorePermission(denied.clone());
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o0)).unwrap();
    let replace = [
        (
            "@DIR_FD@",
            directory_fd.as_ref().unwrap().as_raw_fd().to_string(),
        ),
        ("@FILE_FD@", regular_fd.as_raw_fd().to_string()),
        ("@PIPE_FD@", pipe_fd.as_raw_fd().to_string()),
        ("@DENIED_DIR_FD@", denied_fd.as_raw_fd().to_string()),
    ];
    let parent_cwd = std::env::current_dir().unwrap();
    let mut source = Session::new(&repo, &fixtures::environment()).unwrap();
    let group = capture["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["group"] == "descriptor-paths")
        .unwrap();
    assert_eq!(group["cases"].as_array().unwrap().len(), 11);
    for row in group["cases"].as_array().unwrap() {
        let mut request = row["request"].as_str().unwrap().to_owned();
        for (token, descriptor) in &replace {
            request = request.replace(token, descriptor);
        }
        equal(
            JsonValue::parse(row["response"].as_str().unwrap()).unwrap(),
            source.request(request.as_bytes()).unwrap(),
            row["name"].as_str().unwrap(),
        );
        if row["name"] == "directory-fd" {
            // The caller's fd may close after os.chdir(fd); cwd remains owned.
            drop(directory_fd.take());
        }
        assert_eq!(std::env::current_dir().unwrap(), parent_cwd);
    }
}

#[test]
fn original_chdir_outside_c_int_range_remains_uncaught_overflow_not_protocol_type_error() {
    let capture = admission();
    let temp = tempfile::tempdir().unwrap();
    let mut count = 0;
    for group in capture["groups"].as_array().unwrap() {
        if !group["group"].as_str().unwrap().starts_with("overflow-") {
            continue;
        }
        assert_eq!(group["exit"], 1);
        let mut source = Session::new(temp.path(), &fixtures::environment()).unwrap();
        for row in group["cases"].as_array().unwrap() {
            let error = match source.request(row["request"].as_str().unwrap().as_bytes()) {
                Err(error) => error,
                Ok(value) => panic!(
                    "{} must terminate original worker, got {}",
                    row["name"],
                    value.to_json().unwrap()
                ),
            };
            assert_eq!(error.kind(), row["fatal_error"].as_str().unwrap());
            count += 1;
        }
    }
    assert_eq!(count, 2);
}

#[test]
fn original_initial_cwd_identity_survives_directory_rename() {
    let bytes = fs::read(fixture().join("initial-cwd-rename-original.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "115b6d5cf978a493e8f1386543193476415d2dc2f839b29f11e0f85686434492"
    );
    let captured: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(captured["exit"], 0);
    assert_eq!(captured["stderr"], "");
    let rows = captured["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let temp = tempfile::tempdir().unwrap();
    let initial = temp.path().join("initial");
    fs::create_dir_all(initial.join("openpilot/common")).unwrap();
    fs::write(
        initial.join("openpilot/common/version.h"),
        "#define COMMA_VERSION \"rename-control\"\n",
    )
    .unwrap();
    let parent_cwd = std::env::current_dir().unwrap();
    let mut source = Session::new(&initial, &fixtures::environment()).unwrap();
    for row in rows {
        if row["phase"] == "after-rename" {
            fs::rename(&initial, temp.path().join("renamed")).unwrap();
        }
        equal(
            JsonValue::parse(row["response"].as_str().unwrap()).unwrap(),
            source
                .request(row["request"].as_str().unwrap().as_bytes())
                .unwrap(),
            row["phase"].as_str().unwrap(),
        );
    }
    assert_eq!(std::env::current_dir().unwrap(), parent_cwd);
}

// These tests exercise transport control only. The first shared-deadline Git
// invocation eventually execs real Git; no fixture supplies a policy answer.
struct SlowGit {
    _temporary: tempfile::TempDir,
    repository: PathBuf,
    state: PathBuf,
    environment: BTreeMap<std::ffi::OsString, std::ffi::OsString>,
}
impl SlowGit {
    fn new(mode: &str) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        fixtures::repo(&repository, true, true);
        let commands = temporary.path().join("commands");
        let state = temporary.path().join("state");
        fs::create_dir(&commands).unwrap();
        fs::create_dir(&state).unwrap();
        let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join("git"))
            .find(|path| {
                path.is_file() && path.metadata().unwrap().permissions().mode() & 0o111 != 0
            })
            .map(|path| fs::canonicalize(path).unwrap())
            .expect("real Git is a mandatory existing Version prerequisite");
        std::os::unix::fs::symlink(fixture().join("slow-git"), commands.join("git")).unwrap();
        let mut environment = fixtures::environment();
        environment.extend([
            ("PATH".into(), commands.into_os_string()),
            (
                "VERSION_SLOW_GIT_STATE".into(),
                state.as_os_str().to_owned(),
            ),
            ("VERSION_SLOW_GIT_MODE".into(), mode.into()),
            ("VERSION_SLOW_GIT_REAL".into(), real_git.into_os_string()),
        ]);
        Self {
            _temporary: temporary,
            repository,
            state,
            environment,
        }
    }
    fn start(
        &self,
        request: &'static [u8],
        deadline: std::time::Instant,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> (
        std::thread::JoinHandle<()>,
        std::sync::mpsc::Receiver<Result<String, String>>,
    ) {
        self.start_with_stderr(request, deadline, cancelled, None)
    }
    fn start_with_stderr(
        &self,
        request: &'static [u8],
        deadline: std::time::Instant,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        stderr: Option<fs::File>,
    ) -> (
        std::thread::JoinHandle<()>,
        std::sync::mpsc::Receiver<Result<String, String>>,
    ) {
        let (send, receive) = std::sync::mpsc::channel();
        let repository = self.repository.clone();
        let environment = self.environment.clone();
        let worker = std::thread::spawn(move || {
            let mut source = Session::new(&repository, &environment).unwrap();
            if let Some(stderr) = stderr {
                source.set_stderr(stderr);
            }
            // A controlled failure is an outer error, never a source JSON row.
            let result = source
                .request_controlled(request, deadline, cancelled)
                .map(|value| value.to_json().unwrap())
                .map_err(|error| error.kind().to_owned());
            let _ = send.send(result);
        });
        (worker, receive)
    }
    fn acknowledge(&self, call: usize) {
        fs::write(
            self.state.join(format!("ack.{call}")),
            b"live pidfds held\n",
        )
        .unwrap();
    }
    fn live_call(&self, call: usize, deadline: std::time::Instant) -> GitProcesses {
        use rustix::process::{pidfd_open, Pid, PidfdFlags};
        let marker = self.state.join(format!("call.{call}"));
        let mut token = b"VERSION_SLOW_GIT_STATE=".to_vec();
        token.extend_from_slice(self.state.as_os_str().as_encoded_bytes());
        while std::time::Instant::now() < deadline {
            if let Ok(marker) = fs::read_to_string(&marker) {
                let pids = marker
                    .split_whitespace()
                    .map(str::parse::<i32>)
                    .collect::<Result<Vec<_>, _>>();
                if let Ok(pids) = pids {
                    if pids.len() == 2 {
                        let mut owned = Vec::new();
                        for pid in pids {
                            let Some(identity) = Pid::from_raw(pid) else {
                                break;
                            };
                            let Ok(fd) = pidfd_open(identity, PidfdFlags::empty()) else {
                                break;
                            };
                            // Never signal a numeric identity solely because it
                            // appeared in a stale file: bind the live fixture token.
                            let environment =
                                fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
                            if !environment
                                .split(|byte| *byte == 0)
                                .any(|entry| entry == token)
                            {
                                break;
                            }
                            owned.push(fd);
                        }
                        if owned.len() == 2 {
                            use rustix::event::{poll, PollFd, PollFlags, Timespec};
                            let processes = GitProcesses(owned);
                            for fd in &processes.0 {
                                let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
                                assert_eq!(
                                    poll(&mut descriptors, Some(&Timespec::default())).unwrap(),
                                    0,
                                    "Git fixture identity exited before acknowledgement"
                                );
                            }
                            return processes;
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("actual Git call {call} never reached acknowledged startup");
    }
}
struct GitProcesses(Vec<rustix::fd::OwnedFd>);
impl GitProcesses {
    fn exited(&self, timeout: std::time::Duration) -> bool {
        use rustix::event::{poll, PollFd, PollFlags, Timespec};
        self.0.iter().all(|fd| {
            let mut descriptors = [PollFd::new(fd, PollFlags::IN)];
            poll(
                &mut descriptors,
                Some(&Timespec::try_from(timeout).unwrap()),
            )
            .unwrap()
                == 1
        })
    }
}
impl Drop for GitProcesses {
    fn drop(&mut self) {
        for fd in &self.0 {
            let _ = rustix::process::pidfd_send_signal(fd, rustix::process::Signal::KILL);
        }
    }
}

#[test]
fn controlled_source_request_shares_deadline_across_git_calls_and_returns_outer_timeout() {
    use std::{
        sync::{atomic::AtomicBool, Arc},
        time::{Duration, Instant},
    };
    let fixture = SlowGit::new("shared");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(5);
    let (worker, response) = fixture.start(
        br#"{"op":"metadata","path":"."}"#,
        deadline,
        Arc::new(AtomicBool::new(false)),
    );
    let first = fixture.live_call(1, deadline);
    // Deliberately consume part of ONE request's budget at an acknowledged gate.
    // This is controlled latency, never a guessed startup/readiness sleep.
    if let Some(remaining) =
        (started + Duration::from_secs(3)).checked_duration_since(Instant::now())
    {
        std::thread::sleep(remaining);
    }
    fixture.acknowledge(1);
    let second = fixture.live_call(2, deadline);
    fixture.acknowledge(2);
    let remaining = (deadline + Duration::from_secs(1)).saturating_duration_since(Instant::now());
    let actual = response
        .recv_timeout(remaining)
        .expect("request restarted its deadline for a later Git child");
    assert_eq!(
        actual,
        Err("TimeoutError".into()),
        "timeout became caught source JSON"
    );
    assert!(Instant::now() < deadline + Duration::from_secs(1));
    worker.join().unwrap();
    assert!(
        first.exited(Duration::from_secs(1)),
        "first real Git did not exit"
    );
    assert!(
        second.exited(Duration::from_secs(1)),
        "timed-out Git descendant survived"
    );
    assert_eq!(
        fs::read_to_string(fixture.state.join("count")).unwrap(),
        "2\n"
    );
}

#[test]
fn controlled_source_request_cancels_blocked_git_and_returns_outer_interruption() {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let fixture = SlowGit::new("cancel");
    let cancelled = Arc::new(AtomicBool::new(false));
    let (worker, response) = fixture.start(
        br#"{"op":"git","method":"commit"}"#,
        Instant::now() + Duration::from_secs(15),
        Arc::clone(&cancelled),
    );
    let processes = fixture.live_call(1, Instant::now() + Duration::from_secs(5));
    fixture.acknowledge(1);
    assert!(matches!(
        response.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    let interrupted = Instant::now();
    cancelled.store(true, Ordering::Relaxed);
    let actual = response
        .recv_timeout(Duration::from_secs(2))
        .expect("cancellation waited for the Git timeout");
    assert_eq!(
        actual,
        Err("InterruptedError".into()),
        "cancel became caught source JSON"
    );
    assert!(interrupted.elapsed() < Duration::from_secs(2));
    worker.join().unwrap();
    assert!(
        processes.exited(Duration::from_secs(1)),
        "cancelled Git descendant survived"
    );
    assert_eq!(
        fs::read_to_string(fixture.state.join("count")).unwrap(),
        "1\n"
    );
}

#[test]
fn source_git_stderr_streams_raw_bytes_before_cancel_and_survives_cleanup() {
    use std::{
        io::Write,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };
    let mut fixture = SlowGit::new("cancel");
    fixture
        .environment
        .insert("VERSION_SLOW_GIT_DIAGNOSTIC".into(), "1".into());
    let path = fixture.state.join("source.stderr");
    let mut stderr = fs::File::create(&path).unwrap();
    stderr.write_all(b"existing scenario prefix\n").unwrap();
    stderr.flush().unwrap();
    let expected = b"existing scenario prefix\nraw Git diagnostic:\xff\r\n";
    let cancelled = Arc::new(AtomicBool::new(false));
    let (worker, response) = fixture.start_with_stderr(
        br#"{"op":"git","method":"commit"}"#,
        Instant::now() + Duration::from_secs(15),
        Arc::clone(&cancelled),
        Some(stderr),
    );
    // The launcher emits stderr BEFORE this acknowledged readiness marker.
    // Observing the file now proves streaming rather than post-exit copying.
    let processes = fixture.live_call(1, Instant::now() + Duration::from_secs(5));
    assert_eq!(fs::read(&path).unwrap(), expected);
    fixture.acknowledge(1);
    assert!(matches!(
        response.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    cancelled.store(true, Ordering::Relaxed);
    assert_eq!(
        response
            .recv_timeout(Duration::from_secs(2))
            .expect("stderr routing blocked cancellation"),
        Err("InterruptedError".into()),
    );
    worker.join().unwrap();
    assert!(
        processes.exited(Duration::from_secs(1)),
        "Git descendant survived stderr-path cancellation"
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        expected,
        "cleanup truncated or duplicated raw stderr"
    );
    assert_eq!(
        fs::read_to_string(fixture.state.join("count")).unwrap(),
        "1\n"
    );
}
