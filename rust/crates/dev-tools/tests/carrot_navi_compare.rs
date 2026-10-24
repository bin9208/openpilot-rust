//! Hosted-only differential gate. Never execute archived programs: read the
//! sealed JSON members, run the current comparator and frozen independent oracle.
use openpilot_dev_tools::carrot_navi_compare::{assert_exact, normalize};
use openpilot_dev_tools::ci_policy::{
    parse as parse_workflow, uses_native_action, validate_local_routing, TRUSTED_ALWAYS,
    TRUSTED_SOURCE,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
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
    // Frozen byte-for-byte from the accepted pre-migration embedded program.
    // The oracle is test data, never a maintained workflow entrypoint.
    let script = include_str!("fixtures/carrot-navi-comparator/original-embedded.py").to_owned();
    assert_eq!(
        hash(script.as_bytes()),
        ORACLE_SHA,
        "frozen original oracle changed"
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

const RETAINED_JOB_SHA: &str = "fe4b08ff2bab5d8d199c827c6ed13ac392576ed710544a0adef1e6f6160e34cf";
const BUILD_SHA: &str = "aca5431dabde67b28fa11ce90fc3e71d7f6c0c3c5794025ebb947075deae7e51";
const CALLER_SHA: &str = "0e1d027a4ca577b885be3551757efe560ecfb31e2f559eb886ca07ca538a9bd2";
const ACCEPTED_CONTROLS_SHA: &str =
    "815af34faaf6cf9f2cd02a8934d7c328f8fe7ee16716ed8bea16c7378abdc180";

// 역사 원문 pin은 유지한다. 직렬화 형식과 검토된 private infrastructure만 구분한다.
const RETAINED_JOB: &str = include_str!("fixtures/carrot-navi-comparator/retained-runtime-job.yml");
// a4b1db51c의 추가 11단계 전체 metadata/run. 도메인 단계의 hash를 대신하지 않는다.
const PRIVATE_OVERLAY_SHA: &str =
    "80845db9512f58300af32f597109177631b208cad7384c4628c647ae69668534";
const PRIVATE_STAGES: &[(usize, &str)] = &[
    (0, "Deny fork source and require GitHub-hosted Linux X64"),
    (1, "Common owned job temporary paths"),
    (
        2,
        "Allocate private source cache target and local action alias before checkout",
    ),
    (4, "Admit exact private source and index"),
    (
        5,
        "Configure source-dependent job environment on private paths",
    ),
    (6, "Expose provisioned Rust shims"),
    (8, "Keep original Python installations private"),
    (21, "Seal private source and index after maintained checks"),
    (22, "Retain private job ownership and source evidence"),
    (23, "Clean only uploaded owned source alias and target"),
    (
        24,
        "Clean common owned temporary paths after uploaded source",
    ),
];
fn ordered_model(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            let mut entries = fields.iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k.clone(), ordered_model(v)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(ordered_model).collect()),
        _ => value.clone(),
    }
}
fn maintained_steps(workflow: &str) -> Result<(String, String), String> {
    if RETAINED_JOB.len() != 7333 || hash(RETAINED_JOB.as_bytes()) != RETAINED_JOB_SHA {
        return Err("historical retained job fixture changed".into());
    }
    let historical = parse_workflow(RETAINED_JOB)?;
    let expected = &historical["carrot-navi-runtime"];
    let parsed = parse_workflow(workflow)?;
    let job = &parsed["jobs"]["carrot-navi-runtime"];
    validate_local_routing(&json!({"jobs":{"carrot-navi-runtime":job}}))?;
    let steps = job["steps"].as_array().ok_or("Carrot steps missing")?;
    if steps.len() != 25 {
        return Err("Carrot step roster changed".into());
    }
    let mut overlay = Vec::new();
    for &(index, name) in PRIVATE_STAGES {
        if steps[index]["name"] != name {
            return Err(format!("private Carrot stage missing or reordered: {name}"));
        }
        overlay.push(steps[index].clone());
    }
    let overlay = serde_json::to_vec(&ordered_model(&Value::Array(overlay)))
        .map_err(|error| error.to_string())?;
    if hash(&overlay) != PRIVATE_OVERLAY_SHA {
        return Err("unreviewed Carrot private infrastructure change".into());
    }
    let mut scripts = Vec::new();
    for (index, name, expected_hash) in [
        (18, "Build current native Carrot Navi comparator", BUILD_SHA),
        (
            19,
            "Require exact existing policy and normalized process comparisons",
            CALLER_SHA,
        ),
    ] {
        if steps.iter().filter(|s| s["name"] == name).count() != 1 {
            return Err(format!("expected one exact {name} step"));
        }
        let step = &steps[index];
        let script = step["run"].as_str().ok_or("comparator run missing")?;
        if hash(script.as_bytes()) != expected_hash
            || *step
                != json!({"name":name,"run":script,"if":TRUSTED_SOURCE,"working-directory":"${{ env.CI296_SOURCE }}"})
        {
            return Err(format!("{name} command changed"));
        }
        scripts.push(script.to_owned());
    }
    let mut retained = job.clone();
    let environment = retained["env"]
        .as_object_mut()
        .ok_or("Carrot env missing")?;
    if environment.remove("CI296_ALIAS_BASENAME")
        != Some(json!(".ci296-source-rust--carrot-navi-runtime"))
    {
        return Err("Carrot source alias changed".into());
    }
    for key in ["PYTHONPATH", "CARROT_NAVI_BIN"] {
        if environment
            .insert(key.into(), expected["env"][key].clone())
            .is_some()
        {
            return Err(format!("Carrot private environment shadowed: {key}"));
        }
    }
    let mut retained_steps = steps
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            !matches!(*index, 18..=19)
                && !PRIVATE_STAGES.iter().any(|(position, _)| position == index)
        })
        .map(|(_, step)| step.clone())
        .collect::<Vec<_>>();
    let originals = expected["steps"]
        .as_array()
        .ok_or("historical steps missing")?;
    if retained_steps.len() != originals.len() {
        return Err("retained Carrot traffic roster changed".into());
    }
    for (step, original) in retained_steps.iter_mut().zip(originals) {
        let upload = original["uses"] == "actions/upload-artifact@v6";
        if step["if"]
            != if upload {
                TRUSTED_ALWAYS
            } else {
                TRUSTED_SOURCE
            }
        {
            return Err("retained Carrot trust condition changed".into());
        }
        if upload {
            step["if"] = "always()".into();
        } else {
            step.as_object_mut()
                .ok_or("Carrot step is not a map")?
                .remove("if");
        }
        if original.get("run").is_some() {
            if step["working-directory"] != "${{ env.CI296_SOURCE }}" {
                return Err("retained Carrot run escaped private source".into());
            }
            step.as_object_mut()
                .ok_or("Carrot step is not a map")?
                .remove("working-directory");
        }
        match original["uses"].as_str() {
            Some("actions/checkout@v7") => {
                let with = step["with"]
                    .as_object_mut()
                    .ok_or("Carrot checkout options missing")?;
                for (key, value) in [
                    ("set-safe-directory", "false"),
                    ("clean", "false"),
                    ("lfs", "false"),
                    ("path", "${{ env.CI296_CHECKOUT_PATH }}"),
                ] {
                    if with.remove(key) != Some(json!(value)) {
                        return Err(format!("Carrot private checkout option changed: {key}"));
                    }
                }
            }
            Some("./.github/actions/native-json11") => {
                if !uses_native_action(job, step, "native-json11") {
                    return Err("Carrot dependency action escaped current source".into());
                }
                step["uses"] = original["uses"].clone();
            }
            Some("actions/upload-artifact@v6") => {
                let path = concat!(
                    "${{ env.CI_JOB_TEMP }}/carrot-navi/\n",
                    "${{ env.CI296_SOURCE }}/rust/target/debug/openpilot-carrot-navi\n",
                    "${{ env.CI296_SOURCE }}/rust/target/debug/examples/carrot-navi-policy\n",
                    "${{ env.CI296_SOURCE }}/rust/target/debug/examples/carrot-navi-wire\n"
                );
                if step["with"]["path"] != path || step["id"] != "local_evidence_1" {
                    return Err("Carrot capture paths or upload receipt changed".into());
                }
                step["with"]["path"] = original["with"]["path"].clone();
                step.as_object_mut()
                    .ok_or("Carrot step is not a map")?
                    .remove("id");
            }
            _ => {}
        }
        if original["name"] == "Reserve runner capacity and install confirmed binding dependencies"
        {
            let before = original["run"]
                .as_str()
                .ok_or("historical dependency run missing")?;
            let body = before
                .strip_prefix("sudo rm -rf /usr/local/lib/android /opt/ghc /usr/share/dotnet\n")
                .ok_or("historical dependency prelude changed")?;
            let apt = "sudo apt-get update && sudo apt-get install -y capnproto libcapnp-dev g++ libzmq3-dev iproute2 util-linux\n";
            if body.matches(apt).count() != 1 {
                return Err("historical dependency command changed".into());
            }
            let reviewed = body.replacen(apt, "sudo apt-get -o \"APT::Architectures=$(dpkg --print-architecture)\" update && sudo apt-get -o \"APT::Architectures=$(dpkg --print-architecture)\" install -y capnproto libcapnp-dev g++ libzmq3-dev iproute2 util-linux\n", 1);
            if step["run"] != reviewed {
                return Err("Carrot dependency preparation exceeded reviewed overlay".into());
            }
            step["run"] = before.into();
        }
    }
    retained["steps"] = Value::Array(retained_steps);
    if retained != *expected {
        return Err("retained original runtime/QA traffic changed".into());
    }
    Ok((scripts.remove(0), scripts.remove(0)))
}

