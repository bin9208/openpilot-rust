//! 역사 provider는 미기록이다. 성공 leaf는 실제 current CLI/trace를 Actions에서만 실행한다.
use super::controller;
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};

fn cli(args: Vec<std::ffi::OsString>) -> openpilot_dev_tools::qa_process::Captured {
    cli_at(args, None)
}
fn cli_at(
    args: Vec<std::ffi::OsString>,
    cwd: Option<PathBuf>,
) -> openpilot_dev_tools::qa_process::Captured {
    let mut argv = vec![env!("CARGO_BIN_EXE_check-controlsd-native").into()];
    argv.extend(args);
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(20));
    spec.cwd = cwd;
    NativeRunner.run(&spec).unwrap()
}

#[test]
fn frozen_normal40_admission_rejects_mixed_origins_and_mutation() {
    let id = controller::frozen::REFERENCE_ID;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let reference = root.join("reference");
    fs::create_dir(&reference).unwrap();
    fs::create_dir(reference.join("loop")).unwrap();
    // 거부 전용이다. 이 빈 파일들은 성공 기대값/fixture로 수용될 수 없다.
    for name in [
        "artifact.zip",
        "historical-job.log",
        "loop/input.json",
        "loop/source.json",
        "loop/results.json",
        "loop/process.log",
    ] {
        fs::write(reference.join(name), []).unwrap();
    }
    let evidence = root.join("must-not-create");
    let index = reference.join("frozen-reference-admission.json");
    let args = || {
        vec![
            "--root".into(),
            root.clone().into_os_string(),
            "--trace".into(),
            root.join("must-not-execute").into_os_string(),
            "--numerics".into(),
            root.clone().into_os_string(),
            "--evidence".into(),
            evidence.clone().into_os_string(),
            "--source-commit".into(),
            "1111111111111111111111111111111111111111".into(),
            "--frozen-reference".into(),
            id.into(),
            "--reference-dir".into(),
            reference.clone().into_os_string(),
        ]
    };
    let expect = |result: openpilot_dev_tools::qa_process::Captured, error: &str| {
        assert_eq!(result.code, 1);
        assert!(result.stdout.is_empty());
        assert_eq!(
            result.stderr,
            format!("controlsd native: {error}\n").as_bytes()
        );
        assert!(!evidence.exists());
    };
    let replace = |mut argv: Vec<std::ffi::OsString>, flag: &str, value: std::ffi::OsString| {
        argv.chunks_exact_mut(2)
            .find(|pair| pair[0] == flag)
            .unwrap()[1] = value;
        argv
    };
    fs::write(&index, controller::frozen::REFERENCE_INDEX).unwrap();
    // 완전한 기준 argv/roster/index는 필수 옵션 단계를 지나 실제 archive pin 검사까지 간다.
    // 빈 ZIP은 거부 전용이며, 어떤 분기에서도 성공 reference로 사용할 수 없다.
    expect(cli(args()), "frozen file SHA256 differs");
    let mut without_mode = args();
    without_mode.truncate(without_mode.len() - 4);
    without_mode.extend([
        "--original".into(),
        reference.clone().into_os_string(),
        "--python-prefix".into(),
        root.clone().into_os_string(),
        "--capture-sha256".into(),
        "0".repeat(64).into(),
        "--reference-dir".into(),
        reference.clone().into_os_string(),
    ]);
    expect(
        cli(without_mode),
        "--reference-dir needs --frozen-reference",
    );
    expect(
        cli(replace(args(), "--frozen-reference", "unknown".into())),
        "unknown frozen reference",
    );
    for (flag, value) in [
        ("--original", reference.clone().into_os_string()),
        ("--python-prefix", root.clone().into_os_string()),
        ("--capture-sha256", "0".repeat(64).into()),
    ] {
        let mut mixed = args();
        mixed.extend([flag.into(), value]);
        expect(cli(mixed), "fresh and frozen options cannot be mixed");
    }
    let mut duplicate = args();
    duplicate.extend(["--frozen-reference".into(), id.into()]);
    expect(cli(duplicate), "duplicate controlsd option");
    let mut unknown_flag = args();
    unknown_flag.extend(["--unknown".into(), "value".into()]);
    expect(cli(unknown_flag), "unknown controlsd option");
    expect(
        cli_at(
            replace(args(), "--reference-dir", "reference".into()),
            Some(root.clone()),
        ),
        "canonical frozen/numerics directory required",
    );
    // CLI와 controller의 ID 방어는 같은 오류 문구다. 별도 우선순위 대조로
    // CLI 분기를 없애도 뒤의 같은 문구 때문에 검사가 통과하는 경우를 막는다.
    let invalid_source = replace(args(), "--source-commit", "not-a-source-sha".into());
    expect(
        cli(invalid_source.clone()),
        "invalid current source identity",
    );
    expect(
        cli(replace(
            invalid_source,
            "--frozen-reference",
            "unknown".into(),
        )),
        "unknown frozen reference",
    );
    let recipe: Value = serde_json::from_str(controller::frozen::REFERENCE_INDEX).unwrap();
    for (field, value) in [
        ("artifact_id", json!(12456242813u64)),
        ("run_id", json!(39844308545u64)),
        ("job_id", json!(120055270739u64)),
        (
            "historical_checkout_sha",
            json!("1111111111111111111111111111111111111111"),
        ),
        (
            "historical_original_provider_closure",
            json!("current-provider"),
        ),
        ("historical_original_provider_closure_complete", json!(true)),
        ("historical_provider_paths_dereferenced", json!(true)),
        ("original_capture_executed", json!(true)),
    ] {
        let mut bad = recipe.clone();
        bad[field] = value;
        fs::write(&index, serde_json::to_vec(&bad).unwrap()).unwrap();
        let result = cli(args());
        assert_ne!(result.code, 0);
        assert!(
            String::from_utf8_lossy(&result.stderr)
                .contains("fixed frozen reference recipe differs"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!evidence.exists());
    }
    for mode in 0..5 {
        let mut bad = recipe.clone();
        match mode {
            0 => bad["members"][1]["name"] = json!("loop/native.json"),
            1 => bad["members"][1]["sha256"] = json!("0".repeat(64)),
            2 => {
                bad["members"].as_array_mut().unwrap().pop();
            }
            3 => {
                let first = bad["members"][0].clone();
                bad["members"].as_array_mut().unwrap().push(first);
            }
            _ => bad["historical_result"]["frames"] = json!(8654),
        }
        fs::write(&index, serde_json::to_vec(&bad).unwrap()).unwrap();
        let result = cli(args());
        assert_ne!(result.code, 0);
        assert!(String::from_utf8_lossy(&result.stderr)
            .contains("fixed frozen reference recipe differs"));
        assert!(!evidence.exists());
    }
    for raw in [
        controller::frozen::REFERENCE_INDEX.replacen(
            "\"schema\": 1",
            "\"schema\": 1, \"schema\": 1",
            1,
        ),
        controller::frozen::REFERENCE_INDEX.replacen(
            "\"schema\": 1",
            "\"schema\": 1, \"providers\": []",
            1,
        ),
    ] {
        fs::write(&index, raw).unwrap();
        let result = cli(args());
        assert_ne!(result.code, 0);
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("duplicate field") || error.contains("unknown field"));
        assert!(!evidence.exists());
    }
    fs::write(&index, controller::frozen::REFERENCE_INDEX).unwrap();
    let result = cli(args());
    assert_ne!(result.code, 0);
    assert!(String::from_utf8_lossy(&result.stderr).contains("frozen file SHA256 differs"));
    assert!(!evidence.exists());
    fs::remove_file(reference.join("loop/source.json")).unwrap();
    std::os::unix::fs::symlink("absent", reference.join("loop/source.json")).unwrap();
    let result = cli(args());
    assert_ne!(result.code, 0);
    assert!(String::from_utf8_lossy(&result.stderr).contains("symlink or special entry"));
    assert!(!evidence.exists());
    // selector 없는 fresh 경로는 legacy 원문으로 fallback하지 않는다.
    let mut fresh = args();
    fresh.truncate(fresh.len() - 4);
    fresh.extend([
        "--original".into(),
        reference.into_os_string(),
        "--python-prefix".into(),
        root.into_os_string(),
        "--capture-sha256".into(),
        "0".repeat(64).into(),
    ]);
    let result = cli(fresh);
    assert_ne!(result.code, 0);
    assert!(!String::from_utf8_lossy(&result.stderr).contains("frozen"));
    assert!(!evidence.exists());
}

