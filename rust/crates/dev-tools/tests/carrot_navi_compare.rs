//! Hosted-only differential gate. Never execute archived programs: read the
//! sealed JSON members, run the current comparator and retained workflow oracle.
use openpilot_dev_tools::carrot_navi_compare::{assert_exact, normalize};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const CAPTURE_SHA: &str = "6e4f51e5a3ed9bb4d7592669db976eb0f3215c6b";
const ORACLE_SHA: &str = "a6aa33955b981c9308729c89d744d794acf6127140421d10c01ae76147256262";
const HELPER_SHA: &str = "5425bbe8fd48efd1f3bd5e2e71ce5801a3a2497e3a8b5c084a6ca8b71a4f7c95";
const OUTPUTS: [&str; 5] = [
    "socket-source-normalized.json",
    "socket-native-normalized.json",
    "lifecycle-source-normalized.json",
    "lifecycle-native-normalized.json",
    "comparison.json",
];
const INPUTS: [&str; 16] = [
    "policy-source.json",
    "policy-native.json",
    "socket-source/rows.json",
    "socket-native/rows.json",
    "socket-source/result.json",
    "socket-native/result.json",
    "lifecycle-source/rows.json",
    "lifecycle-native/rows.json",
    "lifecycle-source/result.json",
    "lifecycle-native/result.json",
    "startup-source/result.json",
    "startup-native/result.json",
    "exhaust-source/result.json",
    "exhaust-native/result.json",
    "startup-source/discovery.json",
    "startup-native/discovery.json",
];

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn parse(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}

