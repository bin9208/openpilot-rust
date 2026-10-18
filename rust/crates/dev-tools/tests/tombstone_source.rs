//! Independent original tombstone Worker policy, never a runtime candidate oracle.
#![cfg(feature = "tombstone-tools-native")]

use openpilot_dev_tools::tombstone_source::{self as source, Session};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Cursor,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}
fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tombstone-tools")
}
fn complete_sources(root: &Path) -> BTreeMap<String, String> {
    let mut live = source::sources(root).unwrap();
    for (name, hash) in source::historical_sources(root).unwrap() {
        assert!(
            live.insert(name, hash).is_none(),
            "historical source labelled live"
        );
    }
    live
}
fn captured_source_identity(root: &Path, name: &str) -> String {
    complete_sources(root)
        .get(name)
        .cloned()
        .unwrap_or_else(|| {
            // Additional original build/schema inputs remain mandatory live files.
            digest(&fs::read(root.join(name)).unwrap())
        })
}
fn retired_helper(name: &str) -> bool {
    matches!(
        name,
        "rust/tools/check_version_reference.py"
            | "rust/tools/tombstoned_reference.py"
            | "rust/tools/check_tombstoned_reference.py"
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest() -> Value {
    let bytes = fs::read(fixtures().join("provenance.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "8c4914b4df2538f4c1806708bce8f6a83f6396342661edb61d276836c2122013"
    );
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(manifest["candidate_executed"], false);
    assert_eq!(manifest["build_or_install"], false);
    for (name, expected) in manifest["artifacts"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(fixtures().join(name)).unwrap()),
            expected.as_str().unwrap(),
            "original capture {name}"
        );
    }
    manifest
}
fn cases(name: &str, count: usize) -> Vec<Value> {
    manifest();
    let value: Value = serde_json::from_slice(&fs::read(fixtures().join(name)).unwrap()).unwrap();
    let cases = value["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    cases.clone()
}
fn same_json(expected: JsonValue, actual: JsonValue, label: &str) {
    let mut pending = vec![(expected, actual)];
    while let Some((a, b)) = pending.pop() {
        match (a.view(), b.view()) {
            (JsonView::Null, JsonView::Null) => {}
            (JsonView::Bool(a), JsonView::Bool(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Integer(a), JsonView::Integer(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Float(a), JsonView::Float(b)) => {
                assert!(a == b || (a.is_nan() && b.is_nan()), "{label}")
            }
            (JsonView::Text(a), JsonView::Text(b)) => assert_eq!(a, b, "{label}"),
            (JsonView::Array(a), JsonView::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{label}");
                pending.extend(a.into_iter().zip(b));
            }
            (JsonView::Object(a), JsonView::Object(b)) => {
                assert_eq!(a.len(), b.len(), "{label}");
                for (key, value) in a {
                    let (_, other) = b
                        .iter()
                        .find(|(k, _)| *k == key)
                        .unwrap_or_else(|| panic!("{label}: original field missing"));
                    pending.push((value, other.clone()));
                }
            }
            _ => panic!(
                "{label}: original {} != native {}",
                a.to_json().unwrap(),
                b.to_json().unwrap()
            ),
        }
    }
}
fn environment() -> BTreeMap<OsString, OsString> {
    std::env::vars_os().collect()
}

#[test]
fn fifteen_original_safe_and_filename_cases_preserve_codepoints_types_and_errors() {
    let temp = tempfile::tempdir().unwrap();
    let before_cwd = std::env::current_dir().unwrap();
    let before_env = environment();
    let mut source = Session::new(temp.path(), &before_env).unwrap();
    for row in cases("pure-original.json", 15) {
        let name = row["name"].as_str().unwrap();
        let request = row["request"].as_str().unwrap();
        let expected = JsonValue::parse(row["response"].as_str().unwrap()).unwrap();
        let actual = source.request(request.as_bytes()).unwrap();
        same_json(expected, actual, name);
    }
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        0,
        "pure source calls wrote files before configure"
    );
    assert_eq!(std::env::current_dir().unwrap(), before_cwd);
    assert_eq!(environment(), before_env);
}

#[test]
fn seventeen_original_protocol_errors_are_caught_and_session_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = Session::new(temp.path(), &environment()).unwrap();
    for row in cases("protocol-original.json", 17) {
        let name = row["name"].as_str().unwrap();
        let expected = JsonValue::parse(row["response"].as_str().unwrap()).unwrap();
        let actual = source
            .request(row["request"].as_str().unwrap().as_bytes())
            .unwrap();
        same_json(expected, actual, name);
        // Original tombstoned main catches AssertionError as well as malformed
        // JSON, unlike the Version source worker. Every error keeps it usable.
        let actual = source
            .request(br#"{"op":"safe","text":"recovery/test"}"#)
            .unwrap();
        same_json(
            JsonValue::parse(r#"{"result":{"value":"recoverytest"},"calls":[],"logs":[]}"#)
                .unwrap(),
            actual,
            name,
        );
    }
}

#[test]
fn seven_original_main_transports_preserve_eof_newlines_and_caught_error_recovery() {
    let temp = tempfile::tempdir().unwrap();
    for row in cases("transport-original.json", 7) {
        let name = row["name"].as_str().unwrap();
        let mut input = Cursor::new(row["stdin"].as_str().unwrap().as_bytes());
        let mut output = Vec::new();
        assert_eq!(row["exit"], 0);
        source::source(temp.path(), &environment(), &mut input, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        let actual = output.lines().collect::<Vec<_>>();
        let expected = row["stdout"].as_str().unwrap().lines().collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len(), "{name}");
        for (expected, actual) in expected.into_iter().zip(actual) {
            same_json(
                JsonValue::parse(expected).unwrap(),
                JsonValue::parse(actual).unwrap(),
                name,
            );
        }
    }
}

#[test]
fn all_nine_original_pins_remain_sealed_and_live_policy_is_required() {
    let manifest = manifest();
    let expected = manifest["source_sha256"].as_object().unwrap();
    assert_eq!(expected.len(), 9);
    let actual = complete_sources(&repository());
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        manifest["source_sha256"]
    );
    let temp = tempfile::tempdir().unwrap();
    for name in expected.keys() {
        let path = temp.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if let Ok(bytes) = fs::read(repository().join(name)) {
            fs::write(path, bytes).unwrap();
        }
    }
    source::sources(temp.path()).unwrap();
    for name in expected.keys() {
        let path = temp.path().join(name);
        let original = fs::read(&path).ok();
        fs::write(&path, b"changed original policy must be rejected\n").unwrap();
        assert!(source::sources(temp.path()).is_err(), "changed {name}");
        fs::remove_file(&path).unwrap();
        if retired_helper(name) {
            assert!(!source::sources(temp.path()).unwrap().contains_key(name));
        } else {
            assert!(source::sources(temp.path()).is_err(), "absent {name}");
        }
        symlink("absent-original", &path).unwrap();
        assert!(source::sources(temp.path()).is_err(), "dangling {name}");
        fs::remove_file(&path).unwrap();
        if let Some(original) = original {
            fs::write(path, original).unwrap();
        }
    }
    source::sources(temp.path()).unwrap();
}

fn stateful_fixture(name: &str, count: usize) -> Value {
    // This seal is separate from the pure foundation. It binds fresh full
    // original Worker captures, actual Params binding, and replayed FS actions.
    let bytes = fs::read(fixtures().join("stateful-provenance.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "53c81759802e28d4bd0748d17e1b819de5dbdc5f20fe057fbafb3cafff330371"
    );
    let provenance: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(provenance["candidate_executed"], false);
    assert_eq!(provenance["build_or_install"], false);
    assert_eq!(provenance["original_fixture_replay"]["cases"], 72);
    for (path, expected) in provenance["source_sha256"].as_object().unwrap() {
        assert_eq!(
            captured_source_identity(&repository(), path),
            expected.as_str().unwrap(),
            "original capture source {path}"
        );
    }
    for (path, expected) in provenance["artifacts"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(fixtures().join(path)).unwrap()),
            expected.as_str().unwrap(),
            "immutable stateful capture {path}"
        );
    }
    let value: Value = serde_json::from_slice(&fs::read(fixtures().join(name)).unwrap()).unwrap();
    assert_eq!(value["cases"].as_array().unwrap().len(), count);
    value
}

fn bytes_from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn filesystem_actions(root: &Path, actions: &Value) {
    for action in actions.as_array().unwrap() {
        let relative = if let Some(bytes) = action["path_hex"].as_str() {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(OsString::from_vec(bytes_from_hex(bytes)))
        } else {
            PathBuf::from(action["path"].as_str().unwrap())
        };
        assert!(
            relative.is_relative()
                && !relative
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
        );
        let path = root.join(relative);
        match action["op"].as_str().unwrap() {
            "write" => {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, bytes_from_hex(action["hex"].as_str().unwrap())).unwrap();
                if let Some(mode) = action["mode"].as_u64() {
                    fs::set_permissions(&path, fs::Permissions::from_mode(mode as u32)).unwrap();
                }
            }
            "mkdir" => fs::create_dir_all(&path).unwrap(),
            "symlink" => symlink(action["target"].as_str().unwrap(), &path).unwrap(),
            "sparse" => {
                use std::io::Write;
                let mut file = fs::File::create(&path).unwrap();
                file.write_all(&bytes_from_hex(action["prefix_hex"].as_str().unwrap()))
                    .unwrap();
                file.set_len(action["size"].as_u64().unwrap()).unwrap();
                fs::set_permissions(
                    &path,
                    fs::Permissions::from_mode(action["mode"].as_u64().unwrap() as u32),
                )
                .unwrap();
            }
            "touch_range" => {
                let digits = action["digits"].as_u64().unwrap() as usize;
                for index in action["start"].as_u64().unwrap()..action["end"].as_u64().unwrap() {
                    fs::File::create(path.join(format!(
                        "{}{:0digits$}",
                        action["prefix"].as_str().unwrap(),
                        index
                    )))
                    .unwrap();
                }
            }
            "remove" => match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("remove {}: {error}", path.display()),
            },
            "chmod" => fs::set_permissions(
                &path,
                fs::Permissions::from_mode(action["mode"].as_u64().unwrap() as u32),
            )
            .unwrap(),
            op => panic!("unknown source fixture action {op}"),
        }
    }
}

