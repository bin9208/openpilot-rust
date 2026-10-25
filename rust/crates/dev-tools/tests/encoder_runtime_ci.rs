//! Mandatory native encoder workflow routing; no interpreter fixture bootstrap.
use openpilot_dev_tools::ci_policy::{
    parse, uses_native_action, validate_local_routing, HOSTED_RUNNER, LOCAL_RUNNER, TRUSTED_ALWAYS,
    TRUSTED_SOURCE,
};
use serde_json::Value;
use std::{fs, path::Path};
fn workflow() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    parse(&fs::read_to_string(root.join(".github/workflows/rust.yml")).unwrap()).unwrap()
}
fn steps(job: &Value) -> &Vec<Value> {
    job["steps"].as_array().unwrap()
}
fn runs(job: &Value) -> String {
    steps(job)
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn hosted_job(job: &Value) {
    assert_eq!(job["runs-on"], serde_json::json!(HOSTED_RUNNER));
    assert!(job.get("if").is_none());
    assert!(job.get("continue-on-error").is_none());
}
#[test]
fn encoder_host_rejects_legacy_or_unapproved_routing() {
    let workflow = workflow();
    let job = &workflow["jobs"]["encoder-runtime"];
    hosted_job(job);
    for host in [
        serde_json::json!(LOCAL_RUNNER),
        serde_json::json!("ubuntu-latest"),
        serde_json::json!("windows-latest"),
        serde_json::json!("${{ matrix.runner }}"),
        serde_json::json!([HOSTED_RUNNER]),
    ] {
        let mut bad = job.clone();
        bad["runs-on"] = host;
        assert!(std::panic::catch_unwind(|| hosted_job(&bad)).is_err());
    }
}
#[test]
fn encoder_host_job_uses_native_checkers_without_python_bootstrap() {
    let workflow = workflow();
    let job = &workflow["jobs"]["encoder-runtime"];
    let commands = runs(job);
    hosted_job(job);
    validate_local_routing(&workflow).unwrap();
    assert_eq!(job["env"]["CARGO_BUILD_JOBS"], "2");
    assert_eq!(job["env"]["CARGO_INCREMENTAL"], "0");
    // 실제 첫 source/fork 경계는 위 validate_local_routing으로 먼저 검증한다.
    for step in steps(job).iter().skip(1) {
        assert!(!step["uses"].as_str().unwrap_or("").contains("setup-python"));
        assert!(step.get("continue-on-error").is_none());
        if step.get("run").is_some()
            && !matches!(
                step["name"].as_str().unwrap_or(""),
                "Seal private source and index after maintained checks"
                    | "Clean only uploaded owned source alias and target"
            )
        {
            assert!(
                step["if"] == TRUSTED_SOURCE,
                "mandatory encoder stage gated: {}",
                step["name"]
            );
        }
    }
    for forbidden in [
        "build_visionipc_python.py",
        "check_encoder_runtime.py",
        "check_encoder_ci.py",
        "test_encoder_ci.py",
        "test_encoder_outcomes.py",
        "ENCODER_MSGQ_PYTHON",
        "PYTHONPATH",
        "pycapnp",
        "pyzmq",
        "Cython",
        "setuptools",
        "zstandard==",
        "python -m pip",
        "pip install",
        "python -P",
        "python -c",
        "python3 ",
        "python ",
    ] {
        assert!(
            !commands.contains(forbidden),
            "encoder job retains interpreter path {forbidden}"
        );
        assert!(job["env"].get(forbidden).is_none());
    }
    assert!(job["env"].get("PYTHONPATH").is_none());
    assert!(job["env"].get("ENCODER_MSGQ_PYTHON").is_none());
    let check = steps(job)
        .iter()
        .filter_map(|s| s["run"].as_str())
        .filter(|s| s.contains("rust/target/debug/check-encoder-ci "))
        .collect::<Vec<_>>();
    assert_eq!(check.len(), 1, "one complete native CI execution required");
    for argument in [
        "--json11-stager \"$JSON11_STAGER\"",
        "--binaries rust/target/debug",
        "--build-messages \"$RUNNER_TEMP/encoder-native/build.jsonl\"",
        "--output \"$RUNNER_TEMP/encoder-native/oracles\"",
    ] {
        assert!(check[0].contains(argument), "missing {argument}");
    }
    assert!(
        !check[0].contains("--case"),
        "full default ten runtime cases required"
    );
    assert!(steps(job)
        .iter()
        .any(|s| uses_native_action(job, s, "native-json11")));
}
#[test]
fn encoder_host_retains_all_native_tests_build_provenance_and_strict_checks() {
    let workflow = workflow();
    let job = &workflow["jobs"]["encoder-runtime"];
    let commands = runs(job);
    for required in [
        "-p openpilot-encoderd -p openpilot-msgq --features openpilot-encoderd/native",
        "--features native --bins --examples --locked --message-format=json",
        "tee \"$RUNNER_TEMP/encoder-native/build.jsonl\"",
        "--test encoder_stage",
        "--test encoder_codecs_check",
        "--test encoder_v4l_check",
        "--test encoder_target_check",
        "--test encoder_runtime_wire",
        "--test encoder_runtime_exercise",
        "--test encoder_runtime_check",
        "--test encoder_runtime_close",
        "--test encoder_runtime_shutdown",
        "--test encoder_runtime_native",
        "--test encoder_ci_check",
        "--test encoder_runtime_ci",
        "--bin build-encoder-source",
        "--bin check-encoder-codecs",
        "--bin check-encoder-v4l",
        "--bin check-encoder-target",
        "--bin check-encoder-runtime",
        "--bin check-encoder-ci",
        "--features encoder-runtime-native",
        "-- -D warnings",
    ] {
        assert!(
            commands.contains(required),
            "missing native encoder coverage {required}"
        );
    }
    for command in commands
        .lines()
        .filter(|l| l.trim_start().starts_with("cargo "))
    {
        assert!(
            command.contains("--locked"),
            "unlocked encoder build: {command}"
        );
    }
    let build = commands
        .find("--features native --bins --examples --locked --message-format=json")
        .unwrap();
    let native = commands
        .find("rust/target/debug/check-encoder-ci ")
        .unwrap();
    assert!(
        build < native,
        "Cargo current-artifact JSON must precede source comparison"
    );
    // Receipt validation is behavioral in encoder_ci_check against unchanged
    // original full-flow histories: codec5, V4L32, runtime10. The workflow must
    // dispatch the entire checker, never a substituted partial child command.
    assert!(commands.contains("build.log"));
    assert!(commands.contains("set -o pipefail") || commands.contains("set -euo pipefail"));
    let artifact = steps(job)
        .iter()
        .find(|s| {
            s["uses"]
                .as_str()
                .is_some_and(|v| v.starts_with("actions/upload-artifact@"))
        })
        .unwrap();
    assert_eq!(artifact["if"], TRUSTED_ALWAYS);
    for path in [
        "encoder-native/",
        "openpilot-encoderd",
        "encoder_codec_trace",
        "encoder_v4l_trace",
    ] {
        assert!(artifact["with"]["path"].as_str().unwrap().contains(path));
    }
    // The repository's required aggregate may be renamed, so inspect all jobs.
    assert!(workflow["jobs"]
        .as_object()
        .unwrap()
        .values()
        .any(|j| j["needs"]
            .as_array()
            .is_some_and(|n| n.iter().any(|v| v == "encoder-runtime"))));
}
#[test]
fn encoder_native_allocations_keep_capacity_guards_and_host_dependencies() {
    let workflow = workflow();
    let job = &workflow["jobs"]["encoder-runtime"];
    let commands = runs(job);
    for package in [
        "g++",
        "cmake",
        "pkg-config",
        "capnproto",
        "libcapnp-dev",
        "libavcodec-dev",
        "libavformat-dev",
        "libavutil-dev",
        "libswscale-dev",
        "libyuv-dev",
        "libzmq3-dev",
        "ffmpeg",
    ] {
        assert!(
            commands.contains(package),
            "missing native host prerequisite {package}"
        );
    }
    for step in steps(job) {
        let Some(run) = step["run"].as_str() else {
            continue;
        };
        let lines = run.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let line = line.trim();
            if line.starts_with("cargo ")
                || line.starts_with("rustup ")
                || line.contains("apt-get -o ") && line.contains(" install ")
            {
                assert!(
                    lines[..index]
                        .iter()
                        .rev()
                        .take_while(|p| !p.trim_start().starts_with("cargo ")
                            && !p.trim_start().starts_with("rustup "))
                        .any(
                            |p| p.contains("\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\"")
                                && p.contains("--growth-mib")
                        ),
                    "unguarded encoder allocation: {line}"
                );
            }
        }
    }
    assert!(commands.contains("--growth-mib 8192"));
    assert!(commands.contains("--growth-mib 4096"));
    assert!(commands.contains("--growth-mib 2048"));
    assert!(commands.contains("--growth-mib 1024"));
    let action = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.github/actions/native-json11/action.yml"),
    )
    .unwrap();
    assert!(!action.contains("setup-python"));
    assert!(!action.contains("pip install"));
    assert!(action.contains("stage-json11"));
}
