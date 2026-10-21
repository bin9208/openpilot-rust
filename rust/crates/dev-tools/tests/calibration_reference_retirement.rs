//! Same-ELF calibration estimator proof across the checkout and a private
//! checker-absent root. The original-only 15,128-step capture remains sealed.
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 2] = [
    "rust/tools/check_calibration_reference.py",
    "rust/tools/calibration_cases.py",
];

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("missing mandatory {name}")))
}

fn sha(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn elf(path: &Path) -> String {
    let bytes = fs::read(path).unwrap();
    assert!(
        bytes.starts_with(b"\x7fELF\x02\x01"),
        "not a sealed ELF: {}",
        path.display()
    );
    format!("{:x}", Sha256::digest(bytes))
}

fn absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("retired source reappeared: {}", path.display()),
        Err(error) => panic!("cannot establish source absence: {error}"),
    }
}

fn private_root(repo: &Path, root: &Path, sources: &serde_json::Map<String, Value>) {
    assert_eq!(sources.len(), 15, "original source map lost coverage");
    for retired in RETIRED {
        assert!(
            sources.contains_key(retired),
            "historical source pin missing"
        );
    }
    for (name, hash) in sources {
        let source = repo.join(name);
        if RETIRED.contains(&name.as_str()) {
            continue;
        }
        assert!(fs::symlink_metadata(&source).unwrap().file_type().is_file());
        assert_eq!(
            sha(&source),
            hash.as_str().unwrap(),
            "source changed: {name}"
        );
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source, target).unwrap();
    }
    for retired in RETIRED {
        absent(&root.join(retired));
    }
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
                "candidate argv changed"
            );
        } else {
            panic!("unsealed executable attempt: {line}");
        }
        *counts.entry(path.clone()).or_default() += 1;
        records.push(json!({"path":path,"argv":args}));
    }
    assert_eq!(counts.get(checker.to_str().unwrap()), Some(&1));
    assert_eq!(counts.get(candidate.to_str().unwrap()), Some(&1));
    assert_eq!(records.len(), 2);
    records
}

struct LaneInputs<'a> {
    checker: &'a Path,
    candidate: &'a Path,
    proof: &'a Path,
}

fn run_lane(lane: &str, root: &Path, inputs: &LaneInputs<'_>) -> Result<Value> {
    let output = inputs.proof.join(format!("{lane}-output"));
    let trace = inputs.proof.join(format!("{lane}.exec"));
    let argv = vec![
        inputs
            .checker
            .to_str()
            .ok_or("non-UTF8 checker")?
            .to_owned(),
        "--binary".into(),
        inputs
            .candidate
            .to_str()
            .ok_or("non-UTF8 candidate")?
            .to_owned(),
        "--output".into(),
        output.to_str().ok_or("non-UTF8 output")?.to_owned(),
    ];
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
    let mut spec = CommandSpec::new(args, Duration::from_secs(1_500));
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root.to_str().ok_or("non-UTF8 root")?.to_owned(),
    );
    spec.env.insert("PATH".into(), "/usr/bin:/bin".into());
    spec.stdout_limit = 2 * 1024 * 1024;
    spec.stderr_limit = 2 * 1024 * 1024;
    let captured = NativeRunner.run(&spec)?;
    fs::write(
        inputs.proof.join(format!("{lane}.stdout")),
        &captured.stdout,
    )?;
    fs::write(
        inputs.proof.join(format!("{lane}.stderr")),
        &captured.stderr,
    )?;
    fs::write(
        inputs.proof.join(format!("{lane}.code")),
        captured.code.to_string(),
    )?;
    assert_eq!(
        captured.code,
        0,
        "{lane}: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(captured.stderr.is_empty(), "{lane} checker stderr");
    let executions = audit_exec(&trace, inputs.checker, inputs.candidate, &argv);
    fs::write(
        inputs.proof.join(format!("{lane}.exec.json")),
        serde_json::to_vec_pretty(&executions)?,
    )?;
    let report: Value = serde_json::from_slice(&fs::read(output.join("report.json"))?)?;
    let printed: Value = serde_json::from_slice(&captured.stdout)?;
    assert_eq!(report, printed, "{lane} CLI stdout differs from report");
    assert_eq!(report["result"], "pass");
    assert_eq!(report["steps"], 15_128);
    assert_eq!(report["expected_source_errors"], 30);
    assert_eq!(report["persistence_steps"], 14);
    let raw_trace = fs::read_to_string(output.join("trace.jsonl"))?;
    assert_eq!(raw_trace.lines().count(), 15_128);
    Ok(report)
}

#[test]
fn same_elf_present_or_deleted_checkout_and_private_absent_roots_match_all_original_steps() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let checker = required("CALIBRATION_REFERENCE_CHECKER_BINARY")
        .canonicalize()
        .unwrap();
    let candidate = required("CALIBRATION_REFERENCE_BINARY")
        .canonicalize()
        .unwrap();
    let proof = required("CALIBRATION_REFERENCE_RETIREMENT_EVIDENCE");
    fs::create_dir(&proof).unwrap();
    let expected_absent = match env::var("CALIBRATION_REFERENCE_EXPECT_ABSENT")
        .unwrap()
        .as_str()
    {
        "0" => false,
        "1" => true,
        other => panic!("bad source presence contract: {other}"),
    };
    let manifest: Value = serde_json::from_slice(include_bytes!(
        "fixtures/calibration-estimator-original-04ee2c/manifest.json"
    ))
    .unwrap();
    let sources = manifest["source_sha256"].as_object().unwrap();
    for retired in RETIRED {
        let path = repo.join(retired);
        if expected_absent {
            absent(&path);
        } else {
            assert!(fs::symlink_metadata(&path).unwrap().file_type().is_file());
            assert_eq!(
                sha(&path),
                sources[retired].as_str().unwrap(),
                "historical source changed"
            );
        }
    }
    let current_elf = BTreeMap::from([
        (checker.display().to_string(), elf(&checker)),
        (candidate.display().to_string(), elf(&candidate)),
    ]);
    let private = tempfile::tempdir().unwrap();
    private_root(&repo, private.path(), sources);
    let inputs = LaneInputs {
        checker: &checker,
        candidate: &candidate,
        proof: &proof,
    };
    let checkout = run_lane("checkout", &repo, &inputs).unwrap();
    let absent_lane = run_lane("private", private.path(), &inputs).unwrap();
    assert_eq!(checkout, absent_lane);
    for name in [
        "input.jsonl",
        "native.stdout",
        "native.stderr",
        "trace.jsonl",
        "report.json",
        "summary.json",
    ] {
        assert_eq!(
            fs::read(proof.join("checkout-output").join(name)).unwrap(),
            fs::read(proof.join("private-output").join(name)).unwrap(),
            "same-ELF {name} differs between roots"
        );
    }
    for (name, digest) in &current_elf {
        assert_eq!(sha(Path::new(name)), *digest, "ELF changed: {name}");
    }
    for retired in RETIRED {
        if expected_absent {
            absent(&repo.join(retired));
        }
        absent(&private.path().join(retired));
    }
    fs::write(
        proof.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "passed":true,"steps_per_lane":15_128,"exec_attempts_per_lane":2,
            "historical_sources_absent_in_checkout":expected_absent,
            "executables":current_elf,
            "original_trace_sha256":manifest["trace_sha256"],
        }))
        .unwrap(),
    )
    .unwrap();
}