#[test]
fn maintained_policy_rejects_embedded_oracle_and_changed_traffic() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = fs::read_to_string(repo.join(".github/workflows/rust.yml")).unwrap();
    maintained_steps(&workflow).unwrap();
    // Mutation checks prove the policy is sensitive to reintroduction, a caller
    // bypass and removal of original QA traffic. These do not rerun that traffic.
    let model = parse_workflow(&workflow).unwrap();
    for mutation in ["caller-bypass", "original-traffic", "embedded-oracle"] {
        let mut changed = model.clone();
        let steps = changed["jobs"]["carrot-navi-runtime"]["steps"]
            .as_array_mut()
            .unwrap();
        let name = if mutation == "original-traffic" {
            "Run existing source policy and state comparison inputs"
        } else {
            "Require exact existing policy and normalized process comparisons"
        };
        assert_eq!(steps.iter().filter(|s| s["name"] == name).count(), 1);
        let step = steps.iter_mut().find(|s| s["name"] == name).unwrap();
        let before = step["run"].as_str().unwrap().to_owned();
        let after = match mutation {
            "caller-bypass" => {
                let command = "\"$CARROT_NAVI_COMPARISON\" \"$RUNNER_TEMP/carrot-navi\"";
                assert_eq!(before.matches(command).count(), 1);
                before.replace(command, "true")
            }
            "original-traffic" => {
                let command = "python -P rust/tools/carrot_navi_cases.py";
                assert_eq!(before.matches(command).count(), 1);
                before.replace(command, "echo")
            }
            "embedded-oracle" => {
                format!("{before}python -P - <<'PY'\n{}PY\n", oracle(&repo))
            }
            _ => unreachable!(),
        };
        assert_ne!(after, before);
        step["run"] = after.into();
        assert_ne!(changed, model);
        assert!(
            maintained_steps(&changed.to_string()).is_err(),
            "{mutation}"
        );
    }
    for entry in fs::read_dir(repo.join(".github/workflows")).unwrap() {
        let path = entry.unwrap().path();
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml" | "yaml")
        ) {
            let text = fs::read_to_string(&path).unwrap();
            assert!(
                !(text.contains("def normalize(rows):")
                    && text.contains("cereal_publish_count")
                    && text.contains("'discovery/wire'")),
                "Carrot embedded comparison remains executable in {}",
                path.display()
            );
        }
    }
    // Its independent helper remains shared by the unchanged runtime QA family.
    oracle(&repo);
}