#[test]
fn directional_numeric_and_structural_rules_have_no_tolerance() {
    for (actual, expected, pass) in [
        ("0", "0.0", true),
        ("0", "-0.0", false),
        ("-0.0", "0", true),
        ("-0", "-0.0", false),
        ("-0.0", "0.0", false),
        ("true", "1", false),
        ("1", "true", false),
        ("false", "0.0", false),
        ("1.0000000000000002", "1.0", false),
        ("9007199254740993", "9007199254740992.0", true),
        ("9007199254740992.0", "9007199254740993", false),
        ("9223372036854775808", "9223372036854775809", false),
        ("NaN", "NaN", true),
        ("Infinity", "Infinity", true),
        ("1e400", "Infinity", true),
        ("null", "false", false),
        (r#"{"b":2,"a":1}"#, r#"{"a":1,"b":2}"#, true),
        (r#"{"a":1,"extra":0}"#, r#"{"a":1}"#, false),
        ("[1,2]", "[2,1]", false),
        ("[1]", "[1,1]", false),
        (r#""\ud800""#, r#""\ud800""#, true),
        (r#""\ud800""#, r#""\ud801""#, false),
    ] {
        assert_eq!(
            assert_exact(&parse(actual), &parse(expected), "$").is_ok(),
            pass,
            "{actual} versus {expected}"
        );
    }
    assert!(assert_exact(&parse(&"9".repeat(400)), &parse("Infinity"), "overflow").is_err());
}

#[test]
fn normalization_is_ordered_and_only_projects_inherited_fields() {
    let a = parse(
        r#"[{"before":"aa","session_id":"aa","after":"/aa/","nested":{"cereal_publish_count":77,"port":1234},"received_at_ms":3,"false_clock":{"received_at_ms":false},"zero_clock":{"received_at_ms":0.0},"fixed":{"port":7714}}]"#,
    );
    let e = parse(
        r#"[{"before":"aa","session_id":"<session0>","after":"/<session0>/","nested":{"port":"<bound-port>"},"received_at_ms":"<positive-clock>","false_clock":{"received_at_ms":false},"zero_clock":{"received_at_ms":0.0},"fixed":{"port":7714}}]"#,
    );
    assert_exact(&normalize(&a).unwrap(), &e, "normalized").unwrap();
    // Keys are untouched; lone surrogate tokens use Python codepoint replacement.
    let a = normalize(&parse(r#"{"session_id":"\ud800","\ud800":"x\ud800"}"#)).unwrap();
    assert_exact(
        &a,
        &parse(r#"{"session_id":"<session0>","\ud800":"x<session0>"}"#),
        "surrogate",
    )
    .unwrap();
    for raw in ["-1", "1.0", "\"1\"", "NaN", "Infinity"] {
        assert!(
            normalize(&parse(&format!("{{\"received_at_ms\":{raw}}}"))).is_err(),
            "{raw}"
        );
    }
}

struct Case {
    name: String,
    pass: bool,
    edits: Vec<(&'static str, Option<String>)>,
}
fn case(name: &str, pass: bool, edits: Vec<(&'static str, Option<String>)>) -> Case {
    Case {
        name: name.into(),
        pass,
        edits,
    }
}
fn rows(row: &str, count: usize) -> String {
    format!("[{}]", vec![row; count].join(","))
}
fn row_case(name: &str, source: &str, native: &str, pass: bool) -> Case {
    case(
        name,
        pass,
        vec![
            ("socket-source/rows.json", Some(rows(source, 56))),
            ("socket-native/rows.json", Some(rows(native, 56))),
        ],
    )
}

fn oracle(repo: &Path) -> String {
    let workflow = fs::read_to_string(repo.join(".github/workflows/rust.yml")).unwrap();
    let section = workflow
        .split("      - name: Require exact existing policy and normalized process comparisons\n")
        .nth(1)
        .expect("retained embedded oracle required until this gate is accepted");
    let script = section
        .split("          python -P - <<'PY'\n")
        .nth(1)
        .unwrap()
        .split("          PY\n")
        .next()
        .unwrap();
    let script = script
        .lines()
        .map(|l| l.strip_prefix("          ").unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        hash(script.as_bytes()),
        ORACLE_SHA,
        "original embedded oracle changed"
    );
    assert_eq!(
        hash(&fs::read(repo.join("rust/tools/radarcan_exact.py")).unwrap()),
        HELPER_SHA
    );
    script
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}
fn seed(saved: &Path, parent: &Path, edits: &[(&str, Option<String>)]) -> PathBuf {
    let root = parent.join("carrot-navi");
    for name in INPUTS {
        write(&root.join(name), fs::read(saved.join(name)).unwrap());
    }
    for (name, text) in edits {
        match text {
            Some(text) => write(&root.join(name), text),
            None => fs::remove_file(root.join(name)).unwrap(),
        }
    }
    root
}
fn save_output(parent: &Path, output: &Output) {
    write(&parent.join("stdout.log"), &output.stdout);
    write(&parent.join("stderr.log"), &output.stderr);
    write(
        &parent.join("status.txt"),
        format!("{:?}\n", output.status.code()),
    );
}

#[test]
fn held_captures_and_mutations_match_actual_original_cli_and_file_effects() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let script = oracle(&repo);
    let saved = PathBuf::from(
        std::env::var_os("CARROT_NAVI_SAVED_ROOT")
            .expect("hosted gate supplies sealed saved JSON only"),
    );
    for line in include_str!("fixtures/carrot-navi-comparator/saved-members.sha256").lines() {
        let (sha, name) = line.split_once("  ").unwrap();
        assert_eq!(
            hash(&fs::read(saved.join(name)).unwrap()),
            sha,
            "saved {name}"
        );
    }
    let evidence = PathBuf::from(
        std::env::var_os("CARROT_NAVI_COMPARE_EVIDENCE")
            .expect("hosted gate preserves every control"),
    );
    fs::create_dir(&evidence).expect("evidence directory must be fresh");
    write(&evidence.join("original-embedded.py"), &script);
    write(
        &evidence.join("historical-source.txt"),
        format!("{CAPTURE_SHA}\n"),
    );
    let python =
        std::env::var_os("CARROT_NAVI_ORIGINAL_PYTHON").expect("explicit hosted Python oracle");
    let binary = Path::new(env!("CARGO_BIN_EXE_check-carrot-navi-comparison"));
    write(
        &evidence.join("native-elf.sha256"),
        hash(&fs::read(binary).unwrap()),
    );

    let mut cases = vec![case("held-179-policy-56-socket-21-lifecycle", true, vec![])];
    for (i, (actual, expected, pass)) in [
        ("0", "0.0", true),
        ("0", "-0.0", false),
        ("-0.0", "0", true),
        ("-0", "-0.0", false),
        ("-0.0", "0.0", false),
        ("true", "1", false),
        ("1", "true", false),
        ("false", "0.0", false),
        ("1.0000000000000002", "1.0", false),
        ("9007199254740993", "9007199254740992.0", true),
        ("9007199254740992.0", "9007199254740993", false),
        ("9223372036854775808", "9223372036854775809", false),
        ("NaN", "NaN", true),
        ("Infinity", "Infinity", true),
        ("1e400", "Infinity", true),
        ("null", "false", false),
        (r#"{"b":2,"a":1}"#, r#"{"a":1,"b":2}"#, true),
        (r#"{"a":1,"extra":0}"#, r#"{"a":1}"#, false),
        ("[1,2]", "[2,1]", false),
        ("[1]", "[1,1]", false),
        (r#""\ud800""#, r#""\ud800""#, true),
        (r#""\ud800""#, r#""\ud801""#, false),
        (r#"{"x":1,"x":2}"#, r#"{"x":2}"#, true),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(case(
            &format!("numeric-structure-{i:02}"),
            pass,
            vec![
                ("policy-native.json", Some(format!("[{actual}]"))),
                ("policy-source.json", Some(format!("[{expected}]"))),
            ],
        ));
    }
    cases.push(case(
        "integer-to-float-overflow",
        false,
        vec![
            ("policy-native.json", Some(format!("[{}]", "9".repeat(400)))),
            ("policy-source.json", Some("[Infinity]".into())),
        ],
    ));
    for (i, (source, native, pass)) in [
        (r#"{"session_id":"aa","url":"/aa/","cereal_publish_count":9,"port":4567,"received_at_ms":1}"#, r#"{"session_id":"bb","url":"/bb/","cereal_publish_count":99,"port":4568,"received_at_ms":2}"#, true),
        (r#"{"before":"aa","session_id":"aa"}"#, r#"{"before":"bb","session_id":"bb"}"#, false),
        (r#"{"session_id":"aa","nested":{"session_id":"cc"},"url":"/aa/cc/"}"#, r#"{"session_id":"bb","nested":{"session_id":"dd"},"url":"/bb/dd/"}"#, true),
        (r#"{"session_id":"aa","aa":1}"#, r#"{"session_id":"bb","bb":1}"#, false),
        (r#"{"received_at_ms":0}"#, r#"{"received_at_ms":1}"#, false),
        (r#"{"received_at_ms":null}"#, r#"{"received_at_ms":0}"#, false),
        (r#"{"received_at_ms":false}"#, r#"{"received_at_ms":0}"#, false),
        (r#"{"received_at_ms":true}"#, r#"{"received_at_ms":1}"#, true),
        (r#"{"port":7714}"#, r#"{"port":7714.0}"#, true),
        (r#"{"port":7714}"#, r#"{"port":1234}"#, false),
        (r#"{"port":"7714"}"#, r#"{"port":false}"#, true),
        (r#"{"nested":{"cereal_publish_count":1},"port":[1]}"#, r#"{"nested":{"cereal_publish_count":2},"port":[2]}"#, false),
        (r#"{"received_at_ms":[],"port":{}}"#, r#"{"received_at_ms":[],"port":{}}"#, true),
        (r#"{"session_id":"","other":"x"}"#, r#"{"session_id":"","other":"x"}"#, true),
        (r#"{"session_id":"\ud800","url":"/\ud800/"}"#, r#"{"session_id":"\ud801","url":"/\ud801/"}"#, true),
        (r#"{"cereal_publish_count":{},"other":"cereal_publish_count"}"#, r#"{"other":"cereal_publish_count"}"#, true),
        (r#"{"received_mono_ns":1}"#, r#"{"received_mono_ns":2}"#, false),
        (r#"{"session_id":1}"#, r#"{"session_id":2}"#, false),
    ].into_iter().enumerate() {
        cases.push(row_case(&format!("normalization-{i:02}"), source, native, pass));
    }
    for (i, raw) in ["-1", "1.0", "\"1\"", "NaN", "Infinity"]
        .into_iter()
        .enumerate()
    {
        for key in [
            "last_received_at_ms",
            "received_at_ms",
            "last_cereal_publish_mono_ns",
        ] {
            let row = format!("{{\"{key}\":{raw}}}");
            cases.push(row_case(
                &format!("invalid-clock-{key}-{i}"),
                &row,
                &row,
                false,
            ));
        }
    }
    for (path, count) in [
        ("socket-native/rows.json", 55),
        ("socket-source/rows.json", 57),
        ("lifecycle-native/rows.json", 20),
        ("lifecycle-source/rows.json", 22),
    ] {
        cases.push(case(
            &format!("cardinality-{}", path.replace('/', "-")),
            false,
            vec![(path, Some(rows("{}", count)))],
        ));
    }
    for path in [
        "socket-native/result.json",
        "lifecycle-native/result.json",
        "startup-native/result.json",
        "exhaust-native/result.json",
    ] {
        cases.push(case(
            &format!("result-{}", path.replace('/', "-")),
            false,
            vec![(path, Some("{}".into()))],
        ));
    }
    cases.push(case(
        "discovery-wire-change",
        false,
        vec![(
            "startup-native/discovery.json",
            Some(r#"[{"wire_hex":"00"}]"#.into()),
        )],
    ));
    cases.push(case(
        "discovery-metadata-is-not-projected-into-wire",
        true,
        vec![
            (
                "startup-source/discovery.json",
                Some(r#"[{"wire_hex":"abcd","source_ip":"old","received_ns":10}]"#.into()),
            ),
            (
                "startup-native/discovery.json",
                Some(r#"[{"wire_hex":"abcd","source_ip":"new","received_ns":20}]"#.into()),
            ),
        ],
    ));
    cases.push(case(
        "discovery-missing-wire",
        false,
        vec![("startup-native/discovery.json", Some("[{}]".into()))],
    ));
    cases.push(case(
        "missing-input",
        false,
        vec![("policy-native.json", None)],
    ));
    cases.push(case(
        "malformed-input",
        false,
        vec![("policy-native.json", Some("{".into()))],
    ));
    cases.push(case(
        "utf8-text-bom-rejected",
        false,
        vec![("policy-native.json", Some("\u{feff}[]".into()))],
    ));

    let mut receipts = Vec::new();
    for case in &cases {
        let destination = evidence.join(&case.name);
        let source_parent = destination.join("source");
        let native_parent = destination.join("native");
        let source_root = seed(&saved, &source_parent, &case.edits);
        let native_root = seed(&saved, &native_parent, &case.edits);
        let source = Command::new(&python)
            .args(["-P", "-c", &script])
            .env("RUNNER_TEMP", &source_parent)
            .env("PYTHONPATH", repo.join("rust/tools"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .current_dir(&repo)
            .output()
            .unwrap();
        save_output(&source_parent, &source);
        let trace = native_parent.join("exec.trace");
        let native = Command::new("strace")
            .args([
                "-f",
                "-qq",
                "-s",
                "4096",
                "-e",
                "trace=execve,execveat",
                "-o",
            ])
            .arg(&trace)
            .arg("--")
            .arg(binary)
            .arg(&native_root)
            .output()
            .unwrap();
        save_output(&native_parent, &native);
        let trace = fs::read_to_string(trace).unwrap();
        let execs = trace.lines().collect::<Vec<_>>();
        assert_eq!(
            execs.len(),
            1,
            "{} native comparator must have one closed exec: {trace}",
            case.name
        );
        assert!(
            execs[0].contains(&format!("execve(\"{}\"", binary.display()))
                && execs[0].ends_with("= 0"),
            "unrecognized executable: {trace}"
        );
        assert_eq!(
            source.status.code(),
            Some(if case.pass { 0 } else { 1 }),
            "{} original: {}",
            case.name,
            String::from_utf8_lossy(&source.stderr)
        );
        assert_eq!(
            native.status.code(),
            source.status.code(),
            "{} native: {}",
            case.name,
            String::from_utf8_lossy(&native.stderr)
        );
        assert!(source.stdout.is_empty() && native.stdout.is_empty());
        let mut outputs = serde_json::Map::new();
        for name in OUTPUTS {
            let a = fs::read(native_root.join(name)).ok();
            let e = fs::read(source_root.join(name)).ok();
            assert_eq!(a, e, "{} output bytes/existence {name}", case.name);
            if let Some(bytes) = a {
                if case.edits.is_empty() {
                    assert_eq!(
                        bytes,
                        fs::read(saved.join(name)).unwrap(),
                        "historical {name}"
                    );
                }
                outputs.insert(name.into(), hash(&bytes).into());
            }
        }
        receipts.push(serde_json::json!({"name":case.name,"expected_pass":case.pass,"source_status":source.status.code(),"native_status":native.status.code(),"native_execs":1,"output_sha256":outputs}));
    }
    write(&evidence.join("controls.json"), serde_json::to_vec_pretty(&serde_json::json!({"historical_source":CAPTURE_SHA,"oracle_sha256":ORACLE_SHA,"cases":receipts})).unwrap());
    // CLI arity failures must not read or mutate the supplied input tree.
    for args in [vec![], vec![saved.as_os_str(), saved.as_os_str()]] {
        let result = Command::new(binary).args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
    }
    // No edits or stale generated receipts have been written into held inputs.
    for line in include_str!("fixtures/carrot-navi-comparator/saved-members.sha256").lines() {
        let (sha, name) = line.split_once("  ").unwrap();
        assert_eq!(hash(&fs::read(saved.join(name)).unwrap()), sha);
    }
}
