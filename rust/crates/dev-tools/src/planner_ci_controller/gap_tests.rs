//! 성공 golden은 실제841 archive에만 있다. 여기서는 metadata·비교·실행 경계를 검사한다.
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

fn directory() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[test]
fn qualified_both_hosts_bind_real841_artifacts_and_same_three_original_files() {
    let x86 = parse_audit(X86_AUDIT, "x86_64").unwrap();
    let arm = parse_audit(ARM_AUDIT, "aarch64").unwrap();
    assert_eq!(
        (x86.artifact.id, arm.artifact.id),
        (12623801541, 12623646303)
    );
    assert_eq!(
        (x86.input.bytes, x86.expected.bytes, x86.receipt.bytes),
        (3293789, 265691, 131)
    );
    assert_eq!(
        (arm.input.bytes, arm.expected.bytes, arm.receipt.bytes),
        (3293789, 265691, 131)
    );
    assert_eq!(x86.input.sha256, arm.input.sha256);
    assert_eq!(x86.expected.sha256, arm.expected.sha256);
    assert_eq!(x86.receipt.sha256, arm.receipt.sha256);
    assert_eq!(x86.receipt_contract, arm.receipt_contract);
    assert_eq!(x86.receipt_contract.actions, 857);
    assert_eq!(x86.receipt_contract.owners, 18);
    assert_eq!(x86.receipt_contract.positive_plans, 187);
    assert_eq!(x86.receipt_contract.positive_credit, 9);
    assert!(x86.receipt_contract.first_mismatches.is_empty());
    let mut receipt_bytes = serde_json::to_vec_pretty(&x86.receipt_contract).unwrap();
    receipt_bytes.push(b'\n');
    assert_eq!(receipt_bytes.len(), 131);
    assert_eq!(hash(&receipt_bytes), x86.receipt.sha256);
    assert!(x86
        .input
        .member
        .starts_with("tmp/openpilot-ci.afCEBf/planner-comparison/gap/"));
    assert!(arm
        .input
        .member
        .starts_with("tmp/openpilot-ci.s6xgRN/planner-comparison/gap/"));
}

#[test]
fn unknown_host_metadata_and_every_changed_original_descriptor_reject() {
    assert!(qualified_audit("unknown").is_err());
    assert!(parse_audit(X86_AUDIT, "aarch64").is_err());
    for pointer in [
        "/artifact/id",
        "/artifact/sha256",
        "/input/bytes",
        "/input/path",
        "/expected/sha256",
        "/receipt/member",
        "/source_commit",
        "/receipt_contract/positive_plans",
    ] {
        let mut changed: Value = serde_json::from_slice(X86_AUDIT).unwrap();
        *changed.pointer_mut(pointer).unwrap() = json!("tampered");
        assert!(
            parse_audit(&serde_json::to_vec_pretty(&changed).unwrap(), "x86_64").is_err(),
            "{pointer}"
        );
    }
    let mut unknown: Value = serde_json::from_slice(X86_AUDIT).unwrap();
    unknown["new_field"] = json!(true);
    assert!(serde_json::from_value::<Audit>(unknown).is_err());
}

#[test]
fn staged_roster_rejects_extra_missing_symlink_directory_and_shared_inode() {
    let root = directory();
    for name in ["input.json", "expected.json", "receipt.json", "audit.json"] {
        write_new(&root.path().join(name), b"private negative-control data").unwrap();
    }
    stage_roster(root.path()).unwrap();
    let extra = root.path().join("extra");
    write_new(&extra, b"x").unwrap();
    assert!(stage_roster(root.path()).is_err());
    fs::remove_file(&extra).unwrap();
    let input = root.path().join("input.json");
    fs::remove_file(&input).unwrap();
    assert!(stage_roster(root.path()).is_err());
    symlink(root.path().join("expected.json"), &input).unwrap();
    assert!(stage_roster(root.path()).is_err());
    fs::remove_file(&input).unwrap();
    fs::create_dir(&input).unwrap();
    assert!(stage_roster(root.path()).is_err());
    fs::remove_dir(&input).unwrap();
    fs::hard_link(root.path().join("expected.json"), &input).unwrap();
    assert!(stage_roster(root.path()).is_err());
    fs::remove_file(&input).unwrap();
    write_new(&input, b"x").unwrap();
    OpenOptions::new()
        .write(true)
        .open(&input)
        .unwrap()
        .set_len(STAGE_LIMIT)
        .unwrap();
    assert!(stage_roster(root.path()).is_err());
}

#[test]
fn pinned_reader_rejects_hash_size_alias_and_same_bytes_inode_replacement() {
    let root = directory();
    let path = root.path().join("raw");
    write_new(&path, b"x").unwrap();
    let digest = "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881";
    let (bytes, held) = HeldFile::open(&path, 1, digest, true).unwrap();
    assert_eq!(bytes, b"x");
    assert!(HeldFile::open(&path, 2, digest, true).is_err());
    assert!(HeldFile::open(&path, 1, &"0".repeat(64), true).is_err());
    let alias = root.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(HeldFile::open(&alias, 1, digest, true).is_err());
    fs::rename(&path, root.path().join("old-inode")).unwrap();
    write_new(&path, b"x").unwrap();
    assert!(held.verify().is_err());
}

