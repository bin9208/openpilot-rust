//! Same-ELF original-only beep policy proof with a private checker-absent root.
//! The checkout lane is present before deletion and absent after deletion;
//! BEEP_REFERENCE_EXPECT_ABSENT makes that phase an explicit cloud contract.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("missing mandatory {name}")))
}

fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("retired Python checker reappeared: {}", path.display()),
        Err(error) => panic!("cannot establish checker absence: {error}"),
    }
}

fn private_root(repo: &Path, root: &Path) {
    let provenance: Value = serde_json::from_slice(include_bytes!(
        "fixtures/beep-reference/original-binding-provenance-3835653.json"
    ))
    .unwrap();
    let mut names = provenance["sources"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    names.extend(
        [
            "rust/tools/beep_source.py",
            "rust/tools/beep_fixtures.py",
            "rust/tools/original_params_binding.py",
            "openpilot/selfdrive/controls/beep.py",
            "openpilot/common/utils.py",
            "openpilot/common/realtime.py",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    for name in names {
        let target = root.join(&name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(repo.join(&name), target).unwrap();
    }
    absent(&root.join("rust/tools/check_beepd_reference.py"));
}

fn audit_exec(trace: &Path, checker: &Path, candidate: &Path, argv: &[String]) -> Vec<Value> {
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut records = Vec::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for line in fs::read_to_string(trace)
        .unwrap()
        .lines()
        .filter(|line| opening.is_match(line))
    {
        assert!(!line.contains("execveat("), "opaque execveat: {line}");
        let found = invocation
            .captures(line)
            .unwrap_or_else(|| panic!("failed, resumed or opaque exec: {line}"));
        let path: String = serde_json::from_str(&found[1]).unwrap();
        let args: Vec<String> = serde_json::from_str(&found[2]).unwrap();
        if path == checker.to_str().unwrap() {
            assert_eq!(args, argv, "checker argv changed");
        } else if path == candidate.to_str().unwrap() {
            assert_eq!(
                args,
                [candidate.to_str().unwrap().to_owned()],
                "native example argv changed"
            );
        } else {
            panic!("unsealed executable attempt: {line}");
        }
        *counts.entry(path.clone()).or_default() += 1;
        records.push(json!({"path":path,"argv":args}));
    }
    assert_eq!(counts.get(checker.to_str().unwrap()), Some(&1));
    assert_eq!(counts.get(candidate.to_str().unwrap()), Some(&20));
    assert_eq!(records.len(), 21);
    records
}

struct LaneInputs<'a> {
    checker: &'a Path,
    candidate: &'a Path,
    original: &'a Path,
    original_hash: &'a str,
}

fn run_lane(
    lane: &str,
    root: &Path,
    checker_absent: bool,
    proof: &Path,
    inputs: &LaneInputs<'_>,
) -> Result<Value> {
    let checker = inputs.checker;
    let candidate = inputs.candidate;
    let original = inputs.original;
    let original_hash = inputs.original_hash;
    let output = proof.join(format!("{lane}-output"));
    let trace = proof.join(format!("{lane}.exec"));
    let mut argv = vec![
        checker.to_str().ok_or("non-UTF8 checker")?.to_owned(),
        "--binary".into(),
        candidate.to_str().ok_or("non-UTF8 candidate")?.to_owned(),
        "--original".into(),
        original.to_str().ok_or("non-UTF8 original")?.to_owned(),
        "--original-manifest-sha256".into(),
        original_hash.to_owned(),
        "--output".into(),
        output.to_str().ok_or("non-UTF8 output")?.to_owned(),
    ];
    if checker_absent {
        argv.push("--checker-absent".into());
    }
    let strace = Path::new("/usr/bin/strace");
    assert!(strace.is_file(), "mandatory host strace is missing");
    let mut args = vec![
        strace.to_str().unwrap().to_owned(),
        "-f".into(),
        "-qq".into(),
        "-s65535".into(),
        "-e".into(),
        "trace=execve,execveat".into(),
        "-o".into(),
        trace.to_str().unwrap().to_owned(),
    ];
    args.extend(argv.clone());
    let mut spec = CommandSpec::new(args, Duration::from_secs(300));
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root.to_str().ok_or("non-UTF8 root")?.to_owned(),
    );
    spec.env.insert("PATH".into(), "/usr/bin:/bin".into());
    let captured = NativeRunner.run(&spec)?;
    fs::write(proof.join(format!("{lane}.stdout")), &captured.stdout)?;
    fs::write(proof.join(format!("{lane}.stderr")), &captured.stderr)?;
    fs::write(
        proof.join(format!("{lane}.code")),
        captured.code.to_string(),
    )?;
    assert_eq!(
        captured.code,
        0,
        "{lane}: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(captured.stderr.is_empty(), "{lane} checker stderr");
    let executions = audit_exec(&trace, checker, candidate, &argv);
    fs::write(
        proof.join(format!("{lane}.exec.json")),
        serde_json::to_vec_pretty(&executions)?,
    )?;
    let summary: Value = serde_json::from_slice(&fs::read(output.join("summary.json"))?)?;
    assert_eq!(summary["passed"], true);
    assert_eq!(summary["scenarios"], 20);
    if checker_absent {
        absent(&root.join("rust/tools/check_beepd_reference.py"));
    }
    Ok(summary)
}

#[test]
fn same_elf_present_or_deleted_checkout_and_private_absent_roots_match_original_twenty_cases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let checker = required("BEEP_REFERENCE_CHECKER_BINARY")
        .canonicalize()
        .unwrap();
    let candidate = required("BEEP_REFERENCE_BINARY").canonicalize().unwrap();
    let original = required("BEEP_REFERENCE_ORIGINAL").canonicalize().unwrap();
    let original_hash = env::var("BEEP_REFERENCE_ORIGINAL_MANIFEST_SHA256").unwrap();
    let proof = required("BEEP_REFERENCE_RETIREMENT_EVIDENCE");
    fs::create_dir(&proof).unwrap();
    let expected_absent = match env::var("BEEP_REFERENCE_EXPECT_ABSENT").unwrap().as_str() {
        "0" => false,
        "1" => true,
        other => panic!("bad checker presence contract: {other}"),
    };
    let historical = repo.join("rust/tools/check_beepd_reference.py");
    if expected_absent {
        absent(&historical);
    } else {
        assert!(fs::symlink_metadata(&historical)
            .unwrap()
            .file_type()
            .is_file());
        assert_eq!(
            sha(&historical),
            "051315d756ee2afd43e94051067283c597c2fbcf83d48d10721c62c591797177"
        );
    }
    let current_elf = BTreeMap::from([
        (checker.display().to_string(), sha(&checker)),
        (candidate.display().to_string(), sha(&candidate)),
    ]);
    let private = tempfile::tempdir().unwrap();
    private_root(&repo, private.path());
    let inputs = LaneInputs {
        checker: &checker,
        candidate: &candidate,
        original: &original,
        original_hash: &original_hash,
    };
    let checkout = run_lane("checkout", &repo, expected_absent, &proof, &inputs).unwrap();
    let absent_lane = run_lane("private", private.path(), true, &proof, &inputs).unwrap();
    assert_eq!(checkout, absent_lane);
    for (name, digest) in &current_elf {
        assert_eq!(sha(Path::new(name)), *digest, "ELF changed: {name}");
    }
    if expected_absent {
        absent(&historical);
    }
    let mut compared = 0;
    for entry in fs::read_dir(proof.join("checkout-output")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".comparison.json") {
            compared += 1;
            let left: Value = serde_json::from_slice(&fs::read(entry.path()).unwrap()).unwrap();
            let right: Value = serde_json::from_slice(
                &fs::read(proof.join("private-output").join(&name)).unwrap(),
            )
            .unwrap();
            assert_eq!(
                left,
                right,
                "same-ELF paired case changed: {}",
                name.to_string_lossy()
            );
            let case = name
                .to_string_lossy()
                .strip_suffix(".comparison.json")
                .unwrap()
                .to_owned();
            for suffix in ["rust.stdout", "rust.stderr"] {
                let file = format!("{case}-{suffix}");
                assert_eq!(
                    fs::read(proof.join("checkout-output").join(&file)).unwrap(),
                    fs::read(proof.join("private-output").join(&file)).unwrap(),
                    "raw {file} changed"
                );
            }
            let file = format!("{case}-rust.trace.jsonl");
            assert_eq!(
                fs::read(proof.join("checkout-output").join(&file)).ok(),
                fs::read(proof.join("private-output").join(&file)).ok(),
                "raw {file} changed"
            );
        }
    }
    assert_eq!(compared, 20, "a comparison case disappeared");
    fs::write(
        proof.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "passed":true,"cases_per_lane":20,"exec_attempts_per_lane":21,
            "checker_absent_in_checkout":expected_absent,"executables":current_elf,
            "original_manifest_sha256":original_hash
        }))
        .unwrap(),
    )
    .unwrap();
}