#[test]
fn maintained_policy_accepts_yaml_aliases_and_rejects_unreviewed_overlays() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let workflow = fs::read_to_string(repo.join(".github/workflows/rust.yml")).unwrap();
    let expected = maintained_steps(&workflow).unwrap();
    let model = parse_workflow(&workflow).unwrap();
    let job = &model["jobs"]["carrot-navi-runtime"];
    for representation in [
        serde_json::to_string_pretty(&model).unwrap(),
        format!("runtime: &runtime {job}\njobs:\n  carrot-navi-runtime: *runtime\n"),
    ] {
        assert_eq!(maintained_steps(&representation).unwrap(), expected);
    }
    for mutation in [
        "retained-byte",
        "adjacent-traffic",
        "reordered-traffic",
        "duplicate-comparator",
        "unknown-run",
        "private-run",
        "private-env",
        "wrong-host",
        "wrong-upload",
        "growth",
        "missing-trust",
        "optional-job",
        "action-shell",
    ] {
        let mut changed = model.clone();
        let job = &mut changed["jobs"]["carrot-navi-runtime"];
        match mutation {
            "wrong-host" => job["runs-on"] = "ubuntu-latest".into(),
            "optional-job" => job["continue-on-error"] = "true".into(),
            _ => {
                let steps = job["steps"].as_array_mut().unwrap();
                match mutation {
                    "retained-byte" => steps[10]["run"] = format!("{}\n", steps[10]["run"].as_str().unwrap()).into(),
                    "adjacent-traffic" => { steps.remove(16); }
                    "reordered-traffic" => steps.swap(15, 16),
                    "duplicate-comparator" => { let copy=steps[18].clone(); steps.insert(18, copy); }
                    "unknown-run" => steps.insert(18, json!({"name":"Unreviewed traffic","run":"true\n","if":TRUSTED_SOURCE,"working-directory":"${{ env.CI296_SOURCE }}"})),
                    "private-run" => steps[2]["run"] = format!("{}true\n", steps[2]["run"].as_str().unwrap()).into(),
                    "private-env" => steps[5]["env"]["CI296_LATE_CARROT_NAVI_BIN"] = "/tmp/unrelated".into(),
                    "wrong-upload" => steps[20]["with"]["path"] = "${{ runner.temp }}/unrelated\n".into(),
                    "growth" => {
                        let before = steps[9]["run"].as_str().unwrap();
                        let after = before.replace("--growth-mib 8192", "--growth-mib 8191");
                        assert_ne!(before, after);
                        steps[9]["run"] = after.into();
                    }
                    "missing-trust" => { steps[15].as_object_mut().unwrap().remove("if"); }
                    "action-shell" => steps[11]["with"]["run-shell"] = "bash -x".into(),
                    _ => unreachable!(),
                }
            }
        }
        assert_ne!(changed, model);
        assert!(
            maintained_steps(&changed.to_string()).is_err(),
            "{mutation}"
        );
    }
}