#[test]
fn original_float_bits_integer_credit_keys_and_lengths_stay_strict() {
    let expected = json!({"plan":{"confidence":-0.0},"credit":[18446744073709551615_u64]});
    let mut differences = Differences::default();
    compare(&expected, &expected, "", &mut differences);
    assert_eq!(differences.count, 0);
    let mut changed = expected.clone();
    changed["plan"]["confidence"] = json!(0.0);
    compare(&expected, &changed, "", &mut differences);
    assert_eq!(differences.count, 1);
    let mut changed = expected.clone();
    changed["credit"][0] = json!(18446744073709551614_u64);
    compare(&expected, &changed, "", &mut differences);
    assert_eq!(differences.count, 2);
    compare(&expected, &json!({}), "", &mut differences);
    compare(&json!([0, 1]), &json!([0]), "", &mut differences);
    assert_eq!(differences.count, 4);
}

#[test]
fn all_mismatches_are_counted_and_only_first_fifty_are_retained() {
    let mut differences = Differences::default();
    compare(
        &json!(vec![0; 80]),
        &json!(vec![1; 80]),
        "",
        &mut differences,
    );
    assert_eq!(differences.count, 80);
    assert_eq!(differences.first.len(), 50);
    assert_eq!(differences.first[49]["path"], "[49]");
}

#[test]
fn trace_invocation_reuses_bounded_stdin_and_remaining_single_deadline() {
    let root = directory();
    let options = Options {
        root: root.path().into(),
        binary: root.path().join("plannerd_gap_trace"),
        corpus: root.path().join("corpus"),
        audit: root.path().join("corpus/audit.json"),
        output: root.path().join("output"),
        timeout: Duration::from_secs(2700),
        growth_bytes: 128 * MIB,
    };
    let spec = command(
        &options,
        b"original bytes\n",
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(spec.argv, [options.binary.to_str().unwrap()]);
    assert_eq!(spec.input.as_deref(), Some(b"original bytes\n".as_slice()));
    assert_eq!(spec.stdout_limit, 8 * MIB as usize);
    assert_eq!(spec.stderr_limit, MIB as usize);
    assert!(spec.timeout <= Duration::from_secs(2));
    assert_eq!(spec.cwd.as_deref(), Some(root.path()));
    assert!(spec.env.is_empty());
    assert!(!spec.merge_output && !spec.inherit_stdin && !spec.inherit_output);
    assert!(command(&options, &[], Instant::now() - Duration::from_secs(1)).is_err());
    assert!(group_guard(12, 11).is_err());
    group_guard(12, 12).unwrap();
}

#[test]
fn trace_exit42_remains_in_receipt_and_original_wrapper_failure_stops() {
    let root = directory();
    let spec = CommandSpec::new(vec!["must-not-execute".into()], Duration::from_secs(1));
    let mut calls = 0;
    let error = capture_trace(&spec, root.path(), |_| {
        calls += 1;
        assert!(root.path().join("invocation.json").exists());
        Ok(Captured {
            code: 42,
            stdout: b"original raw failure".to_vec(),
            stderr: b"diagnostic".to_vec(),
        })
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(error.to_string().contains("42"));
    // bin/check_plannerd_ci.rs는 CheckFailure 이외의 원래 미처리 예외를 exit1로 반환한다.
    assert!(error
        .downcast_ref::<crate::planner_ci_controller::CheckFailure>()
        .is_none());
    assert_eq!(
        fs::read(root.path().join("actual.json")).unwrap(),
        b"original raw failure"
    );
    let record: Value =
        serde_json::from_slice(&fs::read(root.path().join("trace-command.json")).unwrap()).unwrap();
    assert_eq!(record["returncode"], 42);
    assert!(!root.path().join("receipt.json").exists());
}

#[test]
fn shared_qa_timeout_retains_partial_streams_without_fallback_or_second_call() {
    let root = directory();
    let spec = CommandSpec::new(vec!["must-not-execute".into()], Duration::from_secs(1));
    let mut calls = 0;
    assert!(capture_trace(&spec, root.path(), |_| {
        calls += 1;
        Err(Box::new(CaptureError::Timeout {
            timeout: Duration::from_secs(1),
            stdout: b"partial stdout".to_vec(),
            stderr: b"partial stderr".to_vec(),
        }))
    })
    .is_err());
    assert_eq!(calls, 1);
    assert_eq!(
        fs::read(root.path().join("actual.json")).unwrap(),
        b"partial stdout"
    );
    assert_eq!(
        fs::read(root.path().join("stderr.txt")).unwrap(),
        b"partial stderr"
    );
    assert!(!root.path().join("receipt.json").exists());
}

#[test]
#[ignore = "requires the same-job authenticated original841 gap/lifecycle stages; no oracle is executed"]
fn frozen841_stage_authenticates857_original_results_and_two_observed_boundaries() {
    let job = PathBuf::from(std::env::var("CI_JOB_TEMP").unwrap());
    let corpus = Corpus::open(
        &job.join("planner-gap-original"),
        &job.join("planner-gap-original/audit.json"),
        &job.join("planner-lifecycle-original"),
        std::env::consts::ARCH,
    )
    .unwrap();
    assert_eq!(corpus.expected.len(), 857);
    for i in [537, 590, 643, 696] {
        assert_eq!(corpus.expected[i]["direction"], 0);
        assert_eq!(corpus.expected[i]["plan"]["reason"], "legacy-side-input");
        assert_eq!(corpus.expected[i]["plan"]["targets"], json!([]));
    }
    assert_eq!(corpus.expected[856]["plan"]["targets"], json!([]));
    assert_eq!(corpus.expected[856]["plan"]["entry_ids"], json!([-1, -1]));
    corpus.verify().unwrap();
}