// Restore only fixture-owned permissions after an assertion panic. A source
// permission-error regression must not leave an undeletable temporary tree.
struct StatefulTree(tempfile::TempDir);
impl Drop for StatefulTree {
    fn drop(&mut self) {
        fn restore(path: &Path) {
            if let Ok(metadata) = fs::symlink_metadata(path) {
                if metadata.is_dir() {
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.flatten() {
                            restore(&entry.path());
                        }
                    }
                } else if metadata.is_file() {
                    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
                }
            }
        }
        restore(self.0.path());
    }
}

fn same_stateful_record(expected: JsonValue, actual: JsonValue, label: &str) {
    let mut pending = vec![(expected, actual)];
    while let Some((expected, actual)) = pending.pop() {
        match (expected.view(), actual.view()) {
            (JsonView::Object(expected), JsonView::Object(actual)) => {
                assert_eq!(expected.len(), actual.len(), "{label}: record fields");
                for (key, expected) in expected {
                    let (_, actual) = actual
                        .iter()
                        .find(|(other, _)| *other == key)
                        .unwrap_or_else(|| panic!("{label}: original field missing"));
                    if key.iter().copied().eq("created".chars().map(u32::from)) {
                        // Original Pair.normalize excludes this wall clock only;
                        // its actual field/type is retained by the source API.
                        assert!(
                            matches!(expected.view(), JsonView::Float(time) if time.is_finite())
                        );
                        assert!(
                            matches!(actual.view(), JsonView::Float(time) if time.is_finite() && time > 0.0)
                        );
                    } else if key.iter().copied().eq("value_json".chars().map(u32::from)) {
                        let (JsonView::Text(expected), JsonView::Text(actual)) =
                            (expected.view(), actual.view())
                        else {
                            panic!("{label}: original SDK value_json must remain text")
                        };
                        pending.push((
                            JsonValue::parse_codepoints(expected).unwrap(),
                            JsonValue::parse_codepoints(actual).unwrap(),
                        ));
                    } else {
                        pending.push((expected, actual.clone()));
                    }
                }
            }
            (JsonView::Array(expected), JsonView::Array(actual)) => {
                assert_eq!(expected.len(), actual.len(), "{label}: ordered records");
                pending.extend(expected.into_iter().zip(actual));
            }
            _ => same_json(expected, actual, label),
        }
    }
}