#[test]
#[ignore = "Actions의 고정 raw reference/current CLI/trace/numerics와 부모 전체 봉인이 필요: UNRUN"]
fn frozen_normal40_actual_cli_matches_legacy_reference() {
    let env = |name| std::env::var_os(name).expect("Actions frozen reference variable");
    let evidence = PathBuf::from(env("CONTROLSD_FROZEN_EVIDENCE"));
    assert!(!evidence.try_exists().unwrap());
    assert!(fs::symlink_metadata(&evidence).is_err());
    let mut spec = CommandSpec::new_os(
        vec![
            env!("CARGO_BIN_EXE_check-controlsd-native").into(),
            "--root".into(),
            env("CONTROLSD_CONTROLLER_ROOT"),
            "--trace".into(),
            env("CONTROLSD_CONTROLLER_TRACE"),
            "--numerics".into(),
            env("CONTROLS_NUMERICS"),
            "--evidence".into(),
            evidence.clone().into_os_string(),
            "--source-commit".into(),
            env("CONTROLSD_CONTROLLER_SOURCE_SHA"),
            "--frozen-reference".into(),
            controller::frozen::REFERENCE_ID.into(),
            "--reference-dir".into(),
            env("CONTROLSD_FROZEN_REFERENCE"),
        ],
        Duration::from_secs(700),
    );
    spec.disable_core_dump = true;
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout: Value = serde_json::from_slice(&result.stdout).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(evidence.join("results.json")).unwrap()).unwrap();
    assert_eq!(stdout, receipt);
    assert_eq!(receipt["pass"], true);
    assert_eq!(receipt["cases"], 40);
    assert_eq!(receipt["frames"], 8655);
    assert_eq!(receipt["numeric_comparisons"], 705948);
    assert!(receipt["max_absolute_error"].as_f64().unwrap().is_finite());
    let admission: Value =
        serde_json::from_slice(&fs::read(evidence.join("admission.json")).unwrap()).unwrap();
    let recipe: Value = serde_json::from_str(controller::frozen::REFERENCE_INDEX).unwrap();
    assert_eq!(admission["historical_reference"], recipe);
    assert_eq!(admission["reference_id"], controller::frozen::REFERENCE_ID);
    assert_eq!(
        admission["current_source_commit"],
        std::env::var("CONTROLSD_CONTROLLER_SOURCE_SHA").unwrap()
    );
    assert_eq!(
        admission["historical_original_provider_closure"],
        "not-recorded"
    );
    assert_eq!(
        admission["historical_original_provider_closure_complete"],
        false
    );
    assert_eq!(admission["historical_provider_paths_dereferenced"], false);
    assert_eq!(admission["original_capture_executed"], false);
    assert_eq!(admission["original_backend_execs"], 0);
    assert_eq!(admission["full_runtime_acceptance"], false);
    assert_eq!(admission["inventory_credit"], false);
    // controller가 보존한 source는 원문 그대로여야 한다. 기대값 재직렬화는 허용하지 않는다.
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(fs::read(evidence.join("source.json")).unwrap())
        ),
        recipe["members"][1]["sha256"].as_str().unwrap()
    );
}