fn saved_seals(saved: &Path) {
    for line in include_str!("fixtures/carrot-navi-comparator/saved-members.sha256").lines() {
        let (sha, name) = line.split_once("  ").unwrap();
        assert_eq!(
            hash(&fs::read(saved.join(name)).unwrap()),
            sha,
            "saved {name}"
        );
    }
}

fn input_state(root: &Path) -> Vec<(bool, Option<Vec<u8>>)> {
    INPUTS
        .iter()
        .map(|name| (root.join(name).is_dir(), fs::read(root.join(name)).ok()))
        .collect()
}

#[test]
fn maintained_caller_matches_frozen_oracle_on_held_capture_and_remaining_edges() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let (_, caller) =
        maintained_steps(&fs::read_to_string(repo.join(".github/workflows/rust.yml")).unwrap())
            .unwrap();
    let script = oracle(&repo);
    let saved =
        PathBuf::from(std::env::var_os("CARROT_NAVI_SAVED_ROOT").expect("sealed held JSON"));
    saved_seals(&saved);
    // This exact receipt is from accepted hosted39826752939. Reuse its 72
    // controls; do not execute them or infer new source-family acceptance.
    let accepted =
        fs::read(std::env::var_os("CARROT_NAVI_ACCEPTED_CONTROLS").expect("accepted receipt"))
            .unwrap();
    assert_eq!(hash(&accepted), ACCEPTED_CONTROLS_SHA);
    let accepted: serde_json::Value = serde_json::from_slice(&accepted).unwrap();
    let prior = accepted["cases"].as_array().unwrap();
    assert_eq!(prior.len(), 72);
    assert_eq!(
        prior.iter().filter(|c| c["expected_pass"] == true).count(),
        20
    );
    assert_eq!(
        prior
            .iter()
            .map(|c| c["output_sha256"].as_object().unwrap().len())
            .sum::<usize>(),
        122
    );
    for control in prior {
        let status = if control["expected_pass"] == true {
            0
        } else {
            1
        };
        assert_eq!(control["source_status"], status);
        assert_eq!(control["native_status"], status);
        assert_eq!(control["native_execs"], 1);
    }
    assert_eq!(accepted["oracle_sha256"], ORACLE_SHA);
    let evidence = PathBuf::from(
        std::env::var_os("CARROT_NAVI_CALLER_EVIDENCE").expect("fresh evidence root"),
    );
    fs::create_dir(&evidence).unwrap();
    write(&evidence.join("maintained-caller.sh"), &caller);
    write(&evidence.join("original-embedded.py"), &script);
    let binary = PathBuf::from(
        std::env::var_os("CARROT_NAVI_COMPARISON").expect("actual maintained build output"),
    );
    assert!(binary.is_absolute());
    let binary_hash = hash(&fs::read(&binary).unwrap());
    let build_proof =
        fs::read_to_string(std::env::var_os("CARROT_NAVI_MAINTAINED_BUILD_PROOF").unwrap())
            .unwrap();
    assert_eq!(
        build_proof,
        format!("{binary_hash}  {}\n", binary.display())
    );
    write(&evidence.join("native-elf.sha256"), &build_proof);
    let python =
        std::env::var_os("CARROT_NAVI_ORIGINAL_PYTHON").expect("explicit hosted oracle Python");
    let mut cases = vec![(
        case("held-179-policy-56-socket-21-lifecycle", true, vec![]),
        None,
        None,
    )];
    for name in INPUTS {
        cases.push((
            case(
                &format!("missing-{}", name.replace('/', "-")),
                false,
                vec![(name, None)],
            ),
            None,
            None,
        ));
        cases.push((
            case(
                &format!("malformed-{}", name.replace('/', "-")),
                false,
                vec![(name, Some("{".into()))],
            ),
            None,
            None,
        ));
    }
    for name in OUTPUTS {
        cases.push((
            case(&format!("blocked-{}", name), false, vec![]),
            Some(name),
            None,
        ));
    }
    for name in ["socket-source/rows.json", "startup-native/discovery.json"] {
        cases.push((
            case(
                &format!("directory-{}", name.replace('/', "-")),
                false,
                vec![(name, None)],
            ),
            None,
            Some(name),
        ));
    }
    cases.push((
        case(
            "discovery-wire-order",
            false,
            vec![
                (
                    "startup-source/discovery.json",
                    Some(r#"[{"wire_hex":"aa"},{"wire_hex":"bb"}]"#.into()),
                ),
                (
                    "startup-native/discovery.json",
                    Some(r#"[{"wire_hex":"bb"},{"wire_hex":"aa"}]"#.into()),
                ),
            ],
        ),
        None,
        None,
    ));
    cases.push((case("held path with space '[$]", true, vec![]), None, None));
    assert_eq!(cases.len(), 42);
    let mut receipts = Vec::new();
    for (case, blocked_output, directory_input) in cases {
        let destination = evidence.join(&case.name);
        let source_parent = destination.join("source");
        let native_parent = destination.join("native");
        let source_root = seed(&saved, &source_parent, &case.edits);
        let native_root = seed(&saved, &native_parent, &case.edits);
        for root in [&source_root, &native_root] {
            // Preserve inherited in-place overwrite and failure semantics, even
            // when an old success receipt exists. Status, not presence, decides.
            for name in OUTPUTS {
                if blocked_output == Some(name) {
                    fs::create_dir(root.join(name)).unwrap();
                } else {
                    write(&root.join(name), format!("stale:{name}\n"));
                }
            }
            if let Some(name) = directory_input {
                fs::create_dir(root.join(name)).unwrap();
            }
        }
        let before_source = input_state(&source_root);
        let before_native = input_state(&native_root);
        let source = Command::new(&python)
            .args(["-P", "-c", &script])
            .env("RUNNER_TEMP", &source_parent)
            .env("PYTHONPATH", repo.join("rust/tools"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .current_dir(&repo)
            .output()
            .unwrap();
        save_output(&source_parent, &source);
        let trace_path = native_parent.join("exec.trace");
        let native = Command::new("strace")
            .args([
                "-f",
                "-qq",
                "-s",
                "4096",
                "-e",
                "trace=execve,execveat",
                "-e",
                "signal=none",
                "-o",
            ])
            .arg(&trace_path)
            .args([
                "--",
                "/bin/bash",
                "--noprofile",
                "--norc",
                "-e",
                "-o",
                "pipefail",
            ])
            .arg(evidence.join("maintained-caller.sh"))
            .env("RUNNER_TEMP", &native_parent)
            .env("CARROT_NAVI_COMPARISON", &binary)
            .current_dir(&repo)
            .output()
            .unwrap();
        save_output(&native_parent, &native);
        let trace = fs::read_to_string(&trace_path).unwrap();
        let execs = trace.lines().collect::<Vec<_>>();
        assert_eq!(
            execs.len(),
            2,
            "{} unexpected child/interpreter: {trace}",
            case.name
        );
        for (line, executable) in execs.iter().zip([Path::new("/bin/bash"), binary.as_path()]) {
            assert!(
                line.contains(&format!("execve(\"{}\"", executable.display()))
                    && line.ends_with("= 0"),
                "unexpected exec: {trace}"
            );
        }
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
        assert_eq!(
            input_state(&source_root),
            before_source,
            "{} original mutated inputs",
            case.name
        );
        assert_eq!(
            input_state(&native_root),
            before_native,
            "{} caller mutated inputs",
            case.name
        );
        let mut outputs = serde_json::Map::new();
        for name in OUTPUTS {
            assert_eq!(
                native_root.join(name).is_dir(),
                source_root.join(name).is_dir()
            );
            assert_eq!(
                native_root.join(name).is_dir(),
                blocked_output == Some(name)
            );
            let a = fs::read(native_root.join(name)).ok();
            let e = fs::read(source_root.join(name)).ok();
            assert_eq!(
                a, e,
                "{} bytes/existence/partial-write order {name}",
                case.name
            );
            if let Some(bytes) = a {
                if case.pass {
                    assert_eq!(
                        bytes,
                        fs::read(saved.join(name)).unwrap(),
                        "historical {name}"
                    );
                }
                outputs.insert(name.into(), hash(&bytes).into());
            }
        }
        receipts.push(serde_json::json!({"name":case.name,"expected_pass":case.pass,"source_status":source.status.code(),"native_status":native.status.code(),"shell_execs":1,"native_execs":1,"input_files_unchanged":true,"output_sha256":outputs}));
    }
    saved_seals(&saved);
    write(&evidence.join("caller-controls.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "historical_source":CAPTURE_SHA,"oracle_sha256":ORACLE_SHA,"retained_job_sha256":RETAINED_JOB_SHA,
        "reused_accepted_run":39826752939_u64,"reused_controls":72,"reused_controls_sha256":ACCEPTED_CONTROLS_SHA,
        "current_binary_sha256":binary_hash,"cases":receipts
    })).unwrap());
}