fn check_params_file(root: &Path, relative: &str, expected: &Value, label: &str) {
    let path = root.join(relative);
    let actual = match fs::read(&path) {
        Ok(bytes) => Value::String(bytes_to_hex(&bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(error) => panic!("{label}: reading {}: {error}", path.display()),
    };
    assert_eq!(&actual, expected, "{label}: actual Params bytes {relative}");
}

fn replay_stateful(name: &str, count: usize) {
    manifest();
    let fixture = stateful_fixture(name, count);
    let tree = StatefulTree(tempfile::tempdir().unwrap());
    let root = tree.0.path().join("source");
    for directory in ["apport", "logs", "base/openpilot/common", "params/fixture"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    filesystem_actions(&root, &fixture["setup"]);
    let cwd_before = std::env::current_dir().unwrap();
    let env_before = environment();
    let mut session = Session::new(&root, &env_before).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let label = format!("{name}: original call {}", row["index"]);
        filesystem_actions(&root, &row["before"]);
        let request = row["request"]
            .as_str()
            .unwrap()
            .replace("$FIXTURE", root.to_str().unwrap());
        // Restore restrictive source fixture modes before reporting any error.
        let response = session.request(request.as_bytes());
        filesystem_actions(&root, &row["after"]);
        let actual = response.unwrap_or_else(|error| panic!("{label}: outer source error {error}"));
        let expected = JsonValue::parse(row["original_stdout"].as_str().unwrap()).unwrap();
        same_stateful_record(expected, actual, &label);
        if let Some(files) = row["source_files"].as_object() {
            assert_eq!(
                files.len(),
                1,
                "bounded source group unexpectedly gained files"
            );
            check_params_file(
                &root,
                "params/fixture/CarrotException",
                &files["CarrotException"],
                &label,
            );
            for directory in ["apport", "logs"] {
                assert_eq!(
                    fs::read_dir(root.join(directory)).unwrap().count(),
                    0,
                    "{label}: unexpected {directory} effects"
                );
            }
        }
    }
    if let Some(files) = fixture["final_files"].as_object() {
        for (relative, expected) in files {
            check_params_file(&root, relative, expected, name);
        }
    }
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
    assert_eq!(environment(), env_before);
}

#[test]
fn fifty_original_sentry_gates_capture_tombstone_and_real_params_error_order() {
    replay_stateful("stateful-gates-original.json", 50);
}

#[test]
fn nine_original_partial_configure_calls_preserve_assignment_cache_and_new_params_identity() {
    replay_stateful("stateful-configure-original.json", 9);
}

#[test]
fn thirteen_original_tags_preserve_lossless_values_sdk_order_errors_and_recovery() {
    replay_stateful("stateful-tag-original.json", 13);
}

#[cfg(target_os = "linux")]
fn original_file_fixture(name: &str, count: usize) -> Value {
    let bytes = fs::read(fixtures().join("files-provenance.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "16af9aa8a1157693385b16a19d2b3637bf20d20450a46a3850ce86dcb4f4586c"
    );
    let provenance: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(provenance["candidate_executed"], false);
    assert_eq!(provenance["compiler_build_install"], false);
    for (name, expected) in provenance["artifacts"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(fixtures().join(name)).unwrap()),
            expected.as_str().unwrap(),
            "original file capture {name}"
        );
    }
    for (name, expected) in provenance["source_sha256"].as_object().unwrap() {
        assert_eq!(
            captured_source_identity(&repository(), name),
            expected.as_str().unwrap(),
            "original file policy {name}"
        );
    }
    let value: Value = serde_json::from_slice(&fs::read(fixtures().join(name)).unwrap()).unwrap();
    assert_eq!(value["cases"].as_array().unwrap().len(), count);
    value
}

#[cfg(target_os = "linux")]
fn crash_snapshot_lossless(root: &Path) -> JsonValue {
    use openpilot_dev_tools::python_fs;
    use std::io::Read;
    fn visit(root: &Path, path: &Path, out: &mut Vec<(String, String)>) {
        let metadata = fs::metadata(path).unwrap(); // original Path.stat follows links
        let mode = metadata.permissions().mode() & 0o7777;
        let key = python_fs::json_string(path.strip_prefix(root).unwrap().as_os_str()).unwrap();
        let value = if metadata.is_dir() {
            serde_json::json!({"directory":true,"mode":mode}).to_string()
        } else if path.is_symlink() {
            let target = fs::read_link(path).unwrap();
            format!(
                "{{\"symlink\":{},\"mode\":{mode}}}",
                python_fs::json_string(target.file_name().unwrap()).unwrap()
            )
        } else {
            let mut file = fs::File::open(path).unwrap();
            let mut hash = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                let count = file.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            serde_json::json!({"size":metadata.len(),"sha256":format!("{:x}",hash.finalize()),"mode":mode}).to_string()
        };
        out.push((key, value));
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut values = Vec::new();
    for directory in ["apport", "logs"] {
        for entry in fs::read_dir(root.join(directory)).unwrap() {
            visit(root, &entry.unwrap().path(), &mut values);
        }
    }
    let value = root.join("params/fixture/CarrotException");
    values.push((
        "\"CarrotException\"".into(),
        if value.is_file() {
            serde_json::to_string(&bytes_to_hex(&fs::read(value).unwrap())).unwrap()
        } else {
            "null".into()
        },
    ));
    JsonValue::parse(&format!(
        "{{{}}}",
        values
            .into_iter()
            .map(|(key, value)| format!("{key}:{value}"))
            .collect::<Vec<_>>()
            .join(",")
    ))
    .unwrap()
}

#[cfg(target_os = "linux")]
fn crash_snapshot(root: &Path) -> Value {
    // The earlier33-file corpus has ASCII filenames. Raw loop snapshots use the
    // lossless node above directly, never a serde String/Value narrowing step.
    serde_json::from_str(&crash_snapshot_lossless(root).to_json().unwrap()).unwrap()
}

#[cfg(target_os = "linux")]
fn bind_default_modes(snapshot: &mut Value, directory_mode: u32, file_mode: u32) {
    for entry in snapshot.as_object_mut().unwrap().values_mut() {
        if entry["directory"] == true && entry["mode"] == 0o775 {
            entry["mode"] = directory_mode.into();
        } else if entry["mode"] == 0o664 {
            entry["mode"] = file_mode.into();
        }
    }
}

// The source explicitly consumes actual directory enumeration order BEFORE its
// predicates and follows stat targets for mode/ctime. Bind both to this fixture,
// rather than sort first or use old epochs from the original capture machine.
#[cfg(target_os = "linux")]
fn live_original_scan(path: &Path) -> Value {
    use std::os::unix::fs::MetadataExt;
    let mut rows = Vec::new();
    if path.exists() {
        for entry in fs::read_dir(path).unwrap().take(1000) {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_str().unwrap();
            if name.starts_with("tombstone") || name.ends_with(".crash") {
                let metadata = match fs::metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return serde_json::json!({"result":{"error":"FileNotFoundError"},"calls":[],"logs":[]});
                    }
                    Err(error) => panic!("source fixture stat: {error}"),
                };
                if name.starts_with("tombstone") || metadata.mode() == 0o100640 {
                    rows.push((entry.path().to_str().unwrap().to_owned(), metadata.ctime()));
                }
            }
        }
    }
    rows.sort(); // original Worker.handle sorts the source get_tombstones result
    serde_json::json!({"result":{"value":rows},"calls":[],"logs":[]})
}

