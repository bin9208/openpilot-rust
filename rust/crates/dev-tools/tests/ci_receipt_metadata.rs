use openpilot_dev_tools::ci_policy::uses_native_action;
use openpilot_dev_tools::ci_receipt_metadata::{
    first_binding, first_binding_from_names, module, prefix, read_receipt, MAX_RECEIPT_BYTES,
};
use std::{
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
};

#[test]
fn json_fields_preserve_duplicate_keys_unicode_and_stdout_codec() {
    assert_eq!(
        module(br#"{"module":"first","module":"last","ignored":NaN}"#, "X").unwrap(),
        b"X=last\n"
    );
    assert_eq!(
        prefix("{\"prefix\":\"경로/😀\"}".as_bytes()).unwrap(),
        "경로/😀\n".as_bytes()
    );
    assert_eq!(
        module(br#"{"module":"raw-\udcff"}"#, "X").unwrap(),
        b"X=raw-\xff\n"
    );
    assert_eq!(
        prefix(br#"{"prefix":"\udcc3\udca9"}"#).unwrap(),
        b"\xc3\xa9\n"
    );
    assert_eq!(
        module(br#"{"module":"\ud800"}"#, "X").unwrap_err().class,
        "UnicodeEncodeError"
    );
    for (data, class) in [
        (b"{".as_slice(), "JSONDecodeError"),
        (b"\xff", "UnicodeDecodeError"),
        (b"{}", "KeyError"),
        (b"[]", "TypeError"),
        (br#"{"module":2}"#, "TypeError"),
    ] {
        assert_eq!(module(data, "X").unwrap_err().class, class);
    }
    assert_eq!(
        prefix(br#"{"prefix":null}"#).unwrap_err().class,
        "UnsupportedReceiptType"
    );
    assert_eq!(
        module(br#"{"module":"a"}"#, "bad-label").unwrap_err().class,
        "ArgumentError"
    );
}
#[test]
fn bounded_receipt_read_follows_links_and_rejects_sparse_oversize_before_json() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("receipt");
    let link = root.path().join("alias");
    fs::write(&target, br#"{"prefix":"ok"}"#).unwrap();
    symlink("receipt", &link).unwrap();
    assert_eq!(prefix(&read_receipt(&link).unwrap()).unwrap(), b"ok\n");
    // No large write/allocation: invalid JSON plus a sparse tail must be rejected
    // by admission before the JSON decoder can report JSONDecodeError.
    fs::File::options()
        .write(true)
        .open(&target)
        .unwrap()
        .set_len(MAX_RECEIPT_BYTES as u64 + 1)
        .unwrap();
    for path in [&target, &link] {
        assert_eq!(read_receipt(path).unwrap_err().class, "ReceiptTooLarge");
    }
    assert_eq!(
        fs::metadata(&target).unwrap().len(),
        MAX_RECEIPT_BYTES as u64 + 1
    );
    assert_eq!(
        fs::read_link(&link).unwrap(),
        std::path::Path::new("receipt")
    );
    assert_eq!(
        read_receipt(&root.path().join("missing"))
            .unwrap_err()
            .class,
        "FileNotFoundError"
    );
    assert_eq!(
        read_receipt(root.path()).unwrap_err().class,
        "IsADirectoryError"
    );
}
#[test]
fn ordered_selector_keeps_each_provider_order_instead_of_sorting() {
    let directory = std::path::Path::new("/held/inputs");
    for (names, expected) in [
        (
            ["params_pyx.z.so", "params_pyx.a.so"],
            b"X=/held/inputs/params_pyx.z.so\n".as_slice(),
        ),
        (
            ["params_pyx.a.so", "params_pyx.z.so"],
            b"X=/held/inputs/params_pyx.a.so\n".as_slice(),
        ),
    ] {
        let names: Vec<OsString> = names.into_iter().map(Into::into).collect();
        assert_eq!(
            first_binding_from_names(directory, "X", &names).unwrap(),
            expected
        );
    }
    let names = vec![
        OsString::from(".params_pyx.hidden.so"),
        OsString::from_vec(b"params_pyx.\xff.so".to_vec()),
        "params_pyx.a.so".into(),
    ];
    assert_eq!(
        first_binding_from_names(directory, "X", &names).unwrap(),
        b"X=/held/inputs/params_pyx.\xff.so\n"
    );
    assert_eq!(
        first_binding_from_names(directory, "X", &[])
            .unwrap_err()
            .class,
        "StopIteration"
    );
}
#[test]
fn glob_is_unsorted_and_keeps_raw_names_and_nonregular_matches() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        first_binding(root.path(), "X").unwrap_err().class,
        "StopIteration"
    );
    assert_eq!(
        first_binding(&root.path().join("missing"), "X")
            .unwrap_err()
            .class,
        "StopIteration"
    );
    fs::write(root.path().join("ordinary"), b"").unwrap();
    assert_eq!(
        first_binding(&root.path().join("ordinary"), "X")
            .unwrap_err()
            .class,
        "StopIteration"
    );
    let raw = root
        .path()
        .join(std::ffi::OsString::from_vec(b"params_pyx.\xff.so".to_vec()));
    symlink("absent", &raw).unwrap();
    let mut expected = b"X=".to_vec();
    expected.extend_from_slice(raw.as_os_str().as_bytes());
    expected.push(b'\n');
    assert_eq!(first_binding(root.path(), "X").unwrap(), expected);
    fs::remove_file(raw).unwrap();
    fs::create_dir(root.path().join("params_pyx.directory.so")).unwrap();
    fs::write(root.path().join("params_pyx.last.so"), b"").unwrap();
    let actual_first = fs::read_dir(root.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|e| e.file_name().as_bytes().starts_with(b"params_pyx"))
        .unwrap();
    let mut expected = b"X=".to_vec();
    expected.extend_from_slice(actual_first.path().as_os_str().as_bytes());
    expected.push(b'\n');
    assert_eq!(
        first_binding(&root.path().join("."), "X").unwrap(),
        expected
    );
}

use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn decode(v: &Value) -> Vec<u8> {
    STANDARD.decode(v.as_str().unwrap()).unwrap()
}
fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
/// Original capture's optional summary omitted qualified exception names.
/// Derive the class from the sealed final traceback line, not a native result
/// or a guessed fallback. Reject malformed/unrecognized exception summaries.
fn original_error_class(stderr: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(stderr).ok()?;
    let line = text.lines().rev().find(|line| !line.is_empty())?;
    let qualified = line.split_once(':').map_or(line, |(name, _)| name);
    if !qualified
        .split('.')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_'))
    {
        return None;
    }
    let class = qualified.rsplit('.').next()?;
    (class.ends_with("Error") || class == "StopIteration").then_some(class)
}
#[test]
fn maintained_callers_build_native_metadata_before_all_thirteen_bridge_uses() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let action = fs::read_to_string(root.join(".github/actions/native-json11/action.yml")).unwrap();
    assert!(action.contains("--features ci-receipt-metadata-native --bin stage-json11 --bin ci-receipt-metadata --target \"$native_host\""));
    assert!(action.contains("CI_RECEIPT_METADATA=%s/rust/target/%s/debug/ci-receipt-metadata"));
    assert!(action.contains("env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_TARGET"));
    let original =
        root.join("rust/crates/dev-tools/tests/fixtures/ci-receipt-original/original/sources.json");
    let rows: Value = serde_json::from_slice(&fs::read(original).unwrap()).unwrap();
    let mut uses = 0;
    for relative in [
        ".github/workflows/rust.yml",
        "rust/crates/carrot-man/ci-jobs.yml",
    ] {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        for row in rows
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["path"] == relative)
        {
            assert!(
                !source.contains(row["snippet"].as_str().unwrap()),
                "old interpreter caller remains: {}",
                row["id"]
            );
        }
        let parsed = openpilot_dev_tools::ci_policy::parse(&source).unwrap();
        let jobs = if relative.ends_with("rust.yml") {
            &parsed["jobs"]
        } else {
            &parsed
        };
        for job in jobs.as_object().unwrap().values() {
            let Some(steps) = job["steps"].as_array() else {
                continue;
            };
            let mut built = false;
            for step in steps {
                if uses_native_action(job, step, "native-json11") {
                    built = true;
                }
                let Some(run) = step["run"].as_str() else {
                    continue;
                };
                for line in run
                    .lines()
                    .filter(|line| line.contains("$CI_RECEIPT_METADATA"))
                {
                    assert!(
                        built,
                        "native receipt consumer precedes its host producer: {relative}: {line}"
                    );
                    assert!(!line.contains("python"));
                    uses += 1;
                }
            }
        }
    }
    // The Rust logging controller owns its metadata; thirteen bridge consumers remain.
    // The fourteen historical captures below stay unchanged as independent data.
    assert_eq!(uses, 13);
}
#[test]
fn qualified_original_traceback_class_is_recovered_without_an_optional_summary() {
    // Actual source receipts015/027/037/049 use json.decoder.JSONDecodeError;
    // their capture error_class field is null, but stderr remains sealed.
    assert_eq!(original_error_class(b"Traceback (most recent call last):\n  File \"<string>\", line 1, in <module>\njson.decoder.JSONDecodeError: Expecting property name enclosed in double quotes: line 1 column 2 (char 1)\n"), Some("JSONDecodeError"));
    assert_eq!(
        original_error_class(b"StopIteration\n"),
        Some("StopIteration")
    );
    assert_eq!(
        original_error_class(b"TypeError: bad field\n"),
        Some("TypeError")
    );
    assert_eq!(original_error_class(b"Traceback only\n"), None);
    assert_eq!(
        original_error_class(b"JSONDecodeError: bad\ntruncated exception"),
        None
    );
    assert_eq!(original_error_class(b".JSONDecodeError: bad"), None);
    assert_eq!(original_error_class(b"\xff"), None);
}
fn replace_root(bytes: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    assert!(!old.is_empty());
    let mut out = Vec::new();
    let mut rest = bytes;
    while let Some(at) = rest.windows(old.len()).position(|w| w == old) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(new);
        rest = &rest[at + old.len()..];
    }
    out.extend_from_slice(rest);
    out
}
fn directory_order(directory: &Path) -> Vec<OsString> {
    fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect()
}
fn first_matching_name(names: &[OsString]) -> Option<&std::ffi::OsStr> {
    names
        .iter()
        .find(|name| {
            name.as_bytes().starts_with(b"params_pyx") && name.as_bytes().ends_with(b".so")
        })
        .map(OsString::as_os_str)
}
/// Only the first basename is provider-dependent. The captured ENV, directory
/// bytes, separators and newline must remain exact, including raw Unix bytes.
fn project_first_basename(
    original: &[u8],
    directory: &Path,
    env: &str,
    old: &std::ffi::OsStr,
    new: &std::ffi::OsStr,
) -> Vec<u8> {
    let mut prefix = format!("{env}=").into_bytes();
    prefix.extend_from_slice(directory.as_os_str().as_bytes());
    prefix.push(b'/');
    let mut original_contract = prefix.clone();
    original_contract.extend_from_slice(old.as_bytes());
    original_contract.push(b'\n');
    assert_eq!(
        original, original_contract,
        "projection requires exact original path/output contract"
    );
    prefix.extend_from_slice(new.as_bytes());
    prefix.push(b'\n');
    prefix
}
#[test]
fn provider_projection_preserves_raw_parent_bytes_env_and_newline() {
    let dir = PathBuf::from(OsString::from_vec(b"/root/runner-\xff/inputs".to_vec()));
    let old = std::ffi::OsStr::new("params_pyx.a.so");
    let new = std::ffi::OsStr::new("params_pyx.z.so");
    assert_eq!(
        project_first_basename(
            b"X=/root/runner-\xff/inputs/params_pyx.a.so\n",
            &dir,
            "X",
            old,
            new
        ),
        b"X=/root/runner-\xff/inputs/params_pyx.z.so\n"
    );
    assert!(std::panic::catch_unwind(|| project_first_basename(
        b"WRONG=/root/runner-\xff/inputs/params_pyx.a.so\n",
        &dir,
        "X",
        old,
        new
    ))
    .is_err());
}
fn file_set(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            assert!(!entry.file_type().unwrap().is_symlink());
            if p.is_dir() {
                walk(root, &p, out)
            } else {
                assert!(entry.file_type().unwrap().is_file());
                out.insert(p.strip_prefix(root).unwrap().to_str().unwrap().into());
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}
/// Consumes independently captured actual snippets, never the native output as
/// expected. Exact stdout/status, original failure class and no effects are
/// checked; first-glob basename alone may project to the independently measured
/// current provider order. The source-fixed selector is also compared exactly.
/// Python traceback wording/locations are intentionally not identical.
#[test]
#[ignore = "hosted sealed original-only capture and registered CLI required"]
fn all_fourteen_callers_and_original_failures() {
    let original = required("CI_RECEIPT_ORIGINAL");
    let binary = required("CI_RECEIPT_BINARY");
    let evidence = required("CI_RECEIPT_EVIDENCE");
    fs::create_dir(&evidence).unwrap();
    let manifest_bytes = fs::read(original.join("manifest.json")).unwrap();
    let manifest_sha = std::env::var("CI_RECEIPT_MANIFEST_SHA256").unwrap();
    assert_eq!(sha(&manifest_bytes), manifest_sha);
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["native_expected_used"], false);
    assert_eq!(manifest["callers"], 14);
    assert_eq!(manifest["observations"], 67);
    assert_eq!(manifest["source_before"], manifest["source_after"]);
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(files.len(), 68);
    let mut expected: BTreeSet<_> = files.keys().cloned().collect();
    expected.insert("manifest.json".into());
    assert_eq!(file_set(&original), expected);
    let verify = || {
        assert_eq!(
            fs::read(original.join("manifest.json")).unwrap(),
            manifest_bytes
        );
        for (name, pin) in files {
            assert!(!Path::new(name).is_absolute());
            assert!(Path::new(name)
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_))));
            let data = fs::read(original.join(name)).unwrap();
            assert_eq!(data.len() as u64, pin["bytes"].as_u64().unwrap());
            assert_eq!(sha(&data), pin["sha256"].as_str().unwrap());
        }
        assert_eq!(file_set(&original), expected);
    };
    verify();
    let sources: Value =
        serde_json::from_slice(&fs::read(original.join("sources.json")).unwrap()).unwrap();
    let sources = sources.as_array().unwrap();
    assert_eq!(sources.len(), 14);
    for source in sources {
        let path = source["path"].as_str().unwrap();
        assert_eq!(source["source_sha256"], manifest["source_before"][path]);
        assert_eq!(
            sha(source["snippet"].as_str().unwrap().as_bytes()),
            source["snippet_sha256"]
        );
    }
    let executable = fs::read(&binary).unwrap();
    let binary_sha = sha(&executable);
    let mut seen = BTreeSet::new();
    let mut receipts = Vec::new();
    for i in 0..67 {
        let row: Value = serde_json::from_slice(
            &fs::read(original.join(format!("receipts/{i:03}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(row["unchanged"], true);
        assert_eq!(row["before"], row["after"]);
        let source = sources.iter().find(|s| s["id"] == row["caller"]).unwrap();
        assert_eq!(source["op"], row["op"]);
        assert_eq!(source["env"], row["env"]);
        if i < 14 {
            assert!(seen.insert(row["caller"].as_str().unwrap().to_owned()));
        }
        let temp = tempfile::tempdir().unwrap();
        let setup = &row["setup"];
        let runner = temp.path().join(if setup["raw_root"] == true {
            OsString::from_vec(b"runner-\xff".to_vec())
        } else {
            "runner".into()
        });
        fs::create_dir(&runner).unwrap();
        let rel = setup["input_relative"].as_str().unwrap();
        assert_eq!(source["input_relative"], rel);
        assert!(Path::new(rel)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_))));
        let target = runner.join(rel);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let op = row["op"].as_str().unwrap();
        let kind = setup["kind"].as_str().unwrap();
        if op == "first-binding" && kind == "directory" {
            fs::create_dir(&target).unwrap();
            for entry in setup["entries"].as_array().unwrap() {
                let name = OsString::from_vec(decode(&entry["name_b64"]));
                assert_eq!(Path::new(&name).components().count(), 1);
                let p = target.join(name);
                match entry["kind"].as_str().unwrap() {
                    "link" => symlink("missing-target", p).unwrap(),
                    "directory" => fs::create_dir(p).unwrap(),
                    "file" => fs::write(p, b"input-only").unwrap(),
                    _ => panic!("unknown entry kind"),
                }
            }
        } else if kind == "directory" {
            fs::create_dir(&target).unwrap();
        } else if kind == "file" {
            fs::write(
                &target,
                if op == "first-binding" {
                    b"not-directory".to_vec()
                } else {
                    decode(&setup["payload_b64"])
                },
            )
            .unwrap();
        } else {
            assert_eq!(kind, "missing");
        }
        let captured_stdout = replace_root(
            &decode(&row["stdout_b64"]),
            &decode(&row["capture_runner_b64"]),
            runner.as_os_str().as_bytes(),
        );
        let mut expected_stdout = captured_stdout.clone();
        let mut enumeration_projection = Value::Null;
        let mut current_order = None;
        if op == "first-binding" && kind == "directory" {
            let historical: Vec<OsString> = row["glob_order_b64"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| OsString::from_vec(decode(v)))
                .collect();
            let measured = directory_order(&target);
            let set = |names: &[OsString]| {
                names
                    .iter()
                    .map(|n| n.as_bytes().to_vec())
                    .collect::<BTreeSet<_>>()
            };
            assert_eq!(
                set(&historical).len(),
                historical.len(),
                "historical duplicate names"
            );
            assert_eq!(
                set(&measured),
                set(&historical),
                "provider order may differ, member roster must not"
            );
            enumeration_projection = json!({"scope":"filesystem-provider enumeration input; no selected basename change", "applied":false,
                "original_order_b64":historical.iter().map(|n|STANDARD.encode(n.as_bytes())).collect::<Vec<_>>(),
                "current_order_b64":measured.iter().map(|n|STANDARD.encode(n.as_bytes())).collect::<Vec<_>>()});
            // Fixed original output is tested using its original raw enumeration.
            // The real CLI below separately uses the actual current filesystem.
            let replay =
                first_binding_from_names(&target, row["env"].as_str().unwrap(), &historical);
            if row["status"] == 0 {
                assert_eq!(
                    replay.unwrap(),
                    captured_stdout,
                    "source-fixed ordered selector for {i}"
                );
                let original_first = first_matching_name(&historical).unwrap();
                let current_first = first_matching_name(&measured).unwrap();
                expected_stdout = project_first_basename(
                    &captured_stdout,
                    &target,
                    row["env"].as_str().unwrap(),
                    original_first,
                    current_first,
                );
                enumeration_projection = json!({"scope":"filesystem-provider order; only selected first basename may change", "applied":original_first != current_first,
                    "original_order_b64":historical.iter().map(|n|STANDARD.encode(n.as_bytes())).collect::<Vec<_>>(),
                    "current_order_b64":measured.iter().map(|n|STANDARD.encode(n.as_bytes())).collect::<Vec<_>>(),
                    "original_first_b64":STANDARD.encode(original_first.as_bytes()), "current_first_b64":STANDARD.encode(current_first.as_bytes()),
                    "original_stdout_b64":STANDARD.encode(&captured_stdout),"expected_projected_stdout_b64":STANDARD.encode(&expected_stdout)});
            } else {
                assert_eq!(
                    replay.unwrap_err().class,
                    original_error_class(&decode(&row["stderr_b64"])).unwrap()
                );
                assert!(first_matching_name(&measured).is_none());
            }
            current_order = Some(measured);
        }
        let before = snapshot(temp.path());
        let mut argv = vec![binary.clone().into_os_string(), op.into()];
        if op != "prefix" {
            argv.push(row["env"].as_str().unwrap().into());
        }
        argv.push(target.as_os_str().to_owned());
        let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(10));
        spec.cwd = Some(temp.path().into());
        spec.env.insert("PATH".into(), "/nonexistent".into());
        spec.stdout_limit = 1024 * 1024;
        spec.stderr_limit = 1024 * 1024;
        let actual = NativeRunner.run(&spec).unwrap();
        // Retain actual bytes before any parity assertion can fail.
        fs::write(evidence.join(format!("case-{i:03}.json")), serde_json::to_vec_pretty(&json!({"case":i,"argv_b64":argv.iter().map(|s|STANDARD.encode(s.as_bytes())).collect::<Vec<_>>(),"status":actual.code,"stdout_b64":STANDARD.encode(&actual.stdout),"stderr_b64":STANDARD.encode(&actual.stderr),"enumeration_projection":enumeration_projection})).unwrap()).unwrap();
        assert_eq!(snapshot(temp.path()), before, "effects for {i}");
        if let Some(order) = &current_order {
            assert_eq!(
                &directory_order(&target),
                order,
                "directory enumeration changed during native command {i}"
            );
        }
        let boundary = row["boundary_nonstring_prefix"] == true;
        if boundary {
            assert_eq!(
                row["status"], 0,
                "original print(non-string) succeeds outside admitted stage schema"
            );
            assert!(actual.code != 0);
            assert!(actual.stdout.is_empty());
            assert!(String::from_utf8_lossy(&actual.stderr).contains("UnsupportedReceiptType"));
        } else {
            assert_eq!(
                actual.code,
                row["status"].as_i64().unwrap() as i32,
                "status for {i}"
            );

            assert_eq!(actual.stdout, expected_stdout, "stdout for {i}");
            if actual.code == 0 {
                assert!(actual.stderr.is_empty());
            } else {
                assert!(actual.stdout.is_empty());
                let original_stderr = decode(&row["stderr_b64"]);
                let class = original_error_class(&original_stderr).unwrap_or_else(|| {
                    panic!("sealed original final exception is invalid for {i}")
                });
                if let Some(summary) = row["error_class"].as_str() {
                    assert_eq!(summary, class, "original exception summary for {i}");
                } else {
                    assert!(
                        row["error_class"].is_null(),
                        "invalid original exception summary for {i}"
                    );
                }
                assert!(
                    String::from_utf8_lossy(&actual.stderr).starts_with(&format!("{class}:")),
                    "error category for {i}"
                );
            }
        }
        assert!(!actual.stdout.windows(4).any(|w| w == b"PASS"));
        receipts.push(json!({"case":i,"argv_b64":argv.iter().map(|s|STANDARD.encode(s.as_bytes())).collect::<Vec<_>>(),"status":actual.code,"stdout_b64":STANDARD.encode(&actual.stdout),"stderr_b64":STANDARD.encode(&actual.stderr),"explicit_nonstring_boundary":boundary,"enumeration_projection":enumeration_projection}));
    }
    assert_eq!(seen.len(), 14);
    verify();
    assert_eq!(sha(&fs::read(binary).unwrap()), binary_sha);
    fs::write(
        evidence.join("native-receipts.json"),
        serde_json::to_vec_pretty(
            &json!({"manifest_sha256":manifest_sha,"binary_sha256":binary_sha,"cases":receipts}),
        )
        .unwrap(),
    )
    .unwrap();
}
fn snapshot(root: &Path) -> Vec<(Vec<u8>, String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(Vec<u8>, String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let p = entry.path();
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec();
            let typ = entry.file_type().unwrap();
            if typ.is_symlink() {
                out.push((
                    rel,
                    "link".into(),
                    fs::read_link(p).unwrap().as_os_str().as_bytes().to_vec(),
                ));
            } else if typ.is_dir() {
                out.push((rel, "dir".into(), Vec::new()));
                walk(root, &p, out);
            } else {
                out.push((rel, "file".into(), fs::read(p).unwrap()));
            }
        }
    }
    let mut rows = Vec::new();
    walk(root, root, &mut rows);
    rows.sort();
    rows
}