#[cfg(target_os = "linux")]
struct RetraceHelpers(PathBuf);
#[cfg(target_os = "linux")]
impl RetraceHelpers {
    fn terminate_leaked_identity(&self) -> bool {
        use rustix::{
            event::{poll, PollFd, PollFlags, Timespec},
            process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
        };
        let marker = self.0.join("helper.pid");
        let Some(pid) = fs::read_to_string(&marker)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .and_then(Pid::from_raw)
        else {
            return false;
        };
        let Ok(identity) = pidfd_open(pid, PidfdFlags::empty()) else {
            return false;
        };
        let token = format!("FIXTURE_PID={}", marker.display());
        let environment =
            fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap_or_default();
        if !environment
            .split(|byte| *byte == 0)
            .any(|entry| entry == token.as_bytes())
        {
            return false;
        }
        let mut descriptors = [PollFd::new(&identity, PollFlags::IN)];
        if poll(
            &mut descriptors,
            Some(&Timespec::try_from(std::time::Duration::from_secs(1)).unwrap()),
        )
        .unwrap()
            != 0
        {
            return false;
        }
        // Cleanup may signal only the live pidfd whose private fixture token
        // was checked after opening it. Numeric PID/group reuse cannot qualify.
        pidfd_send_signal(&identity, Signal::KILL).unwrap();
        true
    }
}
#[cfg(target_os = "linux")]
impl Drop for RetraceHelpers {
    fn drop(&mut self) {
        self.terminate_leaked_identity();
    }
}

#[cfg(target_os = "linux")]
fn replay_original_files(name: &str, count: usize) {
    manifest();
    let fixture = original_file_fixture(name, count);
    let tree = StatefulTree(tempfile::tempdir().unwrap());
    let root = tree.0.path().join("source");
    fs::create_dir(&root).unwrap();
    // Keep the reserve even for the exact100MB accepted boundary/copy case.
    openpilot_disk_space::inspect(&root, 256 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    for directory in [
        "apport",
        "logs",
        "base/openpilot/common",
        "params/fixture",
        "bin",
    ] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    let default_directory_mode = fs::metadata(root.join("logs"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    let probe = root.join("default-write-mode");
    fs::write(&probe, []).unwrap();
    let default_file_mode = fs::metadata(&probe).unwrap().permissions().mode() & 0o7777;
    fs::remove_file(probe).unwrap();
    symlink(
        fixtures().join("apport-retrace"),
        root.join("bin/apport-retrace"),
    )
    .unwrap();
    filesystem_actions(&root, &fixture["setup"]);
    let env_before = environment();
    let cwd_before = std::env::current_dir().unwrap();
    let mut env = env_before.clone();
    env.insert(
        "PATH".into(),
        format!("{}:/usr/bin:/bin", root.join("bin").display()).into(),
    );
    for (key, relative) in [
        ("FIXTURE_INPUT", "retrace.input"),
        ("FIXTURE_MODE", "mode"),
        ("FIXTURE_TRACE", "trace"),
        ("FIXTURE_PID", "helper.pid"),
    ] {
        env.insert(key.into(), root.join(relative).into_os_string());
    }
    let helper_cleanup = RetraceHelpers(root.clone());
    let mut session = Session::new(&root, &env).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let label = format!("{name}: original call {}", row["index"]);
        filesystem_actions(&root, &row["before"]);
        let request = row["request"]
            .as_str()
            .unwrap()
            .replace("$FIXTURE", root.to_str().unwrap())
            .replace("$SHARED", root.join("shared").to_str().unwrap());
        let request_value: Value = serde_json::from_str(&request).unwrap();
        let expected_scan = (request_value["op"] == "scan")
            .then(|| live_original_scan(Path::new(request_value["path"].as_str().unwrap())));
        let started = std::time::Instant::now();
        let response = session.request(request.as_bytes());
        let elapsed = started.elapsed();
        let actual_files = row["source_files"]
            .is_object()
            .then(|| crash_snapshot(&root));
        filesystem_actions(&root, &row["after"]);
        let leaked = helper_cleanup.terminate_leaked_identity();
        assert!(
            !leaked,
            "{label}: timed-out retrace helper survived source request cleanup"
        );
        let actual = response.unwrap_or_else(|error| panic!("{label}: outer source error {error}"));
        let expected = if let Some(expected) = expected_scan {
            JsonValue::parse(&expected.to_string()).unwrap()
        } else {
            JsonValue::parse(
                &row["original_stdout"]
                    .as_str()
                    .unwrap()
                    .replace("$FIXTURE", root.to_str().unwrap())
                    .replace("$SHARED", root.join("shared").to_str().unwrap()),
            )
            .unwrap()
        };
        same_stateful_record(expected, actual, &label);
        if let Some(actual_files) = actual_files {
            let mut expected_files = row["source_files"].clone();
            bind_default_modes(
                &mut expected_files,
                default_directory_mode,
                default_file_mode,
            );
            assert_eq!(
                actual_files, expected_files,
                "{label}: source file snapshot"
            );
        }
        if let Some(milliseconds) = row["minimum_elapsed_ms"].as_u64() {
            assert!(
                elapsed >= std::time::Duration::from_millis(milliseconds),
                "{label}: original timeout returned early: {elapsed:?}"
            );
            let configured = request_value["timeout_ms"].as_u64().unwrap();
            assert!(
                elapsed
                    < std::time::Duration::from_millis(configured)
                        + std::time::Duration::from_secs(5),
                "{label}: retrace deadline cleanup exceeded bound: {elapsed:?}"
            );
        }
    }
    assert_eq!(std::env::current_dir().unwrap(), cwd_before);
    assert_eq!(environment(), env_before);
}

#[test]
#[cfg(target_os = "linux")]
fn thirty_three_original_reports_preserve_parser_copy_delete_permissions_and_100mb_boundary() {
    replay_original_files("stateful-files-original.json", 33);
}

#[test]
#[cfg(target_os = "linux")]
fn six_original_scanner_calls_bind_first_thousand_entries_stat_ctimes_and_hidden_cleanup() {
    replay_original_files("stateful-scan-original.json", 6);
}

#[test]
#[cfg(target_os = "linux")]
fn five_original_retrace_calls_preserve_real_30second_timeout_and_owned_helper_cleanup() {
    replay_original_files("stateful-subprocess-original.json", 5);
}

#[cfg(target_os = "linux")]
fn remaining_fixture(name: &str, count: usize) -> Value {
    let bytes = fs::read(fixtures().join("remaining-provenance.json")).unwrap();
    assert_eq!(
        digest(&bytes),
        "d1700bd83b97156146468b1c8f449e50972bfbab6d584cb84b867bd3fa7c6845"
    );
    let provenance: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(provenance["candidate_executed"], false);
    assert_eq!(provenance["compiler_build_install"], false);
    assert_eq!(provenance["counts"]["text"], 134);
    assert_eq!(provenance["counts"]["reporting-loop"], 7);
    assert_eq!(provenance["counts"]["disabled-loop"], 7);
    for (path, expected) in provenance["source_sha256"].as_object().unwrap() {
        assert_eq!(
            captured_source_identity(&repository(), path),
            expected.as_str().unwrap(),
            "original remaining capture source {path}"
        );
    }
    for (path, expected) in provenance["artifacts"].as_object().unwrap() {
        assert_eq!(
            digest(&fs::read(fixtures().join(path)).unwrap()),
            expected.as_str().unwrap(),
            "immutable remaining source capture {path}"
        );
    }
    let value: Value = serde_json::from_slice(&fs::read(fixtures().join(name)).unwrap()).unwrap();
    assert_eq!(value["cases"].as_array().unwrap().len(), count);
    value
}

#[cfg(target_os = "linux")]
fn snapshot_modes_lossless(snapshot: JsonValue, directory_mode: u32, file_mode: u32) -> JsonValue {
    fn object(fields: Vec<(Vec<u32>, JsonValue)>) -> JsonValue {
        let fields = fields
            .into_iter()
            .map(|(key, value)| {
                format!(
                    "{}:{}",
                    JsonValue::codepoints(key).unwrap().to_json().unwrap(),
                    value.to_json().unwrap()
                )
            })
            .collect::<Vec<_>>();
        JsonValue::parse(&format!("{{{}}}", fields.join(","))).unwrap()
    }
    let JsonView::Object(entries) = snapshot.view() else {
        panic!("original snapshot must be an object")
    };
    let mut result = Vec::new();
    for (key, value) in entries {
        let value = if let JsonView::Object(fields) = value.view() {
            let directory = value
                .get("directory")
                .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)));
            object(
                fields
                    .into_iter()
                    .map(|(field, value)| {
                        let value = if field.iter().copied().eq("mode".chars().map(u32::from)) {
                            match value.view() {
                                JsonView::Integer("509") if directory => {
                                    JsonValue::parse(&directory_mode.to_string()).unwrap()
                                }
                                JsonView::Integer("436") => {
                                    JsonValue::parse(&file_mode.to_string()).unwrap()
                                }
                                _ => value,
                            }
                        } else {
                            value
                        };
                        (field.to_vec(), value)
                    })
                    .collect(),
            )
        } else {
            value
        };
        result.push((key.to_vec(), value));
    }
    object(result)
}

#[cfg(target_os = "linux")]
fn original_cycle_log_key(value: &JsonValue) -> String {
    // Original Pair.normalize recursively drops `created`, then cycle ONLY
    // sorts logs by json.dumps(record, sort_keys=True). Keep that exact order
    // projection without applying it to SDK calls or any noncycle response.
    match value.view() {
        JsonView::Object(fields) => {
            let mut fields = fields
                .into_iter()
                .filter(|(key, _)| !key.iter().copied().eq("created".chars().map(u32::from)))
                .collect::<Vec<_>>();
            fields.sort_by(|(a, _), (b, _)| a.cmp(b));
            format!(
                "{{{}}}",
                fields
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}: {}",
                        JsonValue::codepoints(key.to_vec())
                            .unwrap()
                            .to_json()
                            .unwrap(),
                        original_cycle_log_key(&value)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        JsonView::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(original_cycle_log_key)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => value.to_json().unwrap(),
    }
}

#[cfg(target_os = "linux")]
fn same_cycle_record(expected: JsonValue, actual: JsonValue, label: &str) {
    let (JsonView::Object(expected_fields), JsonView::Object(actual_fields)) =
        (expected.view(), actual.view())
    else {
        panic!("{label}: original response must remain an object")
    };
    assert_eq!(expected_fields.len(), 3, "{label}");
    assert_eq!(actual_fields.len(), 3, "{label}");
    for name in ["result", "calls"] {
        same_stateful_record(
            expected.get(name).unwrap(),
            actual.get(name).unwrap(),
            label,
        );
    }
    let sorted = |record: &JsonValue| {
        let logs = record.get("logs").unwrap();
        let JsonView::Array(logs) = logs.view() else {
            panic!("{label}: logs must remain an array")
        };
        let mut logs = logs
            .into_iter()
            .map(|log| (original_cycle_log_key(&log), log))
            .collect::<Vec<_>>();
        logs.sort_by(|(a, _), (b, _)| a.cmp(b));
        logs
    };
    let expected = sorted(&expected);
    let actual = sorted(&actual);
    assert_eq!(
        expected.len(),
        actual.len(),
        "{label}: cycle log multiplicity"
    );
    for ((key, expected), (actual_key, actual)) in expected.into_iter().zip(actual) {
        assert_eq!(key, actual_key, "{label}: original normalized cycle log");
        same_stateful_record(expected, actual, label);
    }
}

#[cfg(target_os = "linux")]
fn replay_remaining(name: &str, count: usize) {
    manifest();
    let fixture = remaining_fixture(name, count);
    let tree = StatefulTree(tempfile::tempdir().unwrap());
    let root = tree.0.path().join("source");
    for directory in [
        "apport",
        "logs",
        "base/openpilot/common",
        "params/fixture",
        "bin",
    ] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    let directory_mode = fs::metadata(root.join("logs"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    let probe = root.join("default-write-mode");
    fs::write(&probe, []).unwrap();
    let file_mode = fs::metadata(&probe).unwrap().permissions().mode() & 0o7777;
    fs::remove_file(probe).unwrap();
    symlink(
        fixtures().join("apport-retrace"),
        root.join("bin/apport-retrace"),
    )
    .unwrap();
    filesystem_actions(&root, &fixture["setup"]);
    let before_environment = environment();
    let before_cwd = std::env::current_dir().unwrap();
    let mut env = before_environment.clone();
    env.insert(
        "PATH".into(),
        format!("{}:/usr/bin:/bin", root.join("bin").display()).into(),
    );
    for (key, relative) in [
        ("FIXTURE_INPUT", "retrace.input"),
        ("FIXTURE_MODE", "mode"),
        ("FIXTURE_TRACE", "trace"),
        ("FIXTURE_PID", "helper.pid"),
    ] {
        env.insert(key.into(), root.join(relative).into_os_string());
    }
    let helper_cleanup = RetraceHelpers(root.clone());
    let mut session = Session::new(&root, &env).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let label = format!("{name}: original call {}", row["index"]);
        filesystem_actions(&root, &row["before"]);
        let request = row["request"]
            .as_str()
            .unwrap()
            .replace("$FIXTURE", root.to_str().unwrap());
        let response = session.request(request.as_bytes());
        let actual_snapshot = row["source_files_json"]
            .is_string()
            .then(|| crash_snapshot_lossless(&root));
        filesystem_actions(&root, &row["after"]);
        let actual = response.unwrap_or_else(|error| panic!("{label}: outer source error {error}"));
        let expected = JsonValue::parse(
            &row["original_stdout"]
                .as_str()
                .unwrap()
                .replace("$FIXTURE", root.to_str().unwrap()),
        )
        .unwrap();
        if row["cycle_log_sort_only"] == true {
            same_cycle_record(expected, actual, &label);
        } else {
            same_stateful_record(expected, actual, &label);
        }
        if let Some(actual_snapshot) = actual_snapshot {
            let expected = JsonValue::parse(row["source_files_json"].as_str().unwrap()).unwrap();
            same_json(
                snapshot_modes_lossless(expected, directory_mode, file_mode),
                actual_snapshot,
                &format!("{label}: filesystem"),
            );
        }
        assert!(
            !helper_cleanup.terminate_leaked_identity(),
            "{label}: helper survived source operation"
        );
    }
    // Original EOF/finally stop joins its thread before releasing fixture files.
    // Any native source thread must end before this source owner is dropped too.
    drop(session);
    assert_eq!(std::env::current_dir().unwrap(), before_cwd);
    assert_eq!(environment(), before_environment);
}

#[test]
#[cfg(target_os = "linux")]
fn original_134_text_calls_preserve_seeded_unicode_filtering_and_typed_filename_slices() {
    replay_remaining("stateful-text-original.json", 134);
}

#[test]
#[cfg(target_os = "linux")]
fn original_seven_reporting_loop_calls_preserve_sleep_boundaries_raw_files_and_set_membership() {
    replay_remaining("stateful-reporting-loop-original.json", 7);
}

#[test]
#[cfg(target_os = "linux")]
fn original_seven_disabled_loop_calls_clear_new_crashes_without_sdk_or_report_calls() {
    replay_remaining("stateful-disabled-loop-original.json", 7);
}

#[test]
fn retired_worker_loader_and_checker_are_historical_without_weakening_runtime_pins() {
    let captured = manifest();
    let expected: BTreeMap<String, String> =
        serde_json::from_value(captured["source_sha256"].clone()).unwrap();
    let retired = [
        "rust/tools/check_version_reference.py",
        "rust/tools/tombstoned_reference.py",
        "rust/tools/check_tombstoned_reference.py",
    ];
    let copied = tempfile::tempdir().unwrap();
    for name in expected
        .keys()
        .filter(|name| !retired.contains(&name.as_str()))
    {
        let target = copied.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repository().join(name), target).unwrap();
    }
    let live = source::sources(copied.path()).unwrap();
    let historical = source::historical_sources(copied.path()).unwrap();
    assert_eq!(live.len(), 6);
    assert_eq!(historical.len(), 3);
    let mut complete = live.clone();
    for (name, hash) in &historical {
        assert!(complete.insert(name.clone(), hash.clone()).is_none());
    }
    assert_eq!(complete, expected);
    for name in retired {
        let path = copied.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"changed retired original\n").unwrap();
        assert!(source::sources(copied.path()).is_err());
        assert!(source::historical_sources(copied.path()).is_err());
        fs::remove_file(&path).unwrap();
        symlink("missing-retired-original", &path).unwrap();
        assert!(source::sources(copied.path()).is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(source::sources(copied.path()).is_err());
        fs::remove_dir(&path).unwrap();
        if let Ok(original) = fs::read(repository().join(name)) {
            fs::write(&path, original).unwrap();
            assert_eq!(
                source::sources(copied.path()).unwrap()[name],
                expected[name]
            );
            assert!(!source::historical_sources(copied.path())
                .unwrap()
                .contains_key(name));
            fs::remove_file(&path).unwrap();
        }
    }
    for name in live.keys() {
        let path = copied.path().join(name);
        let original = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            source::sources(copied.path()).is_err(),
            "missing live {name}"
        );
        assert!(source::historical_sources(copied.path()).is_err());
        fs::write(path, original).unwrap();
    }
}
