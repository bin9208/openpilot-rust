//! Assertion-for-assertion native migration of test_rust_isolation.py.
use openpilot_dev_tools::ci_policy::{
    parse, run_shell, validate_local_routing, validate_rust_gate, DARWIN_RUNNER, HOSTED_RUNNER,
    TRUSTED_ALWAYS, TRUSTED_SOURCE,
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap()
}
fn workflow() -> Value {
    parse(&read(".github/workflows/rust.yml")).unwrap()
}
#[test]
fn stable_clippy_jobs_install_component_before_first_use() {
    let invocation = regex::Regex::new(r"\bcargo(?:\s+\+1\.94\.0)?\s+clippy\b").unwrap();
    let main = workflow();
    for (name, job) in main["jobs"].as_object().unwrap() {
        if name == "native-arm" {
            // 이 caller의 실제 Clippy provider는 callee의 정확8 fresh SDK다.
            // main/callee/SDK before-after 및 설치·검증 순서 전체를 요구한다.
            let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
            openpilot_dev_tools::ci_policy::validate_native_arm_reuse(&main, &native).unwrap();
            continue;
        }
        let run = commands(job);
        let lines = run.lines().collect::<Vec<_>>();
        let Some(first) = lines.iter().position(|line| invocation.is_match(line)) else {
            continue;
        };
        assert!(
            lines[..first].iter().any(|line| {
                line.contains("rustup ")
                    && line.contains("1.94.0")
                    && line.contains("clippy")
                    && (line.contains("--component ") || line.contains("component add "))
            }),
            "{name}: Clippy must be explicitly installed before its first use"
        );
    }
}
#[test]
fn dbc_native_tools_are_built_before_every_required_consumer() {
    for (path, test) in [
        (
            "rust/crates/dev-tools/tests/dbc_generator.rs",
            "original_raw_os_paths_and_filename_encoding_error_preserve_partial_bytes",
        ),
        (
            "rust/crates/dev-tools/tests/dbc_generator_cli.rs",
            "json_error_boundary_retains_original_io_and_unicode_raw_objects_and_partial_outputs",
        ),
    ] {
        assert!(read(path).contains(&format!(
            "#[cfg(target_os = \"linux\")]\n#[test]\nfn {test}"
        )));
    }
    required(
        &read("rust/crates/dev-tools/tests/dbc_generator_cli.rs"),
        &[
            "#[cfg(target_os = \"macos\")]\n    let source = temp.path().join(\"source-한\");",
            "Some(\"invalid-main-utf8\" | \"invalid-include-utf8\")",
        ],
    );
    let data = workflow();
    assert_eq!(
        data["jobs"]["dbc-darwin"]["uses"],
        "./.github/workflows/rust-dbc-darwin.yml"
    );
    let native = parse(&read(".github/workflows/rust-dbc-darwin.yml")).unwrap();
    let darwin = &native["jobs"]["dbc-darwin"];
    absent(darwin, &["if", "continue-on-error"]);
    assert_eq!(darwin["runs-on"], serde_json::json!(DARWIN_RUNNER));
    for (key, expected) in [
        ("RUSTUP_TOOLCHAIN", "1.94.0"),
        ("CARGO_BUILD_JOBS", "2"),
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_PROFILE_DEV_DEBUG", "0"),
        ("CARGO_PROFILE_TEST_DEBUG", "0"),
    ] {
        assert_eq!(darwin["env"][key], expected, "Darwin {key}");
    }
    let darwin_steps = darwin["steps"].as_array().unwrap();
    let bootstrap_steps = darwin_steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step["run"]
                .as_str()
                .unwrap_or("")
                .contains("rust/tools/build-dbc-generator")
        })
        .collect::<Vec<_>>();
    assert_eq!(bootstrap_steps.len(), 2, "reserve before build and tests");
    for (_, step) in &bootstrap_steps {
        trusted_step(step);
    }
    let build_run = bootstrap_steps[0].1["run"].as_str().unwrap();
    required(
        build_run,
        &[
            "lipo -archs",
            "program-outputs",
            "rustup run 1.94.0 rustc -vV",
        ],
    );
    let test_run = bootstrap_steps[1].1["run"].as_str().unwrap();
    required(
        test_run,
        &[
            "rustup run 1.94.0 cargo test",
            "--features dbc-generator-native --test dbc_generator --test dbc_generator_cli",
            "--locked --target aarch64-apple-darwin",
            "-j2",
        ],
    );
    assert!(
        test_run.find("rust/tools/build-dbc-generator").unwrap()
            < test_run.find("cargo test").unwrap()
    );
    let reclaim = darwin_steps
        .iter()
        .position(|step| step["run"].as_str().unwrap_or("").contains("selected_app"))
        .unwrap();
    assert!(reclaim < bootstrap_steps[0].0);
    required(
        darwin_steps[reclaim]["run"].as_str().unwrap(),
        &[
            "test \"$(uname -s)\" = Darwin",
            "test \"$(uname -m)\" = arm64",
            "xcode-select -p",
            "$application -ef $selected_app",
            "2 * 1024 * 1024",
            "xcrun --find clang",
        ],
    );
    trusted_step(&darwin_steps[reclaim]);
    assert!(data["jobs"]["fast"]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|job| job == "dbc-darwin"));
    assert_eq!(
        data["jobs"]["fast"]["steps"][0]["env"]["DBC_DARWIN"],
        "${{ needs.dbc-darwin.result }}"
    );
    required(
        data["jobs"]["fast"]["steps"][0]["run"].as_str().unwrap(),
        &["test \"$DBC_DARWIN\" = success"],
    );
    let integration = parse(&read(".github/workflows/tests.yaml")).unwrap();
    let build = &integration["jobs"]["build_release"];
    absent(build, &["if", "continue-on-error"]);
    let steps = build["steps"].as_array().unwrap();
    let setup = steps
        .iter()
        .position(|step| step["run"] == "./tools/op.sh setup")
        .unwrap();
    let export = steps
        .iter()
        .position(|step| {
            step["run"]
                .as_str()
                .unwrap_or("")
                .contains("OPENPILOT_DBC_GENERATOR=")
        })
        .unwrap();
    let reserve = steps
        .iter()
        .position(|step| {
            step["run"]
                .as_str()
                .unwrap_or("")
                .contains("check-disk-space")
        })
        .unwrap();
    assert!(reserve < setup && setup < export);
    for (index, step) in steps.iter().enumerate() {
        if step["run"].as_str().unwrap_or("").starts_with("scons") {
            assert!(
                export < index,
                "SCons evaluates the native registry immediately"
            );
        }
    }
    for index in [reserve, setup, export] {
        trusted_step(&steps[index]);
    }
    required(
        steps[export]["run"].as_str().unwrap(),
        &[
            "test -x \"$GITHUB_WORKSPACE/rust/target/debug/dbc-generator\"",
            "OPENPILOT_DBC_GENERATOR=%s/rust/target/debug/dbc-generator",
            "GITHUB_ENV",
        ],
    );
    let setup_source = read("tools/op.sh");
    required(
        &setup_source,
        &[r#""$OPENPILOT_ROOT/rust/tools/build-dbc-generator" >/dev/null"#],
    );
    let bootstrap = read("rust/tools/build-dbc-generator");
    required(
        &bootstrap,
        &[
            "rustup run 1.94.0 cargo build",
            "--features dbc-generator-native --bin dbc-generator --locked",
            "--target \"$native_host\"",
            "CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2",
            "-u CARGO_BUILD_TARGET",
            "reserve \"$target\" 2048",
        ],
    );
    assert!(
        bootstrap.find("reserve \"$target\" 2048").unwrap()
            < bootstrap.find("cargo build").unwrap()
    );
    for (job_name, consumers) in [
        ("workspace", &["python tools/stage_card_fixtures.py"][..]),
        (
            "card-runtime",
            &["python rust/tools/stage_card_fixtures.py"][..],
        ),
        (
            "radar-runtime",
            &["rust/target/debug/prepare-radarcan-assets \""][..],
        ),
        (
            "radar-arm",
            &["rust/target/debug/prepare-radarcan-assets \""][..],
        ),
        (
            "controls-runtime",
            &["rust/target/debug/stage-control-dbcs"][..],
        ),
    ] {
        let actual = native_functional_job(&data, job_name);
        let job = &actual;
        absent(job, &["if", "continue-on-error"]);
        let run = commands(job);
        let build = run
            .find("--features dbc-generator-native --bin dbc-generator")
            .unwrap();
        for consumer in consumers {
            let call = run.find(consumer).unwrap();
            assert!(build < call, "{job_name}: native build follows {consumer}");
        }
        required(&run, &["OPENPILOT_DBC_GENERATOR=", "GITHUB_ENV"]);
        if ["workspace", "card-runtime"].contains(&job_name) {
            required(
                &run,
                &[
                    "export OPENPILOT_CARD_ASSET_PREPARER=",
                    "--bin prepare-card-assets",
                ],
            );
        }
        mandatory(
            job,
            &[
                "dbc-generator-native",
                "stage_card_fixtures.py",
                "prepare-radarcan-assets",
                "stage-control-dbcs",
            ],
        );
        for retired in [
            "radarcan_prepare_assets.py",
            "card_prepare_assets.py",
            "stage_control_dbcs.py",
        ] {
            assert!(!run.contains(retired), "{job_name}: retired {retired}");
        }
    }
    let steps = data["jobs"]["workspace"]["steps"].as_array().unwrap();
    let checks = steps
        .iter()
        .find(|step| step["name"] == "Require native DBC and Version source contracts")
        .unwrap();
    trusted_step(checks);
    let run = checks["run"].as_str().unwrap();
    required(
        run,
        &[
            "cargo test --locked",
            "cargo clippy --locked",
            "--features dbc-generator-native,version-tools-native",
            "-- -D warnings",
        ],
    );
    let tokens = run.split_whitespace().collect::<Vec<_>>();
    for target in [
        "dbc_generator",
        "dbc_generator_cli",
        "dbc_assets",
        "dbc_assets_cli",
        "dbc_assets_raw_names",
        "dbc_bootstrap",
        "version_source",
    ] {
        assert_eq!(
            tokens
                .windows(2)
                .filter(|pair| *pair == ["--test", target])
                .count(),
            2,
            "{target}"
        );
    }
}
#[test]
fn workspace_source_tests_use_the_real_msgq_binding_before_importing_oracles() {
    let config = workflow();
    let steps = config["jobs"]["workspace"]["steps"].as_array().unwrap();
    let builders = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            step["run"].as_str().unwrap_or("").contains(
                "python tools/build_msgq_python.py --output \"$RUNNER_TEMP/rust-msgq-python\"",
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(builders.len(), 1, "build the shared original binding once");
    let (index, step) = steps
        .iter()
        .enumerate()
        .find(|(_, step)| {
            step["run"]
                .as_str()
                .unwrap_or("")
                .contains("--confcutdir=tools/tests tools/tests -q")
        })
        .unwrap();
    assert!(
        builders[0].0 < index,
        "real msgq binding must precede source imports"
    );
    assert!(step["run"]
        .as_str()
        .unwrap()
        .contains("PYTHONPATH=\"$RUNNER_TEMP/rust-msgq-python:$PYTHONPATH\" python -m pytest"));
    trusted_step(builders[0].1);
    trusted_step(step);
}
#[test]
fn workspace_requires_pinned_llvm20_graph_and_primitive_regressions() {
    let data = workflow();
    let job = &data["jobs"]["model-llvm20"];
    let steps = job["steps"].as_array().unwrap();
    let find = |name: &str| {
        steps
            .iter()
            .enumerate()
            .find(|(_, step)| step["name"] == name)
            .unwrap_or_else(|| panic!("missing required {name}"))
    };
    let (stage_index, stage) = find("Stage independently captured LLVM20 provider");
    trusted_step(stage);
    let stage_run = stage["run"].as_str().unwrap();
    required(
        stage_run,
        &[
            "--growth-mib 256",
            "libllvm20_20.1.2-0ubuntu1%7e24.04.3_amd64.deb",
            "f018e71c72c7a4742fb1aff9dc98b45586f42e6ea464d4918af7e3772b7f0658",
            "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a",
            "sha256sum --check --strict",
            "dpkg-deb --extract",
            "ldd",
        ],
    );
    assert!(
        !stage_run.contains("GITHUB_ENV"),
        "retain LLVM18 for original source comparisons"
    );
    for (name, targets) in [
        (
            "Require native LLVM20 graph and binding equivalence",
            &[
                "compiler_batched_matmul",
                "compiler_batched_projection",
                "compiler_batched_bias",
                "compiler_batched_silu",
                "compiler_batched_silu_chain",
                "compiler_batched_silu_chain_bias",
                "compiler_original_node280",
                "compiler_matrix_alias",
                "compiler_gemm_pipeline",
                "compiler_gemm_chain",
                "compiler_gemm_chain_pipeline",
                "compiler_gemm_branch_head",
                "compiler_gemm_normalized_chain",
                "compiler_gemm_packed_normalized",
                "compiler_gemm_norm_indexed",
                "compiler_contraction_target",
                "compiler_parallel_conv_layout",
                "compiler_input_bindings",
                "compiler_concat_slice",
                "compiler_concat_slice_cpu",
            ][..],
        ),
        (
            "Require native LLVM20 primitive and provider equivalence",
            &[
                "compiler_llvm",
                "compiler_conv_depthwise",
                "compiler_conv_pointwise",
                "compiler_normalization_row",
                "compiler_gelu_layout",
                "compiler_residual_scale",
                "compiler_residual_scale_rounding",
                "compiler_gemm_producer",
                "compiler_gemm_residual",
                "compiler_gemm_terminal",
            ][..],
        ),
    ] {
        let (index, step) = find(name);
        assert!(stage_index < index);
        trusted_step(step);
        let run = step["run"].as_str().unwrap();
        required(run, &[
            "LLVM_PATH=\"$RUNNER_TEMP/llvm20-captured/usr/lib/x86_64-linux-gnu/libLLVM.so.20.1\"",
            "cargo +1.94.0 test --locked -p openpilot-model-runtime",
            "profile.test.package.sha2.opt-level=3",
            "profile.dev.package.sha2.opt-level=3",
            "--test-threads=2",
            "check-disk-space",
        ]);
        for target in targets {
            assert!(
                run.contains(&format!("--test {target} ")),
                "missing {target}"
            );
        }
    }
    required(&commands(&data["jobs"]["workspace"]), &[
        "cargo test --workspace --locked --config profile.test.package.sha2.opt-level=3",
        "--test compiler_original_node280 -- --nocapture --test-threads=2",
        "cargo clippy --workspace --all-targets --locked --config profile.test.package.sha2.opt-level=3",
    ]);
}
#[test]
fn llvm20_has_an_independent_mandatory_budget_without_removing_workspace_checks() {
    let data = workflow();
    let job = &data["jobs"]["model-llvm20"];
    assert!(
        job.is_object(),
        "LLVM20 must not consume the workspace job's remaining budget"
    );
    absent(job, &["needs", "if", "continue-on-error"]);
    assert_eq!(job["runs-on"], serde_json::json!(HOSTED_RUNNER));
    assert_eq!(job["timeout-minutes"], "90");
    assert_eq!(job["env"]["CARGO_BUILD_JOBS"], "2");
    assert_eq!(job["env"]["CARGO_INCREMENTAL"], "0");
    let run = commands(job);
    required(
        &run,
        &[
            "rustup toolchain install 1.94.0 --profile minimal",
            "git lfs pull --include=openpilot/selfdrive/modeld/models/driving_supercombo.onnx",
            "--test compiler_batched_silu_chain_bias",
            "--test compiler_original_node280",
            "--test compiler_gemm_terminal",
            "--test-threads=2",
        ],
    );
    assert!(data["jobs"]["fast"]["needs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "model-llvm20"));
    assert_eq!(
        data["jobs"]["fast"]["steps"][0]["env"]["LLVM20"],
        "${{ needs.model-llvm20.result }}"
    );
    required(
        &commands(&data["jobs"]["fast"]),
        &["test \"$LLVM20\" = success"],
    );
    let workspace = commands(&data["jobs"]["workspace"]);
    required(
        &workspace,
        &[
            "cargo test --workspace --locked",
            "cargo build --workspace --release --locked",
        ],
    );
    assert!(
        !workspace.contains("llvm20-captured"),
        "LLVM20 must run exactly once in its own required job"
    );
}
// Keep the existing domain command assertions about their original steps.
// The new ownership/transport scaffolding is validated separately below.
fn private_infrastructure(step: &Value) -> bool {
    matches!(
        step["name"].as_str().unwrap_or(""),
        "Allocate private source cache target and local action alias before checkout"
            | "Admit exact private source and index"
            | "Configure source-dependent job environment on private paths"
            | "Keep original Python installations private"
            | "Seal private source and index after maintained checks"
            | "Clean only uploaded owned source alias and target"
    )
}
fn private_local_action(step: &Value, name: &str) -> bool {
    step["uses"].as_str().is_some_and(|value| {
        value.starts_with("./.ci296-source-")
            && value.ends_with(&format!("/source/.github/actions/{name}"))
    })
}

fn native_functional_job(main: &Value, name: &str) -> Value {
    if !["radar-arm", "carrot-navi-arm"].contains(&name) {
        return main["jobs"][name].clone();
    }
    let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
    openpilot_dev_tools::ci_policy::validate_native_arm_reuse(main, &native).unwrap();
    native["jobs"][name].clone()
}

fn commands(job: &Value) -> String {
    if job["needs"] == serde_json::json!(["native-arm"]) {
        let main = workflow();
        let name = ["radar-arm", "carrot-navi-arm"]
            .into_iter()
            .find(|name| job == &main["jobs"][*name])
            .expect("unknown native ARM result proxy");
        return commands(&native_functional_job(&main, name));
    }
    if let Some(caller) = job["uses"].as_str() {
        if caller == "./.github/workflows/rust-native-arm.yml" {
            let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
            return openpilot_dev_tools::ci_policy::NATIVE_ARM_BINDINGS
                .iter()
                .map(|(name, _)| commands(&native["jobs"][*name]))
                .collect::<Vec<_>>()
                .join("\n");
        }
        assert_eq!(caller, "./.github/workflows/rust-dbc-darwin.yml");
        let workflow = parse(&read(".github/workflows/rust-dbc-darwin.yml")).unwrap();
        let actual = &workflow["jobs"]["dbc-darwin"];
        assert!(
            actual.get("uses").is_none(),
            "Darwin caller must resolve once to native steps"
        );
        return commands(actual);
    }
    job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| !private_infrastructure(step))
        .map(|s| s["run"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}
fn required(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "missing {needle}");
    }
}
fn absent(value: &Value, keys: &[&str]) {
    for key in keys {
        assert!(value.get(*key).is_none(), "unexpected {key}: {value}");
    }
}
fn trusted_step(step: &Value) {
    assert_eq!(step["if"], TRUSTED_SOURCE);
    absent(step, &["continue-on-error"]);
}
fn mandatory(job: &Value, selectors: &[&str]) {
    absent(job, &["if"]);
    // 첫 단계는 조건 없이 fork를 거부한다. 실제 경계를 검증한 뒤 도메인 조건과 구분한다.
    validate_local_routing(&serde_json::json!({"jobs":{"runtime":job}})).unwrap();
    for step in job["steps"].as_array().unwrap().iter().skip(1) {
        if !private_infrastructure(step)
            && step.get("run").is_some()
            && selectors
                .iter()
                .any(|s| step["run"].as_str().unwrap_or("").contains(s))
        {
            assert_eq!(step["if"], TRUSTED_SOURCE);
            absent(step, &["continue-on-error"]);
        }
    }
}
#[test]
fn mandatory_domain_steps_preserve_the_unconditional_fork_boundary() {
    let data = workflow();
    let original = &data["jobs"]["webrtc-runtime"];
    mandatory(original, &[""]);
    for mutation in [
        "skipped-boundary",
        "missing-trust",
        "commented-refusal",
        "unguarded-domain",
    ] {
        let mut job = original.clone();
        match mutation {
            "skipped-boundary" => job["steps"][0]["if"] = TRUSTED_SOURCE.into(),
            "missing-trust" => {
                job["steps"][0]["env"]
                    .as_object_mut()
                    .unwrap()
                    .remove("SOURCE_ALLOWED");
            }
            "commented-refusal" => {
                job["steps"][0]["run"] =
                    format!("# {}", job["steps"][0]["run"].as_str().unwrap()).into();
            }
            "unguarded-domain" => {
                let step = job["steps"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|step| {
                        !private_infrastructure(step)
                            && step["run"]
                                .as_str()
                                .is_some_and(|run| run.contains("cargo test"))
                    })
                    .unwrap();
                step.as_object_mut().unwrap().remove("if");
            }
            _ => unreachable!(),
        }
        assert!(
            std::panic::catch_unwind(|| mandatory(&job, &[""])).is_err(),
            "accepted {mutation}"
        );
    }
}
fn artifacts(job: &Value) {
    let selected = job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["id"] != "ci296_private_evidence"
                && s["uses"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with("actions/upload-artifact@")
        })
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["if"], TRUSTED_ALWAYS);
}
fn some_artifact(job: &Value) {
    assert!(job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["if"] == TRUSTED_ALWAYS
            && s["uses"]
                .as_str()
                .unwrap_or("")
                .starts_with("actions/upload-artifact")));
}
fn job_check(name: &str, needles: &[&str], selectors: &[&str], artifact: bool) {
    let data = workflow();
    let job = &data["jobs"][name];
    mandatory(job, selectors);
    required(&commands(job), needles);
    if artifact {
        artifacts(job);
    }
}
#[test]
fn webcam_requires_two_architectures_and_real_capture_ipc() {
    let data = workflow();
    let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
    openpilot_dev_tools::ci_policy::validate_native_arm_reuse(&data, &native).unwrap();
    let arm = &native["jobs"]["webcam-runtime"];
    mandatory(arm, &["python -P rust/tools/webcam_"]);
    some_artifact(arm);
    let job = &data["jobs"]["webcam-runtime"];
    assert_eq!(
        job["strategy"]["matrix"]["runner"],
        serde_json::json!(["local-x64"])
    );
    for (key, value) in [
        ("CARGO_BUILD_JOBS", "2"),
        ("CARGO_INCREMENTAL", "0"),
        ("CARGO_PROFILE_DEV_DEBUG", "0"),
    ] {
        assert_eq!(job["env"][key], value);
    }
    job_check(
        "webcam-runtime",
        &[
            "webcam_source_requirements.txt",
            r#"sudo apt-get -o "APT::Architectures=$(dpkg --print-architecture)" install -y g++ clang-18 libclang-18-dev cmake pkg-config file capnproto"#,
            "build-xiaoge-opencv -- --prepare-only",
            "build-webcam-opencv",
            "build_visionipc_python.py",
            "libswscale-dev=7:6.1.1-*",
            "pkg-config --modversion libswscale",
            "openpilot-webcam --features native",
            "webcam_camera_compare.py",
            "webcam_worker_compare.py",
            "odd-height unaligned-even",
            "webcam_cli_compare.py",
            "webcam_capture_errors.py",
            "webcam_capture_fault.cc",
        ],
        &["python -P rust/tools/webcam_"],
        true,
    );
}
#[test]
fn athena_requires_complete_native_runtime_and_codec_checks() {
    job_check(
        "athena-runtime",
        &[
            "openpilot-athena",
            "openpilot-process-supervision",
            "check_athena_runtime.py",
            "rust/target/debug/check-jpeg-sanitizers",
        ],
        &[
            "check_athena_runtime.py",
            "rust/target/debug/check-jpeg-sanitizers",
        ],
        true,
    );
}
#[test]
fn ui_connectivity_requires_source_render_and_private_protocol_checks() {
    let caller = read("rust/tools/check_bluetooth_runtime.py");
    required(
        &caller,
        &[
            "--input-decode-checker",
            "if name == 'input_decode':",
            "command = [str(input_checker), '--binary'",
            "binary / 'check-bluetooth-input-decode'",
        ],
    );
    assert!(!caller.contains("check_bluetooth_input_decode.py"));
    required(
        &caller,
        &["binary / 'check-bluetooth-files'", "elif name == 'files':"],
    );
    assert!(!caller.contains("check_bluetooth_files.py"));
    required(
        &caller,
        &[
            "binary / 'check-bluetooth-config'",
            "elif name == 'config':",
        ],
    );
    assert!(!caller.contains("check_bluetooth_config.py"));
    assert!(!caller.contains("check_bluetooth_journal.py"));
    assert!(caller.contains("str(journal_checker)"));
    let config = workflow();
    let native = commands(&config["jobs"]["ui-connectivity"]);
    for retired in [
        "BLUETOOTH_CONFIG_PYTHON=",
        "BLUETOOTH_JOURNAL_PYTHON=",
        "python rust/tools/check_bluetooth_journal.py --binary",
        "from bluetooth_gesture_cases import cases",
    ] {
        assert!(
            !native.contains(retired),
            "retired helper path remains: {retired}"
        );
    }
    let ownership_tests = "cargo test --manifest-path rust/Cargo.toml -p openpilot-bluetooth -p openpilot-wifi --all-targets --locked";
    let launcher_env = "export WIFI_PROCESS_CHILD_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/openpilot-process-child\"";
    required(&native, &[ownership_tests, launcher_env,
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-bluetooth -p openpilot-wifi --all-targets --locked -- -D warnings"]);
    assert!(
        native.find("--bins --examples --locked").unwrap() < native.find(launcher_env).unwrap()
    );
    assert!(native.find(launcher_env).unwrap() < native.find(ownership_tests).unwrap());
    let environment = &config["jobs"]["ui-connectivity"]["env"];
    for key in ["CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"] {
        assert_eq!(environment[key], "0", "unbounded UI debug artifacts: {key}");
    }
    job_check(
        "ui-connectivity",
        &[
            "xvfb-run",
            "-s '-screen 0 2160x1080x24 -noreset'",
            "rust/target/debug/check-ui-framework --python python --target",
            "check_startup_ui.py",
            "rust/target/debug/check-ui-emoji --binary",
        "-p openpilot-dev-tools --bin check-ui-translations --bin check-ui-fps --bin check-ui-stack --bin check-ui-widgets --bin check-ui-navigation --bin check-ui-scroll --bin check-ui-scroller --features egl-reference,polygon-reference,ui-application-lifecycle-native --bin check-ui-egl --bin ui-egl-reference --bin check-ui-polygon --bin ui-polygon-reference --bin check-ui-diagnostics --bin ui-diagnostic-reference --bin build-ui-diagnostic-reference --bin check-ui-emoji --bin check-ui-forms --bin ui-forms-reference --bin check-ui-render --bin ui-render-reference --bin check-ui-network --bin ui-network-reference --bin check-ui-framework --bin check-ui-application --bin ui-application-reference --bin build-ui-application-reference --locked",
            "rust/target/debug/check-ui-translations --binary",
            "check_wifi_policy.py",
            "check_wifi_runtime.py",
            "check_wifi_callback_ordering.py",
            "--features bluetooth-sudo-fixture-native --bin bluetooth-sudo-fixture",
            "--features bluetooth-sudo-fixture-native --test bluetooth_sudo_fixture --locked -- --nocapture",
            "BLUETOOTH_SUDO_ORIGINAL_PYTHON=",
            "BLUETOOTH_SUDO_EVIDENCE=",
            "--features bluetooth-gesture-cases-native --bin generate-bluetooth-gesture-cases",
            "--features bluetooth-gesture-cases-native --test bluetooth_gesture_cases_generate --locked",
            "python rust/tools/check_bluetooth_gestures.py --binary",
            "cmp \"$proof/original/input.jsonl\" \"$proof/native-input.jsonl\"",
            "cmp \"$proof/original/native.jsonl\" \"$proof/native-result.jsonl\"",
            "cmp \"$proof/original/native.stderr\" \"$proof/native-result.stderr\"",
            "--features bluetooth-enumerate-native --bin check-bluetooth-enumerate",
            "--features bluetooth-enumerate-native --lib bluetooth_enumerate_check:: --locked",
            "--features bluetooth-enumerate-native --test bluetooth_enumerate_reference --locked -- --nocapture",
            "test ! -e rust/tools/check_bluetooth_enumerate.py",
            "BLUETOOTH_ENUMERATE_ABSENT_OUTPUT=",
            "--features bluetooth-bluez-policy-native --bin check-bluetooth-bluez-policy",
            "--features bluetooth-bluez-policy-native --test bluetooth_bluez_policy_check --locked",
            "--features bluetooth-bluez-policy-native --lib bluetooth_bluez_policy_check:: --locked",
            "--features bluetooth-bluez-policy-native --test bluetooth_bluez_policy_reference --locked -- --nocapture",
            "test ! -e rust/tools/check_bluetooth_bluez_policy.py",
            "BLUEZ_POLICY_ABSENCE_PROOF=",
            "--features bluetooth-config-native --bin check-bluetooth-config",
            "--features bluetooth-config-native --test bluetooth_config_check --locked",
            "--features bluetooth-config-native --lib --bin check-bluetooth-config",
            "rust/target/debug/check-bluetooth-config\" --binary",
            "--features bluetooth-files-native --bin check-bluetooth-files",
            "--features bluetooth-files-native --test bluetooth_files_check --locked",
            "checker=\"$GITHUB_WORKSPACE/rust/target/debug/check-bluetooth-files\"",
            "--features bluetooth-journal-native --bin check-bluetooth-journal",
            "--features bluetooth-journal-native --test bluetooth_journal_check --locked",
            "rust/target/debug/check-bluetooth-journal --binary",
            "cargo build --manifest-path rust/Cargo.toml -p openpilot-wifi --lib --locked",
            "rustc --edition=2021 rust/tools/wifi_snapshot_allocation_probe.rs --extern",
            "\"$RUNNER_TEMP/wifi-snapshot-allocation-probe\" --require-improvement",
            "check_cweb_policy.py",
            "check_cweb_http.py",
            "check_cweb_daemon.py",
            "check_cweb_address.py",
            "cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features bluetooth-input-native --bin check-bluetooth-input-decode --locked",
            "BLUETOOTH_INPUT_FIXTURE=\"$PWD/rust/target/debug/examples/input_fixture\" cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features bluetooth-input-native --test bluetooth_input_check --locked",
            "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features bluetooth-input-native --lib --test bluetooth_input_check --bin check-bluetooth-input-decode --locked -- -D warnings",
            "build_params_python.py",
        ],
        &[
            "python rust/tools/check_",
            "--bin check-ui-translations",
            "rust/target/debug/check-ui-translations --binary",
        ],
        true,
    );
}
#[test]
fn estimators_require_original_models_loops_and_native_boundaries() {
    let data = workflow();
    let env = &data["jobs"]["estimation-runtime"]["env"];
    assert_eq!(env["CARGO_PROFILE_DEV_DEBUG"], "0");
    assert_eq!(env["CARGO_PROFILE_TEST_DEBUG"], "0");
    job_check(
        "estimation-runtime",
        &[
            "build_locationd_oracle.py",
            "build_paramsd_oracle.py",
            "build_params_python.py",
            "check_locationd.py",
            "check_locationd_loop.py",
            "check_locationd_timestamp.py",
            "check_locationd_daemon.py",
            "check_locationd_startup.py",
            "check_locationd_native.py",
            "check_paramsd.py",
            "--test paramsd_loop_retirement",
            "check_paramsd_daemon.py",
            "--bin check-paramsd-startup-native",
            "check_paramsd_native.py",
            "rust/target/debug/check-lagd-numeric ",
            "rust/target/debug/check-lagd-loop ",
            "rust/target/debug/check-lagd-cache ",
            "rust/target/debug/check-lagd-packet ",
            "rust/target/debug/check-lagd-daemon ",
            "rust/target/debug/check-lagd-params-io ",
            "pocketfft/native/kernel_test.cc",
            "-fsanitize=address,undefined",
        ],
        &["python rust/tools/check_"],
        false,
    );
    let lock: toml::Value = toml::from_str(&read("uv.lock")).unwrap();
    let eigen = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"].as_str() == Some("eigen"))
        .unwrap();
    let commit = eigen["source"]["git"]
        .as_str()
        .unwrap()
        .rsplit('#')
        .next()
        .unwrap();
    required(&read("rust/crates/dev-tools/src/eigen_stage.rs"), &[commit]);
    let data = workflow();
    for (name, architecture) in [("workspace", "x86_64"), ("arm64", "aarch64")] {
        let selected = data["jobs"][name]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| private_local_action(step, "native-eigen"))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        trusted_step(selected[0]);
        assert_eq!(selected[0]["with"]["architecture"], architecture);
    }
    for name in ["workspace", "arm64", "estimation-runtime"] {
        let run = commands(&data["jobs"][name]);
        assert!(!run.contains("subdirectory=eigen"));
        assert!(!run.contains("import eigen"));
    }
    let action = read(".github/actions/native-eigen/action.yml");
    required(
        &action,
        &[
            "--features eigen-tools-native --bin stage-eigen-headers --target",
            "--architecture \"$EIGEN_STAGE_ARCHITECTURE\"",
            "LOCATIOND_EIGEN_INCLUDE=",
            "PARAMSD_EIGEN_INCLUDE=",
            "sha256sum --check \"$proof/stager.sha256\"",
            "--growth-mib 2048",
            "--growth-mib 64",
        ],
    );
    assert!(!action.contains("python"));
}
#[test]
fn eigen_staging_requires_all_release_archives_and_original_header_comparison() {
    job_check(
        "estimation-runtime",
        &[
            "--features eigen-tools-native --bin stage-eigen-headers",
            "x86_64:linux_x86_64 aarch64:linux_aarch64 macos-arm64:macosx_11_0_arm64",
            "https://github.com/commaai/dependencies/releases/download/eigen/v3.4.0/",
            "EIGEN_NATIVE_INCLUDE=\"$proof/$architecture-native/install\"",
            "EIGEN_EVIDENCE=\"$proof/$architecture-comparison\"",
            "--lib eigen_stage::tests",
            "--features eigen-tools-native --test eigen_stage",
            "--lib --bin stage-eigen-headers --test eigen_stage -- -D warnings",
            "sha256sum --check \"$proof/stager.sha256\"",
            "sha256sum --check \"$proof/source.sha256\"",
            "LOCATIOND_EIGEN_INCLUDE=%s/x86_64-native/install",
            "PARAMSD_EIGEN_INCLUDE=%s/x86_64-native/install",
        ],
        &["--features eigen-tools-native"],
        true,
    );
}

#[test]
fn paramsd_atomic_foundation_preserves_the_original_independent_cases() {
    job_check(
        "estimation-runtime",
        &[
            "test ! -e rust/tools/tests/test_paramsd_atomic_gate.py",
            "test ! -L rust/tools/tests/test_paramsd_atomic_gate.py",
            "--features paramsd-startup-gate-native --test paramsd_startup_gate",
            "--features paramsd-startup-gate-native --lib --test paramsd_startup_gate -- -D warnings",
            "--ignored --exact native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown --nocapture",
            "PARAMSD_NATIVE_GATE_CAR_PARAMS=\"$proof/CarParams.bin\"",
            "sha256sum -c \"$proof/artifacts.sha256\"",
        ],
        &["paramsd_startup_gate", "PARAMSD_NATIVE_GATE_CAR_PARAMS"],
        false,
    );
}

#[test]
fn soundd_finalization_requires_original_behavioral_failure_and_retained_bindings() {
    job_check(
        "sensor-audio",
        &[
            "2d8d027932dc880bd0acd00288b256ac8d7f1b89",
            "mod finalization_433_red_tests",
            "baseline-original-source.sha256",
            "regression-only.patch",
            "CARGO_TARGET_DIR=\"$target\" cargo build",
            "CARGO_TARGET_DIR=\"$target\" cargo test",
            "180s cargo test",
            "--exact --nocapture",
            "test \"$status\" -eq 101",
            "grep -Fq 'test result: FAILED'",
            "grep -Fq 'accepted changed source at finalization'",
            "accepted changed source at finalization",
            "--lib soundd_daemon_check::worker::tests --locked -- --test-threads=1",
            "--test soundd_source_stage",
            "--bin stage-soundd-source --test soundd_source_stage",
            "--test soundd_daemon_check --test soundd_daemon_runtime --locked -- --test-threads=1",
            "source-stage.json",
            "SOUNDD_NATIVE_ABSENT_ROOT",
            "test ! -L \"$SOUNDD_NATIVE_ABSENT_ROOT/rust/tools/check_soundd_daemon.py\"",
            "sha256sum -c \"$proof/executables.sha256\"",
        ],
        &[
            "finalization-433",
            "soundd_daemon_check::worker::tests",
            "soundd_source_stage",
            "stage-soundd-source",
        ],
        true,
    );
    let source_stage_tests = read("rust/crates/dev-tools/tests/soundd_source_stage.rs");
    required(
        &source_stage_tests,
        &[
            "fn stages_exact_retained_manifest_sources_and_no_receipt_file()",
            "receipt[\"copied_files\"], 20",
            "receipt[\"source_sha256\"].as_object().unwrap().len(), 21",
            "assert!(!output.join(\"receipt.json\").exists())",
            "fn rejects_missing_or_changed_source_without_publishing_output()",
        ],
    );

    let job = &workflow()["jobs"]["sensor-audio"];
    let steps = job["steps"].as_array().unwrap();
    let step_invocations = |step_name: &str| {
        let step = steps
            .iter()
            .find(|step| step["name"] == step_name)
            .unwrap_or_else(|| panic!("missing soundd workflow step: {step_name}"));
        step["run"]
            .as_str()
            .unwrap_or("")
            .lines()
            .filter(|line| {
                line.contains("cargo ") && line.contains("--features soundd-daemon-native")
            })
            .collect::<Vec<_>>()
    };
    let build = step_invocations("Build and validate native sound daemon controller");
    let baseline = step_invocations("Require audio finalization baseline rejection");
    let native = step_invocations("Require native sound daemon finalization and process contracts");
    let retirement =
        step_invocations("Require sound daemon same-ELF present and absent execution proof");
    assert_eq!(build.len(), 1, "build the daemon checker and source stager");
    assert_eq!(
        baseline.len(),
        3,
        "retain baseline build, compile and behavioral RED"
    );
    assert_eq!(
        native.len(),
        5,
        "run worker, source-stager and daemon contracts"
    );
    assert_eq!(
        retirement.len(),
        1,
        "retain same-ELF execution retirement gate"
    );
    let invocations = build
        .into_iter()
        .chain(baseline)
        .chain(native)
        .chain(retirement);
    for line in invocations {
        required(
            line,
            &[
                "--config profile.test.package.sha2.opt-level=3",
                "--config profile.dev.package.sha2.opt-level=3",
            ],
        );
    }
}

#[test]
fn bluez_policy_retirement_requires_sealed_original_and_native_only_absence() {
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/check_bluetooth_bluez_policy.py",
            "test ! -e \"$absence/root/rust/tools/check_bluetooth_bluez_policy.py\"",
            "test ! -L \"$absence/root/rust/tools/check_bluetooth_bluez_policy.py\"",
            "OPENPILOT_RUST_ROOT=\"$absence/root\" PATH=/nonexistent /usr/bin/timeout",
            "-e trace=execve,execveat -o \"$absence/native.exec\"",
            "BLUEZ_POLICY_ABSENCE_PROOF=\"$absence\"",
            "--test bluetooth_bluez_policy_reference --locked -- --nocapture",
        ],
        &["bluetooth_bluez_policy.py", "BLUEZ_POLICY_ABSENCE_PROOF"],
        false,
    );
}

#[test]
fn enumeration_retirement_requires_sealed_original_and_native_only_absence() {
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/check_bluetooth_enumerate.py",
            "test ! -e \"$BLUETOOTH_ENUMERATE_ABSENT_ROOT/rust/tools/check_bluetooth_enumerate.py\"",
            "test ! -L \"$BLUETOOTH_ENUMERATE_ABSENT_ROOT/rust/tools/check_bluetooth_enumerate.py\"",
            "OPENPILOT_RUST_ROOT=\"$BLUETOOTH_ENUMERATE_ABSENT_ROOT\" PATH=\"$proof/empty-path\" /usr/bin/timeout",
            "-e trace=execve,execveat -o \"$proof/absent.exec\"",
            "controller == 1 && fixture == 2 && unexpected == 0",
            "--test bluetooth_enumerate_reference --locked -- --nocapture",
        ],
        &["bluetooth_enumerate.py", "BLUETOOTH_ENUMERATE_ABSENT_ROOT"],
        false,
    );
}

#[test]
fn bluetooth_sudo_callers_require_the_paired_native_elf_and_identity_seals() {
    job_check(
        "ui-connectivity",
        &[
            "BLUETOOTH_SUDO_ORIGINAL_PYTHON=\"$GITHUB_WORKSPACE/rust/tools/bluetooth_sudo_fixture.py\"",
            "cmp rust/target/debug/bluetooth-sudo-fixture \"$native_sudo\"",
            "BLUETOOTH_SUDO_NATIVE_BINARY=%s\\nBLUETOOTH_SUDO_NATIVE_SHA256=%s",
            "sha256sum \"$native_sudo\" rust/tools/bluetooth_sudo_fixture.py rust/tools/check_bluetooth_input.py rust/tools/bluetooth_daemon_fixture.py",
            "test -x \"$BLUETOOTH_SUDO_NATIVE_BINARY\"",
            "python rust/tools/check_bluetooth_runtime.py",
        ],
        &["bluetooth_sudo_fixture.py", "check_bluetooth_runtime.py"],
        false,
    );
    let script = commands(&workflow()["jobs"]["ui-connectivity"]);
    assert_eq!(
        script.matches("sha256sum -c \"$RUNNER_TEMP/ui-connectivity/bluetooth-sudo-fixture/caller-artifacts.sha256\"").count(),
        2,
    );
}

#[test]
fn lagd_masked_allocation_requires_same_source_old_and_current_measurements() {
    job_check(
        "estimation-runtime",
        &[
            "edb42bf095d6a3e3b38cc0a4546976dfb710363e",
            "CARGO_TARGET_DIR=\"$RUNNER_TEMP/lagd429-old-target\" cargo build",
            "CARGO_TARGET_DIR=\"$GITHUB_WORKSPACE/rust/target\" cargo build",
            "rust/tools/lagd_masked_allocation_probe.rs",
            "--compare \"$proof/old.log\" \"$proof/current.log\"",
            "-L \"dependency=$target/debug/deps\"",
            "\"${native_flags[@]}\"",
        ],
        &["masked-allocation-429"],
        false,
    );
}
#[test]
fn gnss_requires_source_serial_and_daemon_checks() {
    job_check(
        "gnss-runtime",
        &[
            "check_ublox.py",
            "check_pigeon.py",
            "check_ublox_serial.py",
            "check_ublox_daemons.py",
            "check_qcomgps_reference.py",
            "check_qcomgps_daemon.py",
            "check_qcomgps_nmea.py",
            "debug/check-qcomgps-assistance",
            "--features qcom-assistance-tools-native --bin check-qcomgps-assistance",
            "--test qcom_assistance_check --lib",
            "QCOMGPSD_BINARY=",
            "build_msgq_python.py",
        ],
        &["python rust/tools/check_"],
        false,
    );
}
#[test]
fn sensor_audio_requires_original_and_live_native_checks() {
    job_check(
        "sensor-audio",
        &[
            "check_sensord.py",
            "check_sensord_kernel.py",
            "check_sensord_daemon.py",
            "check_micd_analysis.py",
            "check_micd_daemon.py",
            "debug/check-soundd-policy\" --binary",
            "--features soundd-policy-native --bin check-soundd-policy --bin create-soundd-fixture-assets",
            "--test soundd_policy_reference --test soundd_policy_check --test soundd_fixture_assets --test soundd_policy_retirement --test soundd_policy_ci --locked",
            "--fixture-assets-binary \"$SOUNDD_FIXTURE_ASSETS_BINARY\"",
            "test ! -e rust/tools/check_soundd_daemon.py",
            "--features soundd-daemon-native --config profile.test.package.sha2.opt-level=3 --config profile.dev.package.sha2.opt-level=3 --bin check-soundd-daemon",
            "--test soundd_daemon_check --test soundd_daemon_runtime --locked",
            "--lib soundd_daemon_check::worker::tests --locked",
            "\"$SOUNDD_DAEMON_WORKER_BINARY\" --binary",
            "rust/target/debug/check-feedbackd --binary",
            "--features feedback-runtime-native --bin check-feedbackd",
            "--test feedbackd_check --test feedbackd_runtime_check --locked",
            "FEEDBACKD_NATIVE_BINARY=",
            "build_msgq_python.py",
            "build_params_python.py",
            "miri test",
        ],
        &["python rust/tools/check_"],
        false,
    );
}
#[test]
fn startup_services_require_source_and_live_native_protocols() {
    job_check(
        "startup-services",
        &[
            "check_lpa.py",
            "iproute2 strace",
            "--features command-owner-tests --test command_owner --locked",
            "--lib updated_process_check::ownership_tests --locked",
            "--features command-owner-tests --lib --test command_owner",
            "--lib --test updated_process_check --bin check-updated-process",
            "--features bridge-tools-native --bin check-bridge",
            "--features bridge-tools-native --test bridge_check --locked",
            "--features bridge-tools-native --lib bridge_check:: --locked",
            "--features bridge-tools-native --test bridge_controller_reference --locked",
            "BRIDGE_NATIVE_CONTROLLER_REPORT=",
            "sudo -E \"$PWD/rust/target/debug/check-bridge\" --original",
            "sudo chown -R \"$(id -u):$(id -g)\" \"$RUNNER_TEMP/startup-services/bridge-native-controller\"",
            "check_agnos.py",
            "rust/target/debug/build-bridge-source",
            "build_msgq_python.py",
        ],
        &["python rust/tools/check_", "check_bridge.py"],
        false,
    );
    let workflow = workflow();
    let steps = workflow["jobs"]["startup-services"]["steps"]
        .as_array()
        .unwrap();
    let ownership = steps
        .iter()
        .find(|step| step["name"] == "Verify updater command and checker process ownership")
        .unwrap();
    assert_eq!(
        ownership["env"]["UPDATED_OWNER_LAUNCHER"],
        "${{ env.CI296_SOURCE }}/rust/target/debug/openpilot-process-child"
    );
    let run = ownership["run"].as_str().unwrap();
    assert!(
        run.find("rustup component add clippy --toolchain 1.94.0")
            .expect("minimal toolchain must install Clippy")
            < run.find("cargo clippy").unwrap()
    );
}
#[test]
fn modem_requires_real_baseline_failure_and_fixed_helper_lifetimes() {
    job_check("platform-runtime", &[
        "git worktree add --detach \"$baseline\" 1364e48c96e888fa5d178c5eb2132d135cf30765",
        "--exact interface_deadline_includes_stdout_eof_after_leader_exit --nocapture",
        "grep -F 'test result: FAILED'",
        "must time out, not accept leader's valid inet line",
        "MODEM_PROCESS_CHILD_BINARY=",
        "cp rust/crates/modem/tests/fixtures/original-source-reference.json",
        "--features helper-lifetime-tests --test helper_lifetime --locked",
        "--features openpilot-modem/helper-lifetime-tests -p openpilot-process-supervision --all-targets --locked -- -D warnings",
    ], &["helper-lifetime-tests"], false);
}
#[test]
fn platform_runtime_requires_retired_manager_and_modem_hardwared_proof() {
    job_check(
        "platform-runtime",
        &[
            "rustup toolchain install 1.94.0 --profile minimal --component clippy",
            "for name in check_manager.py check_manager_logging.py manager_reference.py; do",
            "test ! -e \"rust/tools/$name\" && test ! -L \"rust/tools/$name\"",
            "test ! -e rust/tools/generate_manager_cars.py && test ! -L rust/tools/generate_manager_cars.py",
            "--features manager-qa-native,manager-cars-native --bin check-manager-qa --bin check-manager --bin check-manager-logging",
            "--features manager-qa-native,manager-cars-native --lib --test manager_qa --test manager_qa_compat",
            "--test manager_retirement --test manager_cars_generate --no-run --message-format=json",
            "--test manager_retirement --test manager_cars_generate -- -D warnings",
            "--features manager-qa-native,manager-cars-native --lib manager_qa:: -- --nocapture",
            "--bin generate-manager-cars",
            "manager_cars_generate::tests::",
            "4 passed; 0 failed;",
            "13 passed; 0 failed;",
            "MANAGER_CARS_RETIREMENT_REQUIRE_ABSENT=1",
            "manager_car_generator_trace_parser_rejects_unknown_failed_and_incomplete_execs",
            "native_car_generator_preserves_original_bytes_in_both_source_states",
            ".closed_native_execs == 11",
            ".total_closed_native_exec_attempts == 62",
            "\"$MANAGER_CARS_BINARY\" rust/crates/manager/data/cars --check",
            "34742676490c2c6133eb940cff7d61721f2d7cb072978fd052c2376faeda9995",
            "MANAGER_RETIREMENT_REQUIRE_ABSENT=1",
            "manager_exec_audit_accepts_empty_threads_without_losing_execution_checks",
            "manager_original_observations_survive_three_controller_retirement",
            "cmp \"$proof/providers-before.sha256\" \"$proof/providers-after.sha256\"",
            "examples/manager_native",
            "examples/manager_adapters",
            "for name in check_modem.py modem_source.py modem_fixture.py modem_fixture_command.py check_modem_process_boundary.py; do",
            "test -f openpilot/system/hardware/tici/modem.py",
            "--features modem-qa-native --bin check-modem",
            "--features modem-qa-native --lib --test modem_retirement --no-run",
            "--features modem-qa-native --lib --bin check-modem --test modem_retirement -- -D warnings",
            "--features modem-qa-native --lib modem_ -- --nocapture",
            "MODEM_REQUIRE_TOOLS_RETIRED=1",
            "MODEM_RETIREMENT_OUTPUT=",
            "grep -Fx \"test same_held_native_elfs_replay_modem_originals_in_checkout_and_private_absent_root ... ok\"",
            "^test result: ok\\. 1 passed; 0 failed; 0 ignored;",
            "same_held_native_elfs_replay_modem_originals_in_checkout_and_private_absent_root",
            "for name in check_hardwared.py hardwared_source.py check_hardwared_daemon.py; do",
            "--features hardwared-qa-native --bin check-hardwared --bin check-hardwared-daemon",
            "--features hardwared-qa-native --lib hardwared_ -- --nocapture",
            "--test hardwared_capture --test hardwared_qa --test hardwared_retirement",
            "HARDWARED_REQUIRE_RETIRED=1",
            "HARDWARED_RETIREMENT_OUTPUT=",
            "same_native_clis_preserve_all_original_cases_and_live_ipc_in_both_roots",
            "cmp -s \"$seals/before.sha256\" \"$seals/after.sha256\"",
        ],
        &[
            "python rust/tools/check_",
            "MANAGER_RETIREMENT_REQUIRE_ABSENT=1",
            "HARDWARED_REQUIRE_RETIRED=1",
            "MODEM_REQUIRE_TOOLS_RETIRED=1",
        ],
        false,
    );
    let data = workflow();
    let run = commands(&data["jobs"]["platform-runtime"]);
    for name in [
        "check_manager.py",
        "check_manager_logging.py",
        "manager_reference.py",
    ] {
        let path = root().join("rust/tools").join(name);
        assert!(
            fs::symlink_metadata(path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "retired manager controller reappeared: {name}"
        );
    }
    assert!(
        fs::symlink_metadata(root().join("rust/tools/generate_manager_cars.py"))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(!run.contains("python rust/tools/generate_manager_cars.py"));
    assert!(root().join("openpilot/system/manager/manager.py").is_file());
    assert!(!run.contains("python rust/tools/check_manager.py"));
    assert!(!run.contains("python rust/tools/check_manager_logging.py"));
    assert!(!run.contains("python rust/tools/check_hardwared.py"));
    assert!(!run.contains("python rust/tools/check_hardwared_daemon.py"));
    assert!(!run.contains("python rust/tools/check_modem.py"));
    assert!(!run.contains("python rust/tools/check_modem_process_boundary.py"));
}
#[test]
fn workspace_checks_run_without_waiting_for_runtime_jobs() {
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    absent(job, &["needs", "if"]);
    required(
        &commands(job),
        &[
            "cargo fmt --all --check",
            "cargo clippy --workspace",
            "cargo test --workspace",
            "cargo build --workspace --release --locked",
            "cargo build --locked -p openpilot-log-format",
            "rustc --edition=2021 tools/json_view_allocation_probe.rs --extern openpilot_log_format=target/debug/libopenpilot_log_format.rlib -L dependency=target/debug/deps",
            "\"$RUNNER_TEMP/json-view-allocation\"",
            "check_usbgpu_hardware.py",
            "check_usbgpu_asm.py",
            "check_usbgpu_status.py",
        ],
    );
    assert_eq!(data["jobs"]["fast"]["name"], "rust checks");
    mandatory(job, &["tools/json_view_allocation_probe.rs"]);
    assert_eq!(data["jobs"]["fast"]["steps"].as_array().unwrap().len(), 1);
    let script = commands(job);
    assert!(
        script
            .find(
                "git lfs pull --include=openpilot/selfdrive/modeld/models/driving_supercombo.onnx"
            )
            .unwrap()
            < script.find("cargo test --workspace").unwrap(),
        "native compiler tests require the actual pinned model before execution"
    );
    let bootlog = "cargo build -p openpilot-bootlog --example bootlog_snapshot -p openpilot-process-supervision --bin openpilot-process-child --locked";
    assert!(
        script.find(bootlog).unwrap() < script.find("cargo test --workspace").unwrap(),
        "snapshot checker regressions require current native candidate and launcher"
    );
    mandatory(job, &[bootlog]);
    let launcher_env = "export WIFI_PROCESS_CHILD_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/openpilot-process-child\"";
    required(&script, &[launcher_env]);
    assert!(script.find(bootlog).unwrap() < script.find(launcher_env).unwrap());
    assert!(script.find(launcher_env).unwrap() < script.find("cargo test --workspace").unwrap());
}
#[test]
fn hardware_runtime_requires_source_and_native_boundaries() {
    let data = workflow();
    let job = &data["jobs"]["hardware-runtime"];
    assert!(!commands(job).contains("python "));
    assert!(!job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["uses"]
            .as_str()
            .is_some_and(|name| name.starts_with("actions/setup-python@"))));
    job_check(
        "hardware-runtime",
        &[
            "rust/target/debug/check-bootlog-artifacts rust/target/debug/openpilot-bootlog",
            "--features openpilot-dev-tools/bootlog-artifact-native",
            "--test bootlog_artifact_check",
            "rust/target/debug/check-bootlog-snapshot rust/target/debug/examples/bootlog_snapshot",
            "rust/target/debug/build-bootlog-snapshot-reference --output",
            "--params-reference \"$RUNNER_TEMP/hardware-runtime/snapshot-reference/bootlog-params-path\"",
            "--child rust/target/debug/bootlog-snapshot-child --launcher rust/target/debug/openpilot-process-child",
            "--test bootlog_snapshot_check",
            "rust/target/debug/check-logger-identifier rust/target/debug/examples/logger_identifier",
            "openpilot-dev-tools/hardware-info-qa-native",
            "--features hardware-info-qa-native",
            "--bin check-hardware-info",
            "--test hardware_info_qa --test hardware_info_capture --test hardware_info_retirement",
            "same_elf_checks_all_original_cases_in_actual_and_private_roots -- --ignored --exact --nocapture",
            "export HARDWARE_INFO_REQUIRE_RETIRED=1",
            "for name in check_hardware_info.py hardware_info_cases.py hardware_info_reference.py; do",
            "rust/target/debug/check-hardware-control-policy rust/target/debug/examples/hardware_control_trace",
            "rust/target/debug/check-amplifier-policy rust/target/debug/examples/amplifier_trace",
            "rust/target/debug/check-amplifier-linux rust/target/debug/examples/amplifier_linux",
            "rust/target/debug/check-hardware-command --evidence \"$RUNNER_TEMP/hardware-runtime\"",
            "bootlog_raw_file_probe.rs",
        ],
        &["same_elf_checks_all_original_cases_in_actual_and_private_roots", "--bin check-hardware-info", "--test hardware_info_qa", "check-hardware-command", "check-bootlog-snapshot", "--test bootlog_snapshot_check", "check-bootlog-artifacts", "--test bootlog_artifact_check"],
        false,
    );
}
#[test]
fn startup_prerequisites_run_actual_children_and_collectors() {
    job_check(
        "startup-runtime",
        &[
            "check_process_supervision.py",
            "MANAGED_ENTRY_WORKER_BINARY=",
            "MANAGED_CHILD_BINARY=",
            "MANAGED_COLLECTOR_BINARY=",
            "MANAGED_PARAMS_BINDING=",
            "\"$MANAGED_ENTRY_WORKER_BINARY\" --readiness \"$MANAGED_CHILD_BINARY\"",
            "\"$MANAGED_ENTRY_WORKER_BINARY\" \"$MANAGED_CHILD_BINARY\"",
            "--features managed-entry-native --lib managed_entry_ --locked",
            "--features managed-entry-native --lib qa_endpoint::tests --locked",
            "--test managed_entry_check --test managed_entry_source --test managed_entry_collector_source --test managed_entry_collector_readiness --test managed_entry_readiness --test managed_entry_ownership --test managed_entry_sigint --test managed_entry_runtime --locked",
            "--features qa-process-title --test source_title --locked",
            "--features qa-process-title --all-targets --locked -- -D warnings",
            "rustup toolchain install 1.94.0 --profile minimal --component clippy",
            "--features openpilot-dev-tools/managed-entry-native",
            "--features sdk-receiver-native --test sdk_receiver --locked",
            "--bin check-sdk-receiver --test sdk_receiver --locked -- -D warnings",
            "rust/target/debug/check-sdk-receiver",
            "rust/target/debug/check-registration rust/target/debug/examples/registration_trace",
            "check-registration-clock",
            "rust/target/debug/check-registration-utf7",
            "rust/target/debug/check-registration-vendor",
            "rust/target/debug/check-manager-catalog rust/target/debug/examples/catalog_probe",
            "--features manager-catalog-tools-native --bin check-manager-catalog --bin manager-catalog-reference",
            "--test manager_catalog_check --test manager_catalog_reference --locked",
            "rust/target/debug/check-checkout-status --binary rust/target/debug/examples/checkout_trace",
            "--features checkout-tools-native --bin check-checkout-status --bin checkout-status-reference",
            "--test checkout_status_check --test checkout_status_reference --locked",
            "--captured-binary rust/target/debug/examples/captured_trace --handshake-fixture rust/target/debug/examples/handshake_fixture",
            "--timeouts",
            "--launcher rust/target/debug/openpilot-process-child",
            "--fixture rust/target/debug/examples/process_fixture",
            "build_msgq_python.py",
            "build_params_python.py",
        ],
        &["python rust/tools/check_", "check-checkout-status", "--test checkout_status_check", "check-manager-catalog", "--test manager_catalog_check", "--test sdk_receiver", "check-sdk-receiver", "--test managed_entry_", "--test source_title", "\"$MANAGED_ENTRY_WORKER_BINARY\""],
        true,
    );
    let run = commands(&workflow()["jobs"]["startup-runtime"]);
    assert!(!run.contains("python rust/tools/check_checkout_status.py"));
    assert!(!run.contains("python rust/tools/check_manager_catalog.py"));
    assert!(!run.contains("check_sdk_receiver.py"));
    assert!(!run.contains("check_managed_entry.py"));
    assert!(!run.contains("check_managed_entry_readiness.py"));
    let data = workflow();
    for key in [
        "CARGO_INCREMENTAL",
        "CARGO_PROFILE_DEV_DEBUG",
        "CARGO_PROFILE_TEST_DEBUG",
    ] {
        assert_eq!(
            data["jobs"]["startup-runtime"]["env"][key], "0",
            "all startup build and test steps must retain the disk budget: {key}"
        );
    }
    let checkout = data["jobs"]["startup-runtime"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| {
            step["name"] == "Compare checkout metadata, real Git and child exec boundaries"
        })
        .unwrap()["run"]
        .as_str()
        .unwrap();
    assert!(
        !checkout.contains("--group"),
        "all checkout groups remain required"
    );
}
#[test]
fn journald_requires_native_ipc_and_independent_fixture_contracts() {
    job_check(
        "support-runtime",
        &[
            "--features journal-runtime-native --bin check-journald --bin journalctl-fixture",
            "--test journald_check --test journald_fixture --test journald_runtime_check --locked",
            "rust/target/debug/check-journald --binary rust/target/debug/journald-rs --fixture rust/target/debug/journalctl-fixture",
        ],
        &["check-journald", "--test journald_check"],
        true,
    );
    assert!(!commands(&workflow()["jobs"]["support-runtime"])
        .contains("python rust/tools/check_journald.py"));
}
#[test]
fn logmessaged_startup_requires_sealed_original_and_native_exec_audit() {
    job_check(
        "support-runtime",
        &[
            "--features logmessaged-startup-native --bin check-logmessaged-startup",
            "--features logmessaged-startup-native --test logmessaged_startup_check --locked",
            "--lib logmessaged_startup_check::worker::tests --locked",
            "--features logmessaged-startup-native --test logmessaged_startup_reference --locked",
            "LOGMESSAGED_STARTUP_PROOF=",
            "env OPENPILOT_RUST_ROOT=\"$root\" PATH=\"$proof/native-path\" /usr/bin/timeout",
            "sha256sum -c \"$proof/inputs.sha256\"",
            "cee8e19032542bc4851d0ce6bfebd9b71dde9184b19bdc5b0fb23e06e974d0dd",
            "(cd \"$fixture\" && sha256sum -c SHA256SUMS)",
        ],
        &["--features logmessaged-startup-native"],
        false,
    );
    assert!(!commands(&workflow()["jobs"]["support-runtime"])
        .contains("python rust/tools/check_logmessaged_startup.py"));
    assert!(!root()
        .join("rust/tools/check_logmessaged_startup.py")
        .exists());
}
#[test]
fn interrupted_send_boundary_is_a_required_support_check() {
    let data = workflow();
    let steps = data["jobs"]["support-runtime"]["steps"].as_array().unwrap();
    let matches = steps
        .iter()
        .filter(|s| {
            s["name"] == "Compare interrupted diagnostic sends and actual upload completion"
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    trusted_step(matches[0]);
    required(
        matches[0]["run"].as_str().unwrap(),
        &[
            "zmq_send_boundary.c",
            "--wrap=zmq_msg_send",
            "check_interrupted_send.py",
            "check-uploader-siblings",
            "--scope interrupted-send",
        ],
    );
}
#[test]
fn root_directory_builds_select_the_pinned_rust_toolchain() {
    let data = workflow();
    let toolchain: toml::Value = toml::from_str(&read("rust/rust-toolchain.toml")).unwrap();
    assert_eq!(
        data["env"]["RUSTUP_TOOLCHAIN"].as_str(),
        toolchain["toolchain"]["channel"].as_str()
    );
}
fn setup_environment(job: &Value, name: &str, expected: impl FnOnce(&str) -> String) {
    let setup = job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap();
    let temp = tempfile::Builder::new()
        .prefix("native CI env ")
        .tempdir()
        .unwrap();
    let output = temp.path().join("environment");
    let temporary = temp.path().to_str().unwrap();
    let env = BTreeMap::from([
        ("RUNNER_TEMP".into(), temporary.into()),
        (
            "PYTHONPATH".into(),
            "/fixture/repository:/fixture/tools".into(),
        ),
        ("GITHUB_ENV".into(), output.to_str().unwrap().into()),
    ]);
    assert!(run_shell(setup["run"].as_str().unwrap(), &env)
        .unwrap()
        .success());
    assert_eq!(fs::read_to_string(output).unwrap(), expected(temporary));
}
#[test]
fn support_binding_path_is_configured_on_the_runner() {
    let data = workflow();
    let pattern = regex::Regex::new(r"\$\{\{\s*runner[.\[]").unwrap();
    for job in data["jobs"].as_object().unwrap().values() {
        if let Some(env) = job["env"].as_object() {
            for value in env.values() {
                assert!(!pattern.is_match(value.as_str().unwrap()));
            }
        }
    }
    setup_environment(
        &data["jobs"]["support-runtime"],
        "Configure original support IPC imports",
        |temporary| {
            format!(
                "PYTHONPATH={temporary}/support-msgq-python:/fixture/repository:/fixture/tools\n"
            )
        },
    );
}
#[test]
fn radar_requires_fresh_source_runtime_arm_and_memory_checks() {
    let data = workflow();
    for name in ["radar-runtime", "radar-arm", "radar-memory"] {
        let actual = native_functional_job(&data, name);
        let job = &actual;
        absent(job, &["if", "continue-on-error"]);
        required(&commands(job), &["check-disk-space"]);
        assert_eq!(job["env"]["CARGO_INCREMENTAL"], "0");
        some_artifact(job);
    }
    required(
        &commands(&data["jobs"]["radar-runtime"]),
        &[
            "build_params_python.py",
            "build_msgq_python.py",
            "RADAR_PARAMS_BINDING",
            "rust/target/debug/prepare-radarcan-assets",
            "--bin stage-torque-numerics",
            "test_radarcan_ci.py",
            "check_radarcan_ci.py --phase reference",
            "check_radarcan_ci.py --phase ipc",
            "-p openpilot-radarcan -p openpilot-can",
            "--all-targets --locked -- -D warnings",
        ],
    );
    let native = native_functional_job(&data, "radar-arm");
    let arm = commands(&native);
    assert_eq!(native["runs-on"], serde_json::json!("ubuntu-24.04-arm"));
    required(
        &arm,
        &[
            "--target aarch64-unknown-linux-gnu",
            "--no-run",
            "--message-format=json",
            "\"$radar_test\" --test-threads=1",
            "test \"$(cat \"$RUNNER_TEMP/radar-arm/machine.txt\")\" = aarch64",
            "check_radarcan_ci.py --phase reference",
            "check_radarcan_ci.py --phase ipc",
            "--platform manylinux_2_27_aarch64",
            "sha256sum",
        ],
    );
    assert_eq!(native["env"]["LOCAL_CI_ARCH"], "${{ matrix.arch }}");
    assert_eq!(native["env"]["CI296_NATIVE_ARM"], "1");
    assert_eq!(
        native["strategy"]["matrix"]["include"],
        serde_json::json!([{"arch":"aarch64","runner":"hosted-arm64"}])
    );
    // 실제 ARM 실행·최종 proof 경계를 확인하고 원래 기능 selector는 위에서 그대로 검사한다.
    required(
        &arm,
        &[
            "tools/ci/native-arm\" prepare",
            "tools/ci/native-arm\" finish",
        ],
    );
    required(
        &commands(&data["jobs"]["radar-memory"]),
        &[
            "nightly-2026-09-29",
            "numerics_",
            "scheduler_argument_abi_preserves_source_fifo_priority",
            "-Zmiri-no-extra-rounding-error",
            "-Zmiri-strict-provenance",
            "-Zmiri-symbolic-alignment-check",
            "-Zmiri-preemption-rate=0.1",
            "-Zmiri-tree-borrows",
        ],
    );
}
#[test]
fn selfdrived_requires_source_loop_native_ipc_and_failure_checks() {
    let data = workflow();
    let job = &data["jobs"]["selfdrive-runtime"];
    absent(job, &["if"]);
    for checker in [
        "check_selfdrived_controller.py",
        "check_selfdrived_ipc.py",
        "check_selfdrived_failures.py",
        "test_selfdrived_source.py",
    ] {
        let step = job["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["run"].as_str().unwrap_or("").contains(checker))
            .unwrap();
        trusted_step(step);
    }
    required(
        &commands(job),
        &[
            "-p openpilot-selfdrived -p openpilot-messaging --locked",
            "7709c2e3c455cefba0ff2a72dffad7c22375e49a",
            "CARGO_TARGET_DIR=\"$RUNNER_TEMP/messaging431-old-target\" cargo build",
            "rust/tools/messaging_update_allocation_probe.rs",
            "-p openpilot-messaging --lib --locked",
            "test \"${#cereal[@]}\" -eq 1",
            "test \"${#capnp[@]}\" -eq 1",
            "--compare \"$proof/old.log\" \"$proof/current.log\"",
            "-p openpilot-messaging --all-targets --locked -- -D warnings",
            "build_params_python.py",
            "build_msgq_python.py",
        ],
    );
    some_artifact(job);
}
#[test]
fn selfdrive_params_requires_native_live_boundary_matrix_and_original_cpp_reference() {
    let data = workflow();
    let job = &data["jobs"]["selfdrive-runtime"];
    let script = commands(job);
    required(&script, &[
        "--features selfdrive-params-native --bin check-selfdrive-params --bin build-selfdrive-params-reference --bin selfdrive-params-test-peer",
        "--features selfdrive-params-native --lib selfdrive_params --locked",
        "--features selfdrive-params-native --test selfdrive_params_check --locked",
        "rust/target/debug/build-selfdrive-params-reference --output",
        "rust/target/debug/check-selfdrive-params --native",
        "--reference \"$RUNNER_TEMP/selfdrived-native/params-reference/selfdrive-params-reference\"",
    ]);
    assert!(!script.contains("check_selfdrive_params.py"));
    mandatory(
        job,
        &[
            "selfdrive-params-native",
            "build-selfdrive-params-reference",
            "check-selfdrive-params --native",
        ],
    );
}
#[test]
fn navigation_requires_original_transport_auth_and_timer_evidence() {
    let data = workflow();
    let job = &data["jobs"]["navd-runtime"];
    absent(job, &["if"]);
    required(
        &commands(job),
        &[
            "build_params_python.py",
            "build_msgq_python.py",
            "NAVD_PARAMS_BINDING",
            "REGISTRATION_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/examples/registration_trace\"",
            "REGISTRATION_WORKER_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/check-registration\"",
            "--bin check-registration -p openpilot-registration --example registration_trace",
            "requests==2.34.2",
            "urllib3==2.7.0",
            "PyJWT==2.14.0",
            "-p openpilot-navd --features native --bins --examples --locked",
            "check_navd_policy.py",
            "check_navd_engine.py",
            "check_navd_http.py",
            "--timeouts",
            "rust/target/debug/check-navd-config --binary rust/target/debug/examples/navd_config",
            "check_navd_destination.py",
            "check_navd_ipc.py",
        ],
    );
    assert!(job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["if"] == TRUSTED_ALWAYS
            && s["with"]["path"]
                .as_str()
                .unwrap_or("")
                .contains("navd-native/")));
    required(&commands(&data["jobs"]["arm64"]), &["cargo build -p openpilot-navd --features native --bins --examples --release --locked --target aarch64-unknown-linux-gnu","release/openpilot-set-destination"]);
}
#[test]
fn inherited_side_effects_are_source_repository_only() {
    for (file, job) in [
        ("sync.yml", "sync"),
        ("naver-upstream-sync.yml", "sync"),
        ("wiki-settings-publish.yaml", "synchronize"),
        ("carrot-route-vault-publish.yaml", "publish"),
    ] {
        let data = parse(&read(&format!(".github/workflows/{file}"))).unwrap();
        required(
            data["jobs"][job]["if"].as_str().unwrap_or(""),
            &["github.repository == 'bin9208/openpilot'"],
        );
    }
}
#[test]
fn rust_checks_cover_protected_pushes_and_prs() {
    validate_rust_gate(&workflow()).unwrap();
}
#[test]
fn gate_mutations_cannot_remove_required_suites_or_allow_skips() {
    let data = workflow();
    for key in [
        "WORKSPACE",
        "MEMORY",
        "ENCODER",
        "XIAOGE",
        "UI_RUNTIME",
        "DBC_DARWIN",
    ] {
        let mut modified = data.clone();
        modified["jobs"]["fast"]["steps"][0]["env"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            validate_rust_gate(&modified).is_err(),
            "accepted removed {key}"
        );
    }
    let mut modified = data.clone();
    modified["jobs"]["fast"]["if"] = "${{ success() }}".into();
    assert!(validate_rust_gate(&modified).is_err());
}
#[test]
fn webrtc_requires_native_host_and_arm_real_media_and_failure_boundaries() {
    let data = workflow();
    let job = &data["jobs"]["webrtc-runtime"];
    assert_eq!(
        job["strategy"]["matrix"]["runner"],
        serde_json::json!(["local-x64"])
    );
    mandatory(job, &[""]);
    required(
        &commands(job),
        &[
            "webrtc_source_requirements.txt",
            r#"sudo apt-get -o "APT::Architectures=$(dpkg --print-architecture)" install -y g++ clang-18 libclang-18-dev cmake pkg-config file capnproto"#,
            "libavcodec-dev=7:6.1.1-*",
            "build_params_python.py",
            "build_msgq_python.py",
            "-p openpilot-webrtc --features native --bins",
            "--features native --lib --locked",
            "--features native --all-targets --locked",
            "webrtc_stun_checks",
            "webrtc_identifiers_compare.py",
            "webrtc_multicamera_compare.py",
            "webrtc_fir_compare.py",
            "webrtc_body_compare.py",
            "webrtc_params_compare.py",
            "webrtc_cli_compare.py",
            "webrtc_srflx_compare.py",
            "webrtc_debug_compare.py",
            "source-standard-h264 native-standard-h264",
            "WEBRTC_PARAMS_BINDING",
            "check-disk-space",
            "SHA256SUMS",
        ],
    );
    artifacts(job);
    let req = read("rust/tools/webrtc_source_requirements.txt");
    for line in ["aiortc==1.14.0", "av==16.1.0", "aioice==0.10.2"] {
        assert!(req.lines().any(|v| v == line));
    }
}

#[test]
fn native_arm_reuse_preserves_all_candidate_steps_and_six_x64_rows() {
    use openpilot_dev_tools::ci_policy::validate_native_arm_reuse;
    let main = workflow();
    let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
    validate_native_arm_reuse(&main, &native).unwrap();
    for (name, candidate) in [
        ("carrot-man-runtime", "python-rust-523-native-arm.yml"),
        ("planner-runtime", "python-rust-523-native-arm-more.yml"),
        ("webrtc-runtime", "python-rust-523-native-arm-more.yml"),
        ("webcam-runtime", "python-rust-523-native-arm-remaining.yml"),
        ("xiaoge-runtime", "python-rust-523-native-arm-remaining.yml"),
        ("radard-runtime", "python-rust-523-native-arm-remaining.yml"),
        ("radar-arm", "python-rust-523-native-arm-final-two.yml"),
        (
            "carrot-navi-arm",
            "python-rust-523-native-arm-final-two.yml",
        ),
    ] {
        let data = parse(&read(&format!(".github/workflows/{candidate}"))).unwrap();
        let mut expected = data["jobs"][name].clone();
        // #572 final-two는 이미 canonical SDK를 갖는다. 나머지6role만
        // 기존 설치→검증/전후 봉인 overlay를 적용하며 전체 job 비교는 유지한다.
        if !["radar-arm", "carrot-navi-arm"].contains(&name) {
            let (dependency, install) = match name {
            "planner-runtime" => (
                "Reserve capacity and install original planner dependencies",
                "rustup toolchain install 1.94.0 --profile minimal --component clippy\n",
            ),
            "carrot-man-runtime" => (
                "Reserve runner capacity and install pinned dependencies",
                "rustup toolchain install 1.94.0 --profile minimal\n",
            ),
            "webrtc-runtime" => (
                "Reserve capacity and install pinned source and native providers",
                "rustup toolchain install 1.94.0 --profile minimal --component clippy\n",
            ),
            "webcam-runtime" => (
                "Install exact source closure and native capture providers",
                "rustup toolchain install 1.94.0 --profile minimal --component clippy\n",
            ),
            "xiaoge-runtime" => (
                "Reserve capacity and install Xiaoge build prerequisites",
                "rustup toolchain install 1.94.0 --profile minimal --component clippy\n",
            ),
            "radard-runtime" => (
                "Reserve capacity and install original oracle dependencies",
                "rustup toolchain install 1.94.0 --profile minimal --component clippy --component rustfmt\n",
            ),
            "radar-arm" => (
                "Reserve runner capacity and install generic ARM dependencies",
                "rustup toolchain install 1.94.0 --profile minimal\nrustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu\n",
            ),
            "carrot-navi-arm" => (
                "Reserve runner capacity and install native ARM build dependencies",
                "rustup toolchain install 1.94.0 --profile minimal\nrustup target add --toolchain 1.94.0 aarch64-unknown-linux-gnu\n",
            ),
            _ => unreachable!(),
        };
            let trusted = "${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}";
            let sdk_env = serde_json::json!({
                "CI296_HOST_CONTROL": "1",
                "ARM530_HELPER_SHA256": "faa58b5a058a927824870ac2df8e9aeed3f33830d62c8bb6407c6e6540277b9b"
            });
            let steps = expected["steps"].as_array_mut().unwrap();
            let dep = steps
                .iter_mut()
                .find(|step| step["name"] == dependency)
                .unwrap();
            let run = dep["run"].as_str().unwrap();
            assert_eq!(
                run.matches(install).count(),
                1,
                "{name}: fixed pilot SDK install"
            );
            dep["run"] = run
            .replace(
                install,
                "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" verify\n",
            )
            .into();
            let first = steps
                .iter()
                .position(|step| step["shell"] == "openpilot-local-shell {0}")
                .unwrap();
            steps.insert(first, serde_json::json!({
            "name": "Prepare exact fresh ARM 1.94 SDK for all eight admitted roles",
            "timeout-minutes": "20", "id": "arm530_sdk", "if": trusted,
            "shell": "bash", "working-directory": "${{ env.CI296_SOURCE }}",
            "env": sdk_env,
            "run": "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" before\n"
        }));
            let finish = steps
                .iter()
                .position(|step| step["name"] == "Finish native ARM execution evidence")
                .unwrap();
            steps.insert(finish, serde_json::json!({
            "name": "Verify ARM SDK after every attempted maintained command",
            "timeout-minutes": "10", "id": "arm530_sdk_final",
            "if": "${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (always() && steps.arm530_sdk.outcome != 'skipped' && env.CI296_SOURCE != '') }}",
            "shell": "bash", "working-directory": "${{ env.CI296_SOURCE }}",
            "env": sdk_env,
            "run": "/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" after\n"
        }));
            for (label, needle) in [
                (
                    "Clean only uploaded owned source alias and target",
                    " && env.CI296_SOURCE_OWNER != ''",
                ),
                (
                    "Clean common owned temporary paths after uploaded source",
                    " && steps.ci296_source_cleanup.outcome == 'success'",
                ),
            ] {
                let step = steps.iter_mut().find(|step| step["name"] == label).unwrap();
                let condition = step["if"].as_str().unwrap();
                assert_eq!(
                    condition.matches(needle).count(),
                    1,
                    "{name}: fixed cleanup condition"
                );
                step["if"] = condition
                    .replace(
                        needle,
                        &format!(" && steps.arm530_sdk_final.outcome == 'success'{needle}"),
                    )
                    .into();
            }
        } else {
            let stages = expected["steps"].as_array().unwrap();
            for (id, phase) in [("arm530_sdk", "before"), ("arm530_sdk_final", "after")] {
                let selected = stages
                    .iter()
                    .filter(|step| step["id"] == id)
                    .collect::<Vec<_>>();
                assert_eq!(
                    selected.len(),
                    1,
                    "{name}: duplicate/missing canonical SDK stage"
                );
                assert_eq!(selected[0]["run"], format!("/usr/bin/bash \"$CI296_SOURCE/tools/ci/prepare-readonly-arm-rust-530\" {phase}\n"));
            }
        }
        let mut actual = native["jobs"][name].clone();
        actual["steps"][0] = expected["steps"][0].clone();
        for key in ["CI296_NATIVE_ARM_CALLER", "CI296_PR_HEAD_REPOSITORY"] {
            actual["env"].as_object_mut().unwrap().remove(key);
        }
        assert_eq!(
            actual, expected,
            "{name}: candidate command/env/conditions/holds changed"
        );
        if ["radar-arm", "carrot-navi-arm"].contains(&name) {
            assert_eq!(
                main["jobs"][name]["needs"],
                serde_json::json!(["native-arm"])
            );
            continue;
        }
        let matrix = &main["jobs"][name]["strategy"]["matrix"];
        if name == "carrot-man-runtime" {
            assert_eq!(
                matrix["include"],
                serde_json::json!([{"arch":"x86_64","runner":"local-x64"}])
            );
        } else if name == "radard-runtime" {
            assert_eq!(
                matrix["include"],
                serde_json::json!([{"runner":"local-x64","machine":"x86_64"}])
            );
        } else {
            assert_eq!(matrix["runner"], serde_json::json!(["local-x64"]));
        }
    }
    assert_eq!(main["jobs"]["fast"]["needs"].as_array().unwrap().len(), 42);
}

#[test]
fn native_arm_main_gate_rejects_removed_needs_binding_or_success_check() {
    let original = workflow();
    validate_rust_gate(&original).unwrap();
    for mode in ["caller", "needs", "binding", "test"] {
        let mut changed = original.clone();
        match mode {
            "caller" => {
                changed["jobs"]
                    .as_object_mut()
                    .unwrap()
                    .remove("native-arm");
            }
            "needs" => changed["jobs"]["fast"]["needs"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "native-arm"),
            "binding" => {
                changed["jobs"]["fast"]["steps"][0]["env"]
                    .as_object_mut()
                    .unwrap()
                    .remove("NATIVE_ARM");
            }
            _ => {
                let body = changed["jobs"]["fast"]["steps"][0]["run"].as_str().unwrap();
                assert!(body.contains("test \"$NATIVE_ARM\" = success\n"));
                changed["jobs"]["fast"]["steps"][0]["run"] =
                    body.replace("test \"$NATIVE_ARM\" = success\n", "").into();
            }
        }
        assert!(validate_rust_gate(&changed).is_err(), "native ARM {mode}");
    }
}
#[test]
fn xiaoge_requires_both_architectures_and_instrumented_native_libraries() {
    let data = workflow();
    let native = parse(&read(".github/workflows/rust-native-arm.yml")).unwrap();
    openpilot_dev_tools::ci_policy::validate_native_arm_reuse(&data, &native).unwrap();
    let arm = &native["jobs"]["xiaoge-runtime"];
    mandatory(arm, &["check_xiaoge_ci.py"]);
    artifacts(arm);
    assert_eq!(
        data["jobs"]["xiaoge-runtime"]["strategy"]["matrix"]["runner"],
        serde_json::json!(["local-x64"])
    );
    for (name, needles) in [
        (
            "xiaoge-runtime",
            &[
                "build-xiaoge-opencv",
                "build_params_python.py",
                "build_visionipc_python.py",
                "check_xiaoge_ci.py",
                "--features native-skip-miri",
                "test_xiaoge_ci.py",
            ][..],
        ),
        (
            "xiaoge-memory",
            &[
                "--sanitizers address,undefined",
                "check_xiaoge_native_memory.py",
                "-Zmiri-strict-provenance",
                "-Zmiri-symbolic-alignment-check",
                "-Zmiri-preemption-rate=0.1",
                "-Zmiri-tree-borrows",
                "--no-default-features",
            ][..],
        ),
    ] {
        let job = &data["jobs"][name];
        mandatory(job, &[""]);
        required(&commands(job), needles);
        artifacts(job);
    }
}
#[test]
fn encoder_requires_source_runtime_and_pinned_arm_artifacts() {
    job_check(
        "encoder-runtime",
        &[
            "--bin build-encoder-source",
            "--bin check-encoder-codecs",
            "--test encoder_codecs_check",
            "--bin check-encoder-v4l",
            "--test encoder_v4l_check",
            "--bin check-encoder-runtime",
            "--bin check-encoder-ci",
            "--test encoder_runtime_shutdown",
            "--test encoder_runtime_native",
            "--test encoder_ci_check",
            "-p openpilot-dev-tools --locked --test encoder_stage",
            "--features native --bins --examples",
            "--features openpilot-encoderd/native",
            "--message-format=json",
            "--features encoder-runtime-native",
        ],
        &["rust/target/debug/check-encoder-ci"],
        true,
    );
    let data = workflow();
    absent(
        &data["jobs"]["encoder-runtime"]["env"],
        &["PYTHONPATH", "ENCODER_MSGQ_PYTHON"],
    );
    let arm = data["jobs"]["arm64"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Build encoder daemon and probes with pinned ARM codecs and ION")
        .unwrap();
    assert_eq!(arm["working-directory"], "${{ env.CI296_SOURCE }}/rust");
    required(
        arm["run"].as_str().unwrap(),
        &["--features static-ffmpeg,visionipc-ion"],
    );
    trusted_step(arm);
    required(
        arm["env"]["FFMPEG_DIR"].as_str().unwrap(),
        &["ENCODER_ARM_FFMPEG"],
    );
    required(
        arm["env"]["ENCODER_LIBYUV_LIB"].as_str().unwrap(),
        &["ENCODER_ARM_LIBYUV"],
    );
}
#[test]
fn ui_checks_require_current_native_tools_after_test_receipt() {
    job_check(
        "ui-runtime",
        &[
            "--bin check-ui-updater-handshake",
            "--bin check-ui-model-adapter",
            "--bin check-ui-training-adapter",
            "rust/target/debug/check-ui-updater-handshake --evidence \"$RUNNER_TEMP/ui-native\"",
            "unit-tests.jsonl",
        ],
        &["check-ui-updater-handshake"],
        true,
    );
    let data = workflow();
    let steps = data["jobs"]["ui-runtime"]["steps"].as_array().unwrap();
    let build = steps
        .iter()
        .position(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("--bin check-ui-updater-handshake"))
        })
        .unwrap();
    let check = steps
        .iter()
        .position(|step| step["name"] == "Require queued exec error after helper exit")
        .unwrap();
    assert!(build < check);
    assert!(steps[build]["run"]
        .as_str()
        .unwrap()
        .contains("unit-tests.jsonl"));
}
#[test]
fn camera_requires_source_driver_fixture_and_visionipc_checks() {
    job_check(
        "camera-runtime",
        &[
            "check-camerad-ci",
            "libclang-rt-18-dev",
            "openpilot-camerad-runtime --features native-skip-miri",
            "--test vision --test vision_server --test vision_native",
        ],
        &["check-camerad-ci"],
        false,
    );
    let data = workflow();
    some_artifact(&data["jobs"]["camera-runtime"]);
    let arm = data["jobs"]["arm64"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "Build generic and ION camera aarch64 artifacts")
        .unwrap()["run"]
        .as_str()
        .unwrap();
    assert!(arm.find("cp target/").unwrap() < arm.find("--features visionipc-ion").unwrap());
    required(arm, &["--features native-skip-miri", "sha256sum"]);
}
#[test]
fn panda_requires_native_transports_and_source_composition() {
    let data = workflow();
    let steps = data["jobs"]["panda-runtime"]["steps"].as_array().unwrap();
    let build = steps.iter().position(|step| step["run"].as_str().is_some_and(|run|
        run.contains("--features openpilot-dev-tools/pandad-safety-padding-native,openpilot-dev-tools/panda-ci-native,openpilot-dev-tools/pandad-device-native,openpilot-dev-tools/pandad-protocol-native,openpilot-dev-tools/pandad-can-io-native --bins --examples --locked --message-format=json"))).expect("all native Panda controllers must be included before orchestration");
    let caller = steps
        .iter()
        .position(|step| {
            step["run"].as_str().is_some_and(|run| {
                run.contains("--features panda-ci-native --test panda_ci_retirement --locked")
            })
        })
        .unwrap();
    assert!(
        build < caller,
        "native checker must exist before the full Panda orchestration"
    );

    job_check("panda-runtime", &["PANDA_CI_BINARIES=", "PANDA_CI_BUILD_MESSAGES=", "build_msgq_python.py","libusb1==3.4.0","-p openpilot-pandad -p openpilot-panda-usb -p openpilot-panda-spi -p openpilot-panda-spi-linux","--message-format=json", "--features panda-ci-native --test panda_ci_check --locked", "--test panda_ci_check --test panda_ci_retirement --locked -- -D warnings", "--features panda-ci-native --test panda_ci_retirement --locked -- --test-threads=1 --nocapture", "PANDA_CI_EXPECT_ABSENT=1", "PANDA_CI_SOURCE_COMMIT=", "PANDA_CI_PRIVATE_ROOT=", "PANDA_CI_CHECKER_BINARY=", "PANDA_CI_RETIREMENT_EVIDENCE=", "PANDA_CI_CLI_EVIDENCE=", "PANDA_CI_ORIGINAL_CLI_RECEIPT=", "PANDA_CI_ORIGINAL_CLI_SHA256=02aa67da5d511a312fe30b7ebc058da5a8ada66098e14aef7b05a8b63eef700b", "test ! -e rust/tools/check_pandad_ci.py", "test ! -L rust/tools/check_pandad_ci.py", "test ! -e rust/tools/tests/test_pandad_ci.py", "test ! -L rust/tools/tests/test_pandad_ci.py"], &["--features panda-ci-native --test panda_ci_retirement"], false);
    let commands = commands(&data["jobs"]["panda-runtime"]);
    assert!(!commands.contains("python rust/tools/check_pandad_ci.py"));
    assert!(!commands.contains("rust/tools/tests/test_pandad_ci.py -q"));
    for path in [
        "rust/tools/check_pandad_ci.py",
        "rust/tools/tests/test_pandad_ci.py",
    ] {
        assert!(
            matches!(std::fs::symlink_metadata(root().join(path)), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "retired Panda source reappeared or could not be inspected: {path}"
        );
    }
    some_artifact(&workflow()["jobs"]["panda-runtime"]);
}
#[test]
fn calibration_requires_sealed_retirement_and_panda_requires_original_pairs() {
    job_check(
        "workspace",
        &[
            "test ! -e rust/tools/check_calibration_float.py",
            "test ! -L rust/tools/check_calibration_float.py",
            "--features calibration-float-native --bin check-calibration-float",
            "--features calibration-float-native --test calibration_float_check --test calibration_float_postdelete --locked",
            "--lib calibration_float_check::tests --locked",
            "rust/target/debug/check-calibration-float --binary",
            "CALIBRATION_FLOAT_BINARY=",
            "CALIBRATION_POSTDELETE_EVIDENCE=",
        ],
        &["--features calibration-float-native"],
        false,
    );
    job_check("panda-runtime", &[
        "--features panda-ci-native --test panda_ci_retirement --locked",
        "--features pandad-safety-padding-native --bin check-pandad-safety-padding",
        "--features pandad-safety-padding-native --test pandad_safety_padding_check --locked",
        "--features pandad-safety-padding-native --test pandad_safety_padding_reference --locked",
        "--features pandad-safety-padding-native --test pandad_safety_padding_absence --locked -- --nocapture",
        "--test pandad_safety_padding_finalize --no-run --locked",
        "--test pandad_safety_padding_check --locked --test pandad_safety_padding_finalize",
        "successful finalization accepted a mutated private native probe after execute returned",
        "test \"$status\" -eq 101",
        "PANDAD_PADDING_SOURCE_BINARY=",
        "PANDAD_PADDING_SOURCE_MANIFEST=",
        "PANDAD_PADDING_TRACE_SOURCE_BINARY=",
        "PANDAD_PADDING_TRACE_SOURCE_MANIFEST=",
        "--output \"$PANDAD_PADDING_TEST_OUTPUT/trace-source\" --capnp-prefix /usr --json11-prefix \"$json11_prefix\" --safety",
        "--test pandad_safety_padding_retirement --locked -- --nocapture",
        "PANDAD_PADDING_NATIVE_BINARY=",
        "PANDAD_PADDING_PYTHON=",
    ], &["--features pandad-safety-padding-native"], false);
    job_check(
        "panda-runtime",
        &[
            "--bin check-pandad-device --bin stage-pandad-device-reference",
            "--test pandad_device_check --test pandad_device_reference --test pandad_device_fixture --locked",
            "PANDAD_DEVICE_SOURCE_BINARY=",
            "PANDAD_DEVICE_SOURCE_MANIFEST=",
            "PANDAD_DEVICE_NATIVE_BINARY=",
            "PANDAD_DEVICE_SOURCE_BUILD_KIND=fresh",
            "PANDAD_DEVICE_RETIREMENT_MODE=postdelete",
            "stage-pandad-device-reference --root",
            "PANDAD_DEVICE_ORIGINAL_CAPTURE/capture-manifest.json",
            "PANDAD_DEVICE_CAPTURE_BUILDER_MANIFEST",
            "test ! -e rust/tools/check_pandad_device.py",
            "test ! -L rust/tools/check_pandad_device.py",
            "test ! -e rust/tools/pandad_device_cases.py",
            "test ! -L rust/tools/pandad_device_cases.py",
            "--features pandad-device-native --lib --bin check-pandad-device",
        ],
        &["--features pandad-device-native"],
        false,
    );
    job_check(
        "panda-runtime",
        &[
            "for file in check_pandad_protocol.py check_pandad_can_io.py pandad_protocol_cases.py",
            "test ! -e \"rust/tools/$file\"",
            "test ! -L \"rust/tools/$file\"",
            "PANDAD_PROTOCOL_SOURCE_BUILD_KIND=fresh",
            "PANDAD_CAN_IO_SOURCE_BUILD_KIND=fresh",
            "PANDAD_PROTOCOL_RETIREMENT_MODE=postdelete",
            "PANDAD_CAN_IO_RETIREMENT_MODE=postdelete",
            "stage-pandad-protocol-reference --root",
            "stage-pandad-can-io-reference --root",
            "--test pandad_protocol_reference --test pandad_protocol_fixture",
            "--test pandad_can_io_reference --test pandad_can_io_fixture",
            "--bin stage-pandad-protocol-reference --bin stage-pandad-can-io-reference",
        ],
        &["--features panda-ci-native,pandad-protocol-native,pandad-can-io-native"],
        false,
    );
}
#[test]
fn owner_and_buffered_write_repairs_require_real_old_failures_and_fixed_checks() {
    job_check(
        "workspace",
        &[
            "jetlink399-baseline",
            "11f3dec411662baa961733b363c0b38d7082fcfa",
            "--test owner_lifecycle --locked -- --nocapture",
            "failed_connect_preserves_foreign_listener failed_bind_preserves_foreign_listener",
            "full_stderr_cannot_extend_failed_stop full_stderr_cannot_extend_drop",
            "test \"$status\" -eq 101",
            "grep -Fx \"    $case\"",
            "test result: FAILED. 1 passed; 5 failed; 1 ignored; 0 measured; 0 filtered out;",
            "cargo build -p openpilot-jetlink --bins --examples --locked",
            "cargo clippy -p openpilot-jetlink --all-targets --locked -- -D warnings",
        ],
        &["--test owner_lifecycle"],
        false,
    );
    job_check(
        "ui-connectivity",
        &[
            "ui423-baseline",
            "CARGO_TARGET_DIR=\"$RUNNER_TEMP/ui423-target\" cargo test",
            "11f3dec411662baa961733b363c0b38d7082fcfa",
            "--exact actual_short_reading_encoder_rejects_partial_frame_and_keeps_queue_capacity",
            "grep -F 'test result: FAILED'",
            "grep -F 'short-reading child left a partial frame accepted as complete'",
        ],
        &["ui423-baseline"],
        false,
    );
}
#[test]
fn bluetooth_file_retirement_requires_fresh_absent_source_proof() {
    assert!(!root().join("rust/tools/check_bluetooth_files.py").exists());
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/check_bluetooth_files.py",
            "test ! -L rust/tools/check_bluetooth_files.py",
            "sha256sum \"$checker\" \"$BLUETOOTH_FILE_FIXTURE\"",
            "env PATH=/nonexistent \"$checker\" --binary \"$BLUETOOTH_FILE_FIXTURE\" --output \"$proof/absent\"",
            "sha256sum -c \"$proof/artifacts.sha256\"",
            "--features bluetooth-files-native --test bluetooth_files_check --locked",
            "strace",
        ],
        &["bluetooth-files-retirement"],
        false,
    );
}
#[test]
fn launcher_failed_handoff_requires_actual_old_failure_and_fixed_lifecycle() {
    job_check(
        "startup-runtime",
        &[
            "handoff424-baseline",
            "edb42bf095d6a3e3b38cc0a4546976dfb710363e",
            "--test handoff --no-run --locked",
            "timeout --signal=TERM --kill-after=2s 15s",
            "test \"$status\" -eq 101",
            "grep -F 'error cleanup exceeded bound'",
            "cargo test --manifest-path rust/Cargo.toml -p openpilot-runtime-launcher --test handoff --locked",
            "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-runtime-launcher --all-targets --locked -- -D warnings",
        ],
        &["--test handoff"],
        false,
    );
}
#[test]
fn every_gate_dependency_and_result_binding_is_required() {
    let original = workflow();
    let needs = original["jobs"]["fast"]["needs"].as_array().unwrap();
    for (index, dependency) in needs.iter().enumerate() {
        let mut modified = original.clone();
        modified["jobs"]["fast"]["needs"]
            .as_array_mut()
            .unwrap()
            .remove(index);
        assert!(
            validate_rust_gate(&modified).is_err(),
            "accepted missing dependency {}",
            dependency
        );
    }
    let bindings = original["jobs"]["fast"]["steps"][0]["env"]
        .as_object()
        .unwrap();
    for name in bindings.keys() {
        let mut removed = original.clone();
        removed["jobs"]["fast"]["steps"][0]["env"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        assert!(
            validate_rust_gate(&removed).is_err(),
            "accepted missing {name}"
        );
        let mut wrong = original.clone();
        wrong["jobs"]["fast"]["steps"][0]["env"][name] = "${{ needs.workspace.result }}".into();
        if name == "WORKSPACE" {
            wrong["jobs"]["fast"]["steps"][0]["env"][name] = "success".into();
        }
        assert!(
            validate_rust_gate(&wrong).is_err(),
            "accepted wrong binding {name}"
        );
    }
}

#[test]
fn retired_tool_checks_require_current_native_builds_and_keep_original_consumers() {
    let data = workflow();
    let workspace = &data["jobs"]["workspace"];
    required(
        &commands(workspace),
        &[
            "cargo build --workspace --examples --locked",
            "target/debug/check-msgq-sanitizers --target-dir",
        ],
    );
    mandatory(workspace, &["check-msgq-sanitizers"]);
    let joystick = &data["jobs"]["joystickd-runtime"];
    required(
        &commands(joystick),
        &[
            "-p openpilot-control-tools -p openpilot-dev-tools --bins --examples",
            "rust/target/debug/check-control-tools-policy",
            "joystickd_source.py",
            "control_tools_policy_source.py",
            "check_joystickd_runtime.py",
            "check_joystick_input_runtime.py",
        ],
    );
    mandatory(joystick, &["check-control-tools-policy"]);
    assert_eq!(
        commands(joystick)
            .matches("rust/target/debug/check-control-tools-policy")
            .count(),
        2
    );
    let startup = &data["jobs"]["startup-runtime"];
    required(
        &commands(startup),
        &[
            "-p openpilot-logmessaged -p openpilot-dev-tools --features openpilot-dev-tools/managed-entry-native,openpilot-dev-tools/managed-runtime-native,openpilot-dev-tools/registration-clock-native --bins --examples",
            "rust/target/debug/check-registration-vendor",
            "rust/target/debug/check-registration rust/target/debug/examples/registration_trace",
        ],
    );
    mandatory(startup, &["check-registration-vendor"]);
    let text = read(".github/workflows/rust.yml");
    assert!(!text.contains("check_registration_vendor.py"));
    assert!(!text.contains("check_msgq_sanitizers.py"));
    assert!(!text.contains("check_control_tools_policy.py"));
}

#[test]
fn managed_runtime_requires_all_seven_daemons_and_the_focused_startup_caller() {
    let data = workflow();
    let workspace = &data["jobs"]["workspace"];
    required(
        &commands(workspace),
        &[
            "cargo build --locked -p openpilot-controlsd -p openpilot-radard -p openpilot-plannerd -p openpilot-paramsd -p openpilot-proclogd -p openpilot-uploader -p openpilot-bluetooth -p openpilot-logmessaged -p openpilot-dev-tools --features openpilot-dev-tools/managed-runtime-native --bins",
            "export MANAGED_RUNTIME_BINARIES=\"$GITHUB_WORKSPACE/rust/target/debug\"",
            "export MANAGED_RUNTIME_CHECKER=\"$MANAGED_RUNTIME_BINARIES/check-managed-runtime\"",
            "cargo test --locked -p openpilot-dev-tools --features managed-runtime-native --test managed_runtime_check",
            "cargo test --locked -p openpilot-dev-tools --features managed-runtime-native --lib managed_runtime_check::tests",
            "cargo clippy --locked -p openpilot-dev-tools --features managed-runtime-native --lib --bin check-managed-runtime --test managed_runtime_check -- -D warnings",
        ],
    );
    mandatory(
        workspace,
        &[
            "--features openpilot-dev-tools/managed-runtime-native --bins",
            "--test managed_runtime_check",
        ],
    );
    let startup = &data["jobs"]["startup-runtime"];
    required(&commands(startup), &["openpilot-dev-tools/managed-runtime-native,openpilot-dev-tools/registration-clock-native --bins --examples", "rust/target/debug/check-managed-runtime --binaries rust/target/debug --daemon uploader"]);
    mandatory(startup, &["rust/target/debug/check-managed-runtime"]);
    assert!(!read(".github/workflows/rust.yml").contains("check_managed_runtime.py"));
}

#[test]
fn diagnostics_family_keeps_all_three_native_consumers_mandatory() {
    job_check(
        "workspace",
        &[
            "cargo build -p openpilot-dev-tools --bin check-runtime-diagnostics --bin check-communication-snapshot --bin check-model-timing --locked",
            "target/debug/check-runtime-diagnostics --binary target/debug/examples/diagnostics_trace",
            "target/debug/check-communication-snapshot --binary target/debug/examples/communication_trace",
            "cargo build -p openpilot-driving-modeld --example model_timing_trace --locked",
            "target/debug/check-model-timing --binary target/debug/examples/model_timing_trace",
        ],
        &["--bin check-runtime-diagnostics", "target/debug/check-runtime-diagnostics --binary", "target/debug/check-communication-snapshot --binary", "target/debug/check-model-timing --binary"],
        false,
    );
    let data = workflow();
    let workspace = &data["jobs"]["workspace"];
    some_artifact(workspace);
    let evidence = workspace["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|step| {
            step["with"]["path"]
                .as_str()
                .is_some_and(|path| path.contains("runtime-diagnostics/report.json"))
        })
        .unwrap();
    assert!(
        evidence["if"] == TRUSTED_SOURCE,
        "preserve original successful-run artifact publication"
    );
    required(
        evidence["with"]["path"].as_str().unwrap(),
        &[
            "runtime-diagnostics/report.json",
            "communication-snapshot/report.json",
            "model-timing/",
        ],
    );
}

#[test]
fn command_reader_gates_keep_both_native_consumers_and_desire_check_mandatory() {
    job_check(
        "workspace",
        &[
            "rust/target/debug/check-desire-reference-native --binary \"$DESIRE_REFERENCE_PROBE\"",
            "cargo build -p openpilot-dev-tools --bin check-command-reference --bin check-command-json-boundary --locked",
            "target/debug/check-command-reference --binary target/debug/examples/command_probe",
            "target/debug/check-command-json-boundary --binary target/debug/examples/command_probe",
        ],
        &["--bin check-command-reference", "target/debug/check-command-reference --binary", "target/debug/check-command-json-boundary --binary", "--bin check-desire-reference-native", "rust/target/debug/check-desire-reference-native --binary"],
        false,
    );
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    let run = commands(job);
    assert!(!run.contains("python tools/check_command_reference.py"));
    assert!(!run.contains("python tools/check_command_json_boundary.py"));
    assert!(job["steps"].as_array().unwrap().iter().any(|step| {
        step["uses"] == "actions/upload-artifact@v6"
            && step["if"] == TRUSTED_SOURCE
            && step["with"]["path"]
                .as_str()
                .is_some_and(|path| path.contains("model-commands/report.json"))
    }));
}

#[test]
fn workspace_preserves_complete_proclog_packets_and_checker_ownership() {
    job_check(
        "workspace",
        &[
            "cargo build -p openpilot-dev-tools --features proclog-reference-native --bin check-proclog-reference --locked",
            "cargo build -p openpilot-proclogd --example reference_trace --locked",
            "PROCLOG_REFERENCE_TRACE=\"$PWD/target/debug/examples/reference_trace\" cargo test -p openpilot-dev-tools --features proclog-reference-native --test proclog_reference_check --locked",
            "cargo clippy -p openpilot-dev-tools --features proclog-reference-native --lib --bin check-proclog-reference --test proclog_reference_check --locked -- -D warnings",
            "target/debug/check-proclog-reference --binary target/debug/examples/reference_trace",
            "python tools/check_proclog_daemon.py",
        ],
        &["--bin check-proclog-reference", "--test proclog_reference_check", "target/debug/check-proclog-reference --binary"],
        false,
    );
    let data = workflow();
    assert!(!commands(&data["jobs"]["workspace"]).contains("check_proclog_reference.py"));
    job_check(
        "workspace",
        &[
            "cargo build -p openpilot-dev-tools -p openpilot-proclogd --features openpilot-dev-tools/aarch64-check-native --bin check-aarch64 --bin openpilot-proclogd --locked",
            "AARCH64_HOST_PROBE=\"$PWD/target/debug/openpilot-proclogd\" cargo test -p openpilot-dev-tools --features aarch64-check-native --test aarch64_check --locked",
            "cargo clippy -p openpilot-dev-tools --features aarch64-check-native --lib --bin check-aarch64 --test aarch64_check --locked -- -D warnings",
        ],
        &["--features openpilot-dev-tools/aarch64-check-native", "--test aarch64_check", "--bin check-aarch64"],
        false,
    );
    let arm_job = &data["jobs"]["arm64"];
    // Keep the inherited PR/protected-push routing. Only individual checker
    // steps must be unconditional within that existing required ARM job.
    assert_eq!(arm_job["if"], "github.event_name != 'push' || github.ref == 'refs/heads/dev' || github.ref == 'refs/heads/main'");
    let mut required_arm = arm_job.clone();
    required_arm.as_object_mut().unwrap().remove("if");
    mandatory(
        &required_arm,
        &[
            "--features aarch64-check-native --bin check-aarch64",
            "target/debug/check-aarch64",
        ],
    );
    required(
        &commands(arm_job),
        &[
            "cargo zigbuild --workspace --release --locked --features openpilot-loggerd/static-ffmpeg --target aarch64-unknown-linux-musl",
            "cargo build -p openpilot-dev-tools --features aarch64-check-native --bin check-aarch64 --locked",
            "target/debug/check-aarch64",
            "--binary target/aarch64-unknown-linux-musl/release/openpilot-proclogd",
            "--runner /usr/bin/qemu-aarch64-static --report \"$RUNNER_TEMP/aarch64-probe.json\"",
        ],
    );
    assert!(data["jobs"]["arm64"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| {
            step["uses"] == "actions/upload-artifact@v6"
                && step["with"]["name"] == "rust-c3x-static-probe-pending-device-test"
                && step["with"]["path"] == "${{ env.CI296_SOURCE }}/rust/target/c3x-probe/"
                && step["if"] == TRUSTED_SOURCE
        }));
    let arm = commands(&data["jobs"]["arm64"]);
    assert!(!arm.contains("python tools/check_aarch64.py"));
    let build=arm.find("cargo build -p openpilot-dev-tools --features aarch64-check-native --bin check-aarch64 --locked").unwrap();
    assert!(build < arm.find("target/debug/check-aarch64").unwrap());
}

#[test]
fn dynamic_cpp_params_interoperability_gate_keeps_native_build_and_execution() {
    job_check(
        "workspace",
        &[
            "cargo build -p openpilot-dev-tools --bin check-params-reference --locked",
            "target/debug/check-params-reference",
        ],
        &[
            "--bin check-params-reference",
            "target/debug/check-params-reference",
        ],
        false,
    );
    assert!(!commands(&workflow()["jobs"]["workspace"])
        .contains("python tools/check_params_reference.py"));
}

#[test]
fn model_publication_comparison_reads_generation_through_verified_native_catalog() {
    let data = workflow();
    let steps = data["jobs"]["model-pipelines"]["steps"].as_array().unwrap();
    let validated = steps
        .iter()
        .position(|step| step["name"] == "Build and validate immutable native pipeline catalog")
        .unwrap();
    let compared = steps
        .iter()
        .position(|step| {
            step["name"] == "Compare model publications with the original Python implementations"
        })
        .unwrap();
    assert!(validated < compared);
    assert!(steps[validated]["run"]
        .as_str()
        .unwrap()
        .contains("model-run --check-catalog \"$RUNNER_TEMP/native-catalog\""));
    let run = steps[compared]["run"].as_str().unwrap();
    let generation = run.find("generation=$(rust/target/debug/model-run --catalog-generation \"$RUNNER_TEMP/native-catalog\")").unwrap();
    let compare = run
        .find("python rust/tools/check_model_outputs.py")
        .unwrap();
    assert!(generation < compare);
    assert!(run.contains("for resolution in '1928 1208' '1344 760'"));
    assert!(run.contains("--driving-pipeline \"$RUNNER_TEMP/native-catalog/$generation/driving-${1}x${2}/pipeline.json\""));
    assert!(run.contains("--driver-pipeline \"$RUNNER_TEMP/native-catalog/$generation/driver-${1}x${2}/pipeline.json\""));
    assert!(!run.contains("python -c"));
}

#[test]
fn telemetry_retains_metadata_error_transport_across_calls_and_threads() {
    job_check(
        "telemetry-runtime",
        &[
            "rustup toolchain install 1.94.0 --profile minimal --component clippy",
            "--test runtime_producer --locked",
            "--test retained_facade --locked",
            "--test error_log_lifetime --locked",
            "--features stats-producer-native --bin check-stats-producer --locked",
            "--features stats-producer-native --test stats_producer_check --locked",
            "--features stats-producer-native --lib --test stats_producer_check --bin check-stats-producer --locked -- -D warnings",
            "STATS_PRODUCER_CHECKER=\"$PWD/rust/target/debug/check-stats-producer\"",
            "STATS_PRODUCER_EXAMPLES=\"$PWD/rust/target/debug/examples\"",
            "rust/target/debug/check-stats-producer rust/target/debug/examples",
            "--features stats-daemon-native --bin check-stats-daemon --bin check-stats-failures --bin check-stats-log-fault --locked",
            "--features stats-daemon-native --test stats_daemon_check --locked",
            "--features stats-daemon-native --lib --test stats_daemon_check --bin check-stats-daemon --bin check-stats-failures --bin check-stats-log-fault --locked -- -D warnings",
            "STATS_DAEMON_BINARIES=\"$PWD/rust/target/debug\"",
            "STATS_DAEMON_CHECKER=\"$PWD/rust/target/debug/check-stats-daemon\"",
            "rust/target/debug/check-stats-daemon rust/target/debug",
            "rust/target/debug/check-stats-failures rust/target/debug",
            "rust/target/debug/check-stats-log-fault rust/target/debug",
            "rust/target/debug/check-stats-interrupted rust/target/debug/examples/stats_daemon",
            "--features stats-daemon-native --lib --test stats_interrupted_check --locked",
            "--lib --test stats_interrupted_check --bin check-stats-interrupted --locked -- -D warnings",
            "--features openpilot-dev-tools/version-tools-native",
            "--test version_source --test version_check --test version_collector_check --locked",
            "--bin check-version-reference --bin check-version-collector --bin version-runtime-worker",
            "rust/target/debug/check-version-reference",
            "rust/target/debug/check-version-collector",
            "openpilot-dev-tools/tombstone-live-native,openpilot-dev-tools/sdk-transport-native",
            "--test tombstone_source --test tombstone_reference_check --test tombstone_safety_check --test sdk_transport_check --test sdk_transport_runtime --locked",
            "--features tombstone-live-native,sdk-transport-native --lib tombstone_ --locked",
            "--bin check-tombstoned-reference --bin check-tombstoned-safety --bin check-crash-sdk-transport",
            "--test tombstone_runtime_check --test tombstone_shutdown_check --locked",
            "--bin check-tombstoned-runtime --bin check-tombstoned-shutdown --bin tombstone-runtime-worker",
            "rust/target/debug/check-tombstoned-safety",
            "rust/target/debug/check-tombstoned-runtime",
            "rust/target/debug/check-tombstoned-shutdown",
            "rust/target/debug/check-tombstoned-reference",
            "rust/target/debug/check-crash-sdk-transport",
        ],
        &[
            "cargo test",
            "rust/target/debug/check-version-reference",
            "rust/target/debug/check-stats-producer",
            "rust/target/debug/check-version-collector",
            "rust/target/debug/check-tombstoned-reference",
            "rust/target/debug/check-crash-sdk-transport",
            "rust/target/debug/check-tombstoned-safety",
            "rust/target/debug/check-tombstoned-runtime",
            "rust/target/debug/check-tombstoned-shutdown",
        ],
        false,
    );
    let data = workflow();
    let telemetry = &data["jobs"]["telemetry-runtime"];
    let script = commands(telemetry);
    for retired_call in [
        "check_stats_producer.py",
        "stats_source_producer.py",
        "check_stats_daemon.py",
        "check_stats_failures.py",
        "check_stats_log_fault.py",
        "check_version_reference.py",
        "check_version_collector.py",
        "check_tombstoned_reference.py",
        "check_crash_sdk_transport.py",
        "check_tombstoned_safety.py",
        "check_tombstoned_runtime.py",
        "check_tombstoned_shutdown.py",
    ] {
        assert!(
            !script.contains(retired_call),
            "legacy Version caller: {retired_call}"
        );
    }
    let steps = telemetry["steps"].as_array().unwrap();
    let build = steps
        .iter()
        .position(|step| {
            step["run"].as_str().is_some_and(|run| {
                run.contains("--features openpilot-dev-tools/version-tools-native")
            })
        })
        .unwrap();
    let compare = steps
        .iter()
        .position(|step| {
            step["name"] == "Compare cached Git metadata and actual collector delivery"
        })
        .unwrap();
    assert!(
        build < compare,
        "Version tools must be built before tests and live comparison"
    );
    for (job, target) in [
        ("gnss-runtime", "--test assistance_log_lifetime --locked"),
        ("platform-runtime", "--test params_log_lifetime --locked"),
    ] {
        job_check(job, &[target], &[target], false);
    }
    for job in [
        "telemetry-runtime",
        "startup-services",
        "gnss-runtime",
        "platform-runtime",
    ] {
        for key in [
            "CARGO_INCREMENTAL",
            "CARGO_PROFILE_DEV_DEBUG",
            "CARGO_PROFILE_TEST_DEBUG",
        ] {
            assert_eq!(
                data["jobs"][job]["env"][key], "0",
                "bounded native ownership test artifacts: {job}/{key}"
            );
        }
    }
}

#[test]
fn support_metadata_capture_preserves_total_deadline_and_drains_both_pipes() {
    job_check(
        "support-runtime",
        &["--test git_capture --locked", "check_dashcam_metadata.py"],
        &["--test git_capture", "check_dashcam_metadata.py"],
        false,
    );
}

#[test]
fn registration_clock_requires_actual_probe_and_all_native_failure_controls() {
    let data = workflow();
    let startup = &data["jobs"]["startup-runtime"];
    required(&commands(startup), &[
        "openpilot-dev-tools/registration-clock-native --bins --examples",
        "export REGISTRATION_CLOCK_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/examples/registration_clock\"",
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features registration-clock-native --test registration_clock_check --locked",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features registration-clock-native --lib --bin check-registration-clock --test registration_clock_check --locked -- -D warnings",
        "rust/target/debug/check-registration-clock \"$REGISTRATION_CLOCK_BINARY\" \"$RUNNER_TEMP/startup-runtime/registration-clock.json\"",
    ]);
    mandatory(
        startup,
        &[
            "--test registration_clock_check",
            "rust/target/debug/check-registration-clock",
        ],
    );
    assert!(!read(".github/workflows/rust.yml").contains("check_registration_clock.py"));
}

#[test]
fn registration_and_navd_require_native_full_policy_and_real_delayed_collector() {
    let data = workflow();
    let startup = &data["jobs"]["startup-runtime"];
    assert_eq!(startup["timeout-minutes"], "60");
    mandatory(
        startup,
        &[
            "--features registration-tools-native",
            "rust/target/debug/check-registration ",
            "REGISTRATION_COLLECTOR_DELAY_MS=800",
        ],
    );
    let run = commands(startup);
    required(&run, &[
        "REGISTRATION_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/examples/registration_trace\"",
        "REGISTRATION_WORKER_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/check-registration\"",
        "--features registration-tools-native --bin check-registration --locked",
        "--features registration-tools-native --lib registration_check:: --locked",
        "--features registration-tools-native --test registration_check --test registration_reference --locked",
        "--features registration-tools-native --lib --bin check-registration --test registration_check --test registration_reference --locked -- -D warnings",
        "\"$RUNNER_TEMP/startup-runtime/registration\"",
        "\"$RUNNER_TEMP/startup-runtime/registration-timeouts\" --timeouts",
        "REGISTRATION_COLLECTOR_DELAY_MS=800 rust/target/debug/check-registration",
        "--case clock_year_ten_thousand",
        "build_msgq_python.py", "build_params_python.py", "check_process_supervision.py", "rust/target/debug/check-uploader-reference",
    ]);
    mandatory(startup, &["rust/target/debug/check-uploader-reference"]);
    assert!(!run.contains("check_registration.py"));
    assert!(!run.contains("LD_PRELOAD="));
    let navd = &data["jobs"]["navd-runtime"];
    mandatory(
        navd,
        &[
            "--features navd-config-tools-native",
            "rust/target/debug/check-navd-config",
        ],
    );
    let run = commands(navd);
    required(&run, &[
        "NAVD_CONFIG_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/examples/navd_config\"",
        "NAVD_CONFIG_WORKER_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/check-navd-config\"",
        "--features navd-config-tools-native --bin check-navd-config --bin check-registration -p openpilot-registration --example registration_trace --locked",
        "--features navd-config-tools-native --test navd_config_check --test navd_config_reference --test registration_navd_retirement --locked",
        "--features navd-config-tools-native --lib navd_config_check::publication::tests --locked",
        "--features navd-config-tools-native --lib --bin check-navd-config --test navd_config_check --test navd_config_reference --test registration_navd_retirement --locked -- -D warnings",
        "rust/target/debug/check-navd-config --binary rust/target/debug/examples/navd_config --output",
        "check_navd_policy.py", "check_navd_engine.py", "check_navd_http.py", "--timeouts", "check_navd_destination.py", "check_navd_ipc.py", "build_msgq_python.py", "build_params_python.py",
    ]);
    assert!(!run.contains("check_navd_config.py"));
    // This real CLI integration test validates source AND candidate schedules.
    required(
        &read("rust/crates/dev-tools/tests/registration_check.rs"),
        &[
            "fn actual_cli_delayed_collector_preserves_both_sides_headers_and_event_order()",
            "NativeRunner.run(&spec)",
            "REGISTRATION_COLLECTOR_DELAY_MS",
            "clock_year_ten_thousand",
            "schedules, expected,",
            "exactly two real source/native schedules",
            "assert_eq!(schedule[\"requested_delay_ms\"], 800)",
            "before[0].as_u64().unwrap() >= 2",
            "assert_eq!(before[2], 0)",
            "before[83..]",
            "assert_eq!(reset[0], 0)",
            "assert_ne!(reset[2], 0)",
            "reset[83..].iter().all",
            "refreshed[0].as_u64().unwrap() > 0",
            "assert_eq!(refreshed[2], reset[2])",
            "refreshed[83..]",
            "released - subscribed >= 0.8",
            "acknowledged >= released",
            "refreshed_at >= acknowledged",
        ],
    );
    // Keep the full independent original corpus, timeout group and real header assertions.
    required(&read("rust/crates/dev-tools/tests/registration_reference.rs"), &[
        "all_131_original_policies_keep_real_params_signed_requests_logs_and_collector_shutdown",
        "(Suite::All, \"matrix\", 131)", "(Suite::Timeouts, \"timeouts\", 3)",
        "delayed_real_error_publisher_resets_then_refreshes_the_actual_reader_before_error_workload",
        "before[0].as_u64().unwrap() >= 2", "reset[0], 0", "refreshed[2], reset[2]", "released - subscribed >= 0.8",
    ]);
}

#[test]
fn production_statistics_require_actual_entrypoint_and_busy_shutdown_checks() {
    let data = workflow();
    let job = &data["jobs"]["telemetry-runtime"];
    mandatory(
        job,
        &[
            "--test stats_runtime_check",
            "rust/target/debug/check-stats-runtime",
            "rust/target/debug/check-stats-flood",
        ],
    );
    let run = commands(job);
    required(&run, &[
        "--features stats-daemon-native --bin check-stats-runtime --bin check-stats-flood --locked",
        "STATS_RUNTIME_BINARIES=\"$PWD/rust/target/debug\"",
        "STATS_RUNTIME_CHECKER=\"$PWD/rust/target/debug/check-stats-runtime\"",
        "STATS_FLOOD_CHECKER=\"$PWD/rust/target/debug/check-stats-flood\"",
        "--features stats-daemon-native --test stats_runtime_check --locked",
        "--lib --test stats_runtime_check --bin check-stats-runtime --bin check-stats-flood --locked -- -D warnings",
        "rust/target/debug/check-stats-runtime rust/target/debug \"$RUNNER_TEMP/telemetry-runtime/stats-runtime\"",
        "rust/target/debug/check-stats-flood rust/target/debug \"$RUNNER_TEMP/telemetry-runtime/stats-flood\"",
        "build_msgq_python.py", "build_params_python.py",
        "rust/target/debug/check-stats-interrupted",
    ]);
    assert!(!run.contains("check_stats_runtime.py"));
    assert!(!run.contains("check_stats_flood.py"));
}

#[test]
fn bridge_retirement_requires_actual_source_absence_and_same_elf_native_trace() {
    job_check(
        "startup-services",
        &[
            "test ! -e rust/tools/check_bridge.py",
            "test ! -L rust/tools/check_bridge.py",
            "--test bridge_controller_reference --locked",
            "--lib bridge_check:: --locked",
            "BRIDGE_ABSENCE_PROOF=\"$proof\"",
            "OPENPILOT_RUST_ROOT=$proof/native-root",
            "PATH=$proof/native-path",
            "timeout --signal=TERM --kill-after=5s 300s",
            "trace=execve,execveat",
            "sha256sum -c \"$proof/artifacts.sha256\"",
        ],
        &["--test bridge_controller_reference"],
        false,
    );
}

#[test]
fn paramsd_full_native_controller_requires_live_source_packet_comparison() {
    job_check(
        "estimation-runtime",
        &[
            "python rust/tools/check_paramsd_daemon.py",
            "--features paramsd-daemon-native --bin check-paramsd-daemon-native",
            "--test paramsd_daemon_check --locked",
            "--test paramsd_daemon_reference --locked -- --nocapture",
            "PARAMSD_ORIGINAL_OUTPUT=",
            "PARAMSD_NATIVE_OUTPUT=",
            "continuous/input.json",
            "restart/input.json",
            "continuous/source.json",
            "restart/source.json",
            "180s \"$checker\" --daemon",
            "sha256sum -c \"$proof/inputs.sha256\"",
        ],
        &["native-controller-proof"],
        true,
    );
}

#[test]
fn soundd_retirement_requires_both_traced_native_lanes_and_raw_reference() {
    job_check(
        "sensor-audio",
        &[
            "test ! -e rust/tools/check_soundd_daemon.py",
            "SOUNDD_NATIVE_PRESENT_EXEC=",
            "SOUNDD_NATIVE_ABSENT_EXEC=",
            "for lane in PRESENT ABSENT; do",
            "PATH=/nonexistent /usr/bin/timeout",
            "-e trace=execve,execveat",
            "test ! -L \"$SOUNDD_NATIVE_ABSENT_ROOT/rust/tools/check_soundd_daemon.py\"",
            "--test soundd_daemon_retirement --locked -- --nocapture",
            "sha256sum -c \"$proof/executables.sha256\"",
        ],
        &["soundd-retirement"],
        true,
    );
}

#[test]
fn journal_retirement_requires_sealed_original_all_rows_and_two_native_exec_lanes() {
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/check_bluetooth_journal.py",
            "test ! -L rust/tools/check_bluetooth_journal.py",
            "BLUETOOTH_JOURNAL_ARTIFACTS_BEFORE=",
            "BLUETOOTH_JOURNAL_ARTIFACTS_AFTER=",
            "BLUETOOTH_JOURNAL_ABSENCE_ROOT=",
            "original-observation-39507901490.json",
            "openpilot/selfdrive/carrot/bluetooth/model.py rust/crates/bluetooth/examples/journal_fixture.rs",
            "for lane in PRESENT ABSENT; do",
            "-e trace=execve,execveat -o \"${!exec_var}\"",
            "--test bluetooth_journal_reference --locked -- --nocapture",
        ],
        &["bluetooth-journal-retirement"],
        false,
    );
}

#[test]
fn gesture_retirement_keeps_sealed_original_inputs_and_fresh_private_consumer() {
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/bluetooth_gesture_cases.py",
            "b5eefe42e5f5fb135b71ef50e97a7905d5f41a4d943b7b5e3bcebddf1d0d8166",
            "--input \"$proof/original-input.jsonl\"",
            "BLUETOOTH_GESTURE_GENERATOR_BINARY=",
            "BLUETOOTH_GESTURE_ABSENT_CHECK_OUTPUT=",
            "PYTHONPATH=\"$BLUETOOTH_GESTURE_ABSENT_ROOT\"",
            "test ! -L \"$BLUETOOTH_GESTURE_ABSENT_ROOT/rust/tools/bluetooth_gesture_cases.py\"",
            "--test bluetooth_gesture_cases_retirement --locked -- --nocapture",
        ],
        &["bluetooth-gesture-cases"],
        false,
    );
}

#[test]
fn config_retirement_requires_sealed_original_all_cases_and_both_exec_lanes() {
    job_check(
        "ui-connectivity",
        &[
            "test ! -e rust/tools/check_bluetooth_config.py",
            "BLUETOOTH_CONFIG_ARTIFACTS_SHA256=",
            "BLUETOOTH_CONFIG_PRESENT_EXEC=",
            "BLUETOOTH_CONFIG_ABSENT_EXEC=",
            "test ! -L \"$BLUETOOTH_CONFIG_ABSENT_ROOT/rust/tools/check_bluetooth_config.py\"",
            "OPENPILOT_RUST_ROOT=\"$BLUETOOTH_CONFIG_ABSENT_ROOT\" PATH=/nonexistent",
            "sha256sum -c \"$BLUETOOTH_CONFIG_ARTIFACTS_SHA256\"",
            "--test bluetooth_config_reference --locked -- --nocapture",
        ],
        &["bluetooth-config-retirement"],
        false,
    );
}

#[test]
fn retired_bluetooth_helpers_are_absent_at_checkout() {
    for helper in [
        "rust/tools/check_bluetooth_config.py",
        "rust/tools/check_bluetooth_journal.py",
        "rust/tools/bluetooth_gesture_cases.py",
    ] {
        assert!(
            matches!(
                fs::symlink_metadata(root().join(helper)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound
            ),
            "retired helper still present: {helper}"
        );
    }
}

#[test]
fn paramsd_fixture_conversion_keeps_fresh_original_wire_and_real_cli_proof() {
    assert!(workflow()["jobs"]["estimation-runtime"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["name"]
            == "Compare native paramsd fixture generator against all original wire inputs"));
    job_check(
        "estimation-runtime",
        &[
            "from paramsd_fixture import car_params, scenarios",
            "from check_paramsd_daemon import frames",
            "bzip2 -dc rust/crates/dev-tools/tests/fixtures/paramsd-loop-retired-original/input.json.bz2",
            "4f446bc98c9d303d292f487c9ebe36bc8721436a30627dfca55baabae4c6811d",
            "assert json.loads(observed.read_text()) == case",
            "PATH=/nonexistent /usr/bin/timeout --kill-after=5s 30s /usr/bin/strace",
            "--features paramsd-fixture-native --test paramsd_fixture_generate --test paramsd_loop_fixture_generate --locked",
            "--features paramsd-fixture-native --lib --bin generate-paramsd-fixture --bin generate-paramsd-loop-fixture --test paramsd_fixture_generate --test paramsd_loop_fixture_generate --locked -- -D warnings",
            "sha256sum -c \"$proof/original.sha256\"",
            "sha256sum -c \"$proof/generator.sha256\"",
        ],
        &["--features paramsd-fixture-native"],
        true,
    );
}

#[test]
fn paramsd_loop_native_controller_keeps_original_and_full_sealed_comparison() {
    job_check(
        "estimation-runtime",
        &[
            "test ! -e rust/tools/check_paramsd_loop.py",
            "test ! -L rust/tools/check_paramsd_loop.py",
            "test ! -e rust/tools/paramsd_loop_fixture.py",
            "test ! -L rust/tools/paramsd_loop_fixture.py",
            "PARAMSD_LOOP_RETIREMENT_OUTPUT=",
            "--features paramsd-loop-native --bin check-paramsd-loop-native --locked",
            "PARAMSD_LOOP_NATIVE_RAW=",
            "--features paramsd-loop-native --lib --test paramsd_loop_check --test paramsd_loop_reference --test paramsd_loop_retirement --locked",
            "--bin check-paramsd-loop-native --test paramsd_loop_check --test paramsd_loop_reference --test paramsd_loop_retirement --locked -- -D warnings",
            "PATH=/nonexistent /usr/bin/timeout --signal=TERM --kill-after=5s 200s /usr/bin/strace",
            "sha256sum -c \"$proof/inputs.sha256\"",
        ],
        &["native-loop-controller"],
        true,
    );
}

#[test]
fn logmessaged_startup_retirement_keeps_sealed_original_and_same_elf_absent_proof() {
    let data = workflow();
    let job = &data["jobs"]["support-runtime"];
    let steps = job["steps"].as_array().unwrap();
    let selected: Vec<_> = steps
        .iter()
        .filter(|step| {
            step["name"] == "Require same-ELF logmessaged startup with historical checker absent"
        })
        .collect();
    assert_eq!(selected.len(), 1);
    trusted_step(selected[0]);
    required(
        selected[0]["run"].as_str().unwrap(),
        &[
            "LOGMESSAGED_STARTUP_RETIREMENT_CURRENT_ROOT=",
            "for lane in current absent",
            "test ! -e rust/tools/check_logmessaged_startup.py",
            "test ! -L rust/tools/check_logmessaged_startup.py",
            "test ! -e \"$roots/absent/rust/tools/check_logmessaged_startup.py\"",
            "test ! -L \"$roots/absent/rust/tools/check_logmessaged_startup.py\"",
            "-e trace=execve,execveat",
            "sha256sum -c \"$proof/inputs.sha256\"",
            "cmp \"$proof/inputs.stat\" \"$proof/inputs-after.stat\"",
            "--features logmessaged-startup-native --test logmessaged_startup_retirement --locked",
        ],
    );
    required(
        &read("rust/crates/dev-tools/tests/logmessaged_startup_retirement.rs"),
        &[
            "ORIGINAL_MANIFEST_SHA256",
            "records.lines().count(), 18",
            "original_fixture()",
            "packets(&original.join(case), service)",
            "disk(&original.join(case))",
            "same_elf_current_and_absent_lanes_match_sealed_original_startup",
        ],
    );
    artifacts(job);
}

#[test]
fn lpa_cookie_header_keeps_original_tls_and_requires_measured_byte_exact_improvement() {
    job_check("startup-services", &[
        "python rust/tools/check_lpa.py --target",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-lpa --all-targets --locked -- -D warnings",
        "rustc --edition=2021 -O -D warnings rust/tools/lpa_cookie_header_allocation_probe.rs",
        "\"$proof/lpa-cookie-header-allocation-probe\" --require-improvement",
        "sha256sum -c \"$proof/source.sha256\"",
        "sha256sum -c \"$proof/binary.sha256\"",
    ], &["lpa_cookie_header_allocation_probe.rs"], true);
    required(
        &read("rust/crates/lpa/src/http.rs"),
        &[
            "matched.retain(|cookie| cookie.secure() != Some(true) || url.scheme() == \"https\")",
            "matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.as_ref().len()))",
            "cookies::header(matched.iter().map(|cookie| (cookie.name(), cookie.value())))",
        ],
    );
}

#[test]
fn telemetry_combined_library_tests_receive_their_built_tombstone_probe() {
    let data = workflow();
    let steps = data["jobs"]["telemetry-runtime"]["steps"]
        .as_array()
        .unwrap();
    let build = steps
        .iter()
        .position(|step| step["name"] == "Build telemetry daemons and source-comparison probes")
        .unwrap();
    assert!(steps[build]["run"]
        .as_str()
        .unwrap()
        .contains("-p openpilot-tombstoned"));
    assert!(steps[build]["run"]
        .as_str()
        .unwrap()
        .contains("--bins --examples --locked"));
    let paired = steps
        .iter()
        .position(|step| {
            step["name"] == "Compare cached Git metadata and actual collector delivery"
        })
        .unwrap();
    assert!(build < paired);
    let run = steps[paired]["run"].as_str().unwrap();
    let export = run.find("export TOMBSTONE_PROBE_BINARY=\"$GITHUB_WORKSPACE/rust/target/debug/examples/tombstone_trace\"").unwrap();
    let verify = run.find("test -x \"$TOMBSTONE_PROBE_BINARY\"").unwrap();
    let test = run.find("cargo test ").unwrap();
    assert!(export < verify && verify < test);
}

#[test]
fn beep_native_controller_keeps_all_twenty_original_cases_and_real_caller() {
    job_check("telemetry-runtime", &[
        "openpilot-dev-tools/beep-reference-native",
        "BEEP_REFERENCE_ORIGINAL_MANIFEST_SHA256=0a02d6aacb5b930012a47886c389571d9cf3ff404ecd796171045f10734a9188",
        "BEEP_REFERENCE_EVIDENCE=",
        "BEEP_REFERENCE_RETIREMENT_EVIDENCE=",
        "BEEP_REFERENCE_EXPECT_ABSENT=1",
        "BEEP_REFERENCE_CHECKER_ABSENT=1",
        "test ! -e rust/tools/check_beepd_reference.py",
        "test ! -L rust/tools/check_beepd_reference.py",
        "--locked --lib --test beep_reference_check --test beep_reference_retirement",
        "--bin check-beep-reference --test beep_reference_check --test beep_reference_retirement",
        "rust/target/debug/check-beep-reference --binary",
    ], &["--test beep_reference_check", "--test beep_reference_retirement"], false);
    let run = commands(&workflow()["jobs"]["telemetry-runtime"]);
    assert!(!run.contains("python rust/tools/check_beepd_reference.py"));
}

#[test]
fn calibration_estimator_retirement_keeps_complete_sealed_oracle_and_native_caller() {
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    let steps = job["steps"].as_array().unwrap();
    let selected = steps
        .iter()
        .filter(|step| {
            step["name"]
                == "Verify retired calibration estimator against full original observations"
        })
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    trusted_step(selected[0]);
    required(selected[0]["run"].as_str().unwrap(), &[
        "CALIBRATION_REFERENCE_BINARY=",
        "CALIBRATION_REFERENCE_CHECKER_BINARY=",
        "CALIBRATION_REFERENCE_EVIDENCE=",
        "CALIBRATION_REFERENCE_RETIREMENT_EVIDENCE=",
        "CALIBRATION_REFERENCE_EXPECT_ABSENT=1",
        "test ! -e rust/tools/check_calibration_reference.py",
        "test ! -L rust/tools/check_calibration_reference.py",
        "test ! -e rust/tools/calibration_cases.py",
        "test ! -L rust/tools/calibration_cases.py",
        "--features calibration-reference-native --bin check-calibration-reference --locked",
        "--features calibration-reference-native --lib --test calibration_reference_check --test calibration_reference_retirement --locked -- --nocapture",
        "--features calibration-reference-native --lib --bin check-calibration-reference --test calibration_reference_check --test calibration_reference_retirement --locked -- -D warnings",
        "rust/target/debug/check-calibration-reference --binary \"$CALIBRATION_REFERENCE_BINARY\" --output",
    ]);
    assert!(!commands(job).contains("python tools/check_calibration_reference.py"));
    let artifacts = steps
        .iter()
        .filter(|step| step["name"] == "Retain complete calibration estimator retirement evidence")
        .collect::<Vec<_>>();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["if"], TRUSTED_ALWAYS);
    assert_eq!(artifacts[0]["uses"], "actions/upload-artifact@v6");
    assert_eq!(
        artifacts[0]["with"]["name"],
        "rust-calibration-estimator-retirement"
    );
    assert_eq!(
        artifacts[0]["with"]["path"],
        "${{ env.CI_JOB_TEMP }}/calibration-estimator-native/\n"
    );
}

#[test]
fn retired_updater_controller_keeps_all_original_cases_and_remaining_live_gates() {
    let data = workflow();
    let job = &data["jobs"]["startup-services"];
    let steps = job["steps"].as_array().unwrap();
    let step = steps
        .iter()
        .find(|step| {
            step["name"] == "Verify retired updater controller against all original lifecycle cases"
        })
        .unwrap();
    trusted_step(step);
    required(step["run"].as_str().unwrap(), &[
        "for source in rust/tools/check_updated.py rust/tools/updated_source.py rust/tools/tests/test_updated_comparison.py rust/tools/updated_fixtures.py; do",
        "test ! -e \"$source\"", "test ! -L \"$source\"",
        "UPDATED_CHECK_BINARY=", "UPDATED_CHECK_LAUNCHER=", "UPDATED_CHECK_SHIM=",
        "UPDATED_CHECK_OUTPUT=", "UPDATED_CHECK_CLI_OUTPUT=", "UPDATED_CHECK_ABSENCE_EVIDENCE=",
        "UPDATED_FIXTURE_NATIVE_OUTPUT=", "UPDATED_FIXTURE_REQUIRE_ABSENT=1",
        "--features updated-fixture-native --test updated_check --test updated_fixture_original --locked -- --nocapture",
        "--features updated-fixture-native --lib --bin check-updated --bin updated-fixture --test updated_check --test updated_fixture_original --locked -- -D warnings",
        "rust/target/debug/check-updated --binary \"$UPDATED_CHECK_BINARY\" --launcher \"$UPDATED_CHECK_LAUNCHER\" --output",
    ]);
    let run = commands(job);
    required(
        &run,
        &[
            "--features openpilot-dev-tools/updated-daemon-native --bins --examples --locked",
            "rust/target/debug/check-updated-process",
            "rust/target/debug/check-updated-daemon",
            "python rust/tools/check_updated_agnos.py",
        ],
    );
    let daemon = job["steps"].as_array().unwrap().iter().find(|step| step["name"] == "Verify retired updater daemon and helper against independent original lifecycle observations").unwrap();
    trusted_step(daemon);
    required(daemon["run"].as_str().unwrap(), &[
        "for source in rust/tools/check_updated_daemon.py rust/tools/updated_fixtures.py; do",
        "test ! -e \"$source\"", "test ! -L \"$source\"",
        "UPDATED_DAEMON_BINARY=", "UPDATED_DAEMON_LAUNCHER=", "UPDATED_DAEMON_FIXTURE=", "UPDATED_DAEMON_CHECKER=", "UPDATED_DAEMON_SOURCE_COMMIT=",
        "UPDATED_DAEMON_NATIVE_OUTPUT=", "UPDATED_DAEMON_RETIREMENT_EVIDENCE=", "UPDATED_DAEMON_RAW_NATIVE_EVIDENCE=",
        "UPDATED_DAEMON_REQUIRE_ABSENT=1", "UPDATED_FIXTURE_REQUIRE_ABSENT=1",
        "--lib updated_daemon_check:: --locked -- --nocapture",
        "--test updated_daemon_check --test updated_daemon_retirement --test updated_daemon_cli --locked -- --nocapture",
        "--bin check-updated-daemon --bin updated-fixture --test updated_daemon_check --test updated_daemon_retirement --test updated_daemon_cli --locked -- -D warnings",
        "cmp rust/crates/dev-tools/tests/fixtures/updated-daemon-original-19e47ad9/original.stdout",
    ]);
    assert!(!run.contains("python rust/tools/check_updated_daemon.py"));
    assert!(!run.contains("python rust/tools/check_updated.py"));
    assert!(!run.contains("python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=rust/tools/tests rust/tools/tests/test_updated_comparison.py"));
}

#[test]
fn dashcam_progress_allocation_proof_keeps_functional_and_full_upload_routes() {
    let data = workflow();
    let job = &data["jobs"]["support-runtime"];
    let steps = job["steps"].as_array().unwrap();
    let step = steps
        .iter()
        .find(|step| {
            step["name"] == "Require dashcam progress equivalence and allocation accounting"
        })
        .unwrap();
    trusted_step(step);
    required(step["run"].as_str().unwrap(), &[
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-dashcam-upload --lib --locked -- --nocapture",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dashcam-upload --all-targets --locked -- -D warnings",
    ]);
    required(
        &commands(job),
        &[
            "python rust/tools/check_dashcam_jobs.py",
            "python rust/tools/check_dashcam_catalog.py",
            "python rust/tools/check_dashcam_metadata.py",
            "python rust/tools/check_dashcam_params.py",
            "python rust/tools/check_dashcam_runtime.py",
            "python rust/tools/check_dashcam_lifecycle.py",
            "--test git_capture --locked",
        ],
    );
}

#[test]
fn desire_controller_requires_retired_helper_and_private_absence_cli_command_gates() {
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    let steps = job["steps"].as_array().unwrap();
    let step = steps
        .iter()
        .find(|step| {
            step["name"] == "Verify native desire controller after shared Python helper retirement"
        })
        .unwrap();
    trusted_step(step);
    assert_eq!(step["working-directory"], "${{ env.CI296_SOURCE }}");
    required(step["run"].as_str().unwrap(), &[
        "test ! -e rust/tools/check_desire_reference.py", "test ! -L rust/tools/check_desire_reference.py",
        "export DESIRE_REFERENCE_EXPECT_ABSENT=1", "DESIRE_REFERENCE_CHECKER=", "DESIRE_REFERENCE_PROBE=",
        "DESIRE_REFERENCE_RETIREMENT_OUTPUT=", "DESIRE_REFERENCE_CLI_OUTPUT=",
        "--features desire-reference-native --bin check-desire-reference-native --locked",
        "--features desire-reference-native --lib desire_reference_check:: --locked -- --nocapture",
        "--test desire_reference_check --test desire_reference_retirement --test desire_reference_cli --locked -- --nocapture",
        "--lib --bin check-desire-reference-native --test desire_reference_check --test desire_reference_retirement --test desire_reference_cli --locked -- -D warnings",
        "rust/target/debug/check-desire-reference-native --binary",
    ]);
    let run = commands(job);
    assert!(!run.contains("python tools/check_desire_reference.py"));
    required(
        &run,
        &[
            "target/debug/check-command-reference",
            "target/debug/check-command-json-boundary",
        ],
    );
    let artifact = steps
        .iter()
        .find(|step| step["name"] == "Retain complete desire controller retirement evidence")
        .unwrap();
    assert_eq!(artifact["if"], TRUSTED_ALWAYS);
    assert_eq!(
        artifact["with"]["path"],
        "${{ env.CI_JOB_TEMP }}/desire-controller-native/\n"
    );
}

#[test]
fn cweb_redirect_allocation_keeps_full_http_daemon_and_address_comparisons_mandatory() {
    let data = workflow();
    let job = &data["jobs"]["ui-connectivity"];
    let steps = job["steps"].as_array().unwrap();
    let allocation = steps
        .iter()
        .find(|step| {
            step["name"] == "Require cweb redirect byte equivalence and allocation accounting"
        })
        .unwrap();
    trusted_step(allocation);
    required(allocation["run"].as_str().unwrap(), &[
        "cargo test --manifest-path rust/Cargo.toml -p openpilot-cweb-push --all-targets --locked -- --nocapture",
        "cargo clippy --manifest-path rust/Cargo.toml -p openpilot-cweb-push --all-targets --locked -- -D warnings",
        "tee \"$proof/package-tests.log\"",
        "tee \"$proof/clippy.log\"",
    ]);
    mandatory(
        job,
        &[
            "python rust/tools/check_cweb_policy.py",
            "python rust/tools/check_cweb_http.py",
            "python rust/tools/check_cweb_daemon.py",
            "python rust/tools/check_cweb_address.py",
        ],
    );
}

#[test]
fn native_kalman_generation_remains_required_with_fixed_originals_and_live_cpp_oracles() {
    job_check(
        "workspace",
        &[
            "--features kalman-model-generator-native --bin generate-pose-model --bin generate-car-model",
            "fresh_native_files_compile_and_match_original_functions_and_constant_bits -- --ignored --exact",
        ],
        &["kalman-model-generator-native"],
        false,
    );
    job_check(
        "estimation-runtime",
        &[
            "python rust/tools/build_locationd_oracle.py",
            "python rust/tools/build_paramsd_oracle.py",
            "for source in generate_pose_corpus.py generate_car_corpus.py; do",
            "test ! -e \"rust/tools/$source\" && test ! -L \"rust/tools/$source\"",
            "--features kalman-corpus-generator-native --bin generate-pose-corpus --bin generate-car-corpus",
            "cp rust/crates/locationd/tests/data/model.json",
            "cp rust/crates/paramsd/tests/data/model.json",
            "native_cli_matches_fresh_original_complete_json_bits_and_order -- --ignored --exact",
            "KALMAN_CORPUS_RETIRED_ROOT=",
            "sha256sum -c \"$proof/held-inputs.sha256\"",
            "sha256sum -c \"$proof/provider-dependencies.sha256\"",
        ],
        &["kalman-corpus-generator-native"],
        true,
    );
    let script = commands(&workflow()["jobs"]["estimation-runtime"]);
    assert!(!script.contains("python rust/tools/generate_pose_corpus.py"));
    assert!(!script.contains("python rust/tools/generate_car_corpus.py"));
}

#[test]
fn retired_route_controllers_require_ordered_native_build_checks_and_current_caller() {
    let data = workflow();
    let job = &data["jobs"]["logger-runtime"];
    absent(job, &["if", "continue-on-error"]);
    assert_eq!(
        job["defaults"]["run"]["working-directory"],
        "${{ github.workspace }}"
    );
    let required_commands = [
        "cargo +1.94.0 build -p openpilot-dev-tools --features route-upload-native --bin check-runtime-log-upload --locked",
        "cargo +1.94.0 test -p openpilot-dev-tools --features route-upload-native --lib route_upload_check:: --locked",
        "cargo +1.94.0 test -p openpilot-dev-tools --features route-upload-native --test route_upload_check --test route_upload_lifecycle --locked -- --test-threads=1",
        "target/debug/check-runtime-log-upload --root \"$GITHUB_WORKSPACE\" ",
    ];
    mandatory(job, &required_commands);
    let run = commands(job);
    let mut previous = None;
    for command in required_commands {
        assert_eq!(
            run.matches(command).count(),
            1,
            "required route command must occur exactly once: {command}"
        );
        let position = run.find(command).unwrap();
        assert!(
            previous.is_none_or(|before| before < position),
            "route build/check/caller order changed"
        );
        previous = Some(position);
    }
    required(
        &run,
        &["--binaries target/debug --output \"$RUNNER_TEMP/runtime-log-upload\""],
    );
    for name in [
        "check_manager_log_upload.py",
        "check_runtime_log_upload.py",
        "check_uploader.py",
        "check_uploader_logging.py",
        "check_uploader_daemon.py",
        "check_uploader_interrupted.py",
        "check_uploader_http_interrupted.py",
        "check_uploader_timeouts.py",
        "check_uploader_log_fault.py",
        "check_params_string_uploader.py",
    ] {
        assert!(
            !run.contains(name),
            "retired uploader controller remains a logger-runtime caller: {name}"
        );
        let path = root().join("rust/tools").join(name);
        assert!(
            matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "retired source, alias or broken symlink remains: {}",
            path.display()
        );
    }
}

const DELETER_CALLER_NAME: &str =
    "Compare log-space retention with the original loop and actual low-space filesystem";
const DELETER_CALLER_BODY: &str = r###"set -euo pipefail
"$GITHUB_WORKSPACE/rust/tools/check-disk-space" --path "$GITHUB_WORKSPACE" --growth-mib 1024
cargo build -p openpilot-deleter --bins --examples --locked
"$GITHUB_WORKSPACE/rust/tools/check-disk-space" --path "$GITHUB_WORKSPACE" --growth-mib 1024
cargo build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-dev-tools --no-default-features --features deleter-check-native --bin check-deleter --locked
cargo test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" --locked -p openpilot-dev-tools --no-default-features --features deleter-check-native --lib deleter_check:: -- --test-threads=1
"$GITHUB_WORKSPACE/rust/target/debug/check-deleter" --root "$GITHUB_WORKSPACE" --scope reference \
  --trace "$GITHUB_WORKSPACE/rust/target/debug/examples/deleter_trace" --binary "$GITHUB_WORKSPACE/rust/target/debug/openpilot-deleter" \
  --fixture "$GITHUB_WORKSPACE/rust/crates/dev-tools/tests/fixtures/deleter-original-group/source-observations.json" \
  --fixture-sha256 b4e652323459d74d54acbd3cf05eb8ae0a1f543033686d039928f63c530f6a64 --output "$RUNNER_TEMP/deleter-reference"
"$GITHUB_WORKSPACE/rust/target/debug/check-deleter" --root "$GITHUB_WORKSPACE" --scope daemon \
  --trace "$GITHUB_WORKSPACE/rust/target/debug/examples/deleter_trace" --binary "$GITHUB_WORKSPACE/rust/target/debug/openpilot-deleter" \
  --fixture "$GITHUB_WORKSPACE/rust/crates/dev-tools/tests/fixtures/deleter-original-group/source-observations.json" \
  --fixture-sha256 b4e652323459d74d54acbd3cf05eb8ae0a1f543033686d039928f63c530f6a64 --output "$RUNNER_TEMP/deleter-native"
"$GITHUB_WORKSPACE/rust/target/debug/check-deleter" --root "$GITHUB_WORKSPACE" --scope observer \
  --trace "$GITHUB_WORKSPACE/rust/target/debug/examples/deleter_trace" --binary "$GITHUB_WORKSPACE/rust/target/debug/openpilot-deleter" \
  --fixture "$GITHUB_WORKSPACE/rust/crates/dev-tools/tests/fixtures/deleter-original-group/source-observations.json" \
  --fixture-sha256 b4e652323459d74d54acbd3cf05eb8ae0a1f543033686d039928f63c530f6a64 --output "$RUNNER_TEMP/deleter-native/observer"
"###;
fn validate_deleter_callers(data: &Value) -> Result<(), String> {
    let job = &data["jobs"]["workspace"];
    if job.get("if").is_some() || job.get("continue-on-error").is_some() {
        return Err("optional workspace deleter gate".into());
    }
    let steps = job["steps"].as_array().ok_or("workspace steps absent")?;
    let selected = steps
        .iter()
        .filter(|step| step["name"] == DELETER_CALLER_NAME)
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return Err("deleter caller step missing or duplicated".into());
    }
    let step = selected[0];
    if step["if"] != TRUSTED_SOURCE || step.get("continue-on-error").is_some() {
        return Err("optional native deleter comparison".into());
    }
    let actual = step["run"].as_str().ok_or("native deleter script absent")?;
    if actual.lines().map(str::trim).collect::<Vec<_>>()
        != DELETER_CALLER_BODY
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
    {
        return Err("native deleter build, guard, fixture, hash, provider or scope changed".into());
    }
    let all = steps
        .iter()
        .filter_map(|s| s["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if all.contains("python tools/check_deleter_reference.py")
        || all.contains("python tools/check_deleter_daemon.py")
    {
        return Err("original deleter launcher remains".into());
    }
    Ok(())
}
#[test]
fn native_deleter_callers_bind_complete_original_reference_daemon_and_observer_scopes() {
    validate_deleter_callers(&workflow()).unwrap();
}
#[test]
fn native_deleter_callers_reject_missing_rebound_softened_or_duplicated_blocks() {
    let baseline = workflow();
    validate_deleter_callers(&baseline)
        .expect("current complete native callers before negative mutations");
    let index = baseline["jobs"]["workspace"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["name"] == DELETER_CALLER_NAME)
        .unwrap();
    for marker in DELETER_CALLER_BODY.lines().filter(|s| !s.is_empty()) {
        let mut changed = baseline.clone();
        let run = changed["jobs"]["workspace"]["steps"][index]["run"]
            .as_str()
            .unwrap()
            .replace(marker, "REMOVED");
        changed["jobs"]["workspace"]["steps"][index]["run"] = Value::String(run);
        assert!(
            validate_deleter_callers(&changed).is_err(),
            "missing or rebound {marker} admitted"
        );
    }
    for key in ["if", "continue-on-error"] {
        let mut changed = baseline.clone();
        changed["jobs"]["workspace"]["steps"][index][key] = Value::String("true".into());
        assert!(validate_deleter_callers(&changed).is_err());
    }
    for suffix in [" || true", "\nset +e", DELETER_CALLER_BODY] {
        let mut changed = baseline.clone();
        changed["jobs"]["workspace"]["steps"][index]["run"] =
            Value::String(format!("{DELETER_CALLER_BODY}{suffix}"));
        assert!(
            validate_deleter_callers(&changed).is_err(),
            "softened or same-step duplicate admitted"
        );
    }
    let mut duplicate = baseline.clone();
    let step = duplicate["jobs"]["workspace"]["steps"][index].clone();
    duplicate["jobs"]["workspace"]["steps"]
        .as_array_mut()
        .unwrap()
        .push(step);
    assert!(
        validate_deleter_callers(&duplicate).is_err(),
        "duplicate native step admitted"
    );
}

#[test]
fn paramsd_startup_maintained_caller_requires_native_two_root_exec_and_cleanup_audit() {
    let data = workflow();
    let steps = data["jobs"]["estimation-runtime"]["steps"]
        .as_array()
        .unwrap();
    let step = steps
        .iter()
        .find(|step| {
            step["name"] == "Require native paramsd startup caller and private source absence"
        })
        .unwrap();
    trusted_step(step);
    required(step["run"].as_str().unwrap(), &[
        "--features paramsd-startup-check-native --bin check-paramsd-startup-native --locked",
        "PARAMSD_STARTUP_CALLER_ABSENT_ROOT=", "git archive HEAD", "for lane in 0 1; do",
        "test ! -e \"$PARAMSD_STARTUP_CALLER_ABSENT_ROOT/rust/tools/check_paramsd_startup.py\"",
        "test ! -L \"$PARAMSD_STARTUP_CALLER_ABSENT_ROOT/rust/tools/check_paramsd_startup.py\"",
        "PATH=/nonexistent /usr/bin/timeout --signal=TERM --kill-after=5s 180s /usr/bin/strace",
        "--test paramsd_startup_caller --locked -- --ignored --exact native_startup_caller_preserves_seven_cases_and_private_source_absence --nocapture",
        "sha256sum -c \"$proof/held-elf.sha256\"", "sha256sum -c \"$proof/providers.sha256\"",
    ]);
    assert!(!commands(&data["jobs"]["estimation-runtime"])
        .contains("python rust/tools/check_paramsd_startup.py"));
}

#[test]
fn paramsd_atomic_gate_car_params_caller_is_native_and_preserves_shared_original_consumers() {
    let data = workflow();
    let run = commands(&data["jobs"]["estimation-runtime"]);
    required(&run, &[
        "--features paramsd-fixture-native --bin generate-paramsd-car-params --locked",
        "--test paramsd_car_params_cli --locked", "-o \"$proof/car-params.exec\"",
        "\"$generator\" --root \"$GITHUB_WORKSPACE\" --output \"$PARAMSD_NATIVE_GATE_CAR_PARAMS\"",
        "python rust/tools/check_paramsd.py", "python rust/tools/check_paramsd_daemon.py",
        "from paramsd_fixture import car_params, scenarios", "from check_paramsd_daemon import frames",
    ]);
    assert!(!run.contains("sys.stdout.buffer.write(bytes(car_params()))"));
    for helper in [
        "rust/tools/paramsd_fixture.py",
        "rust/tools/check_paramsd.py",
        "rust/tools/check_paramsd_daemon.py",
        "rust/tools/controlsd_fixture.py",
        "rust/tools/controlsd_scenarios.py",
    ] {
        assert!(
            root().join(helper).is_file(),
            "live shared consumer removed prematurely: {helper}"
        );
    }
}

#[test]
fn native_arm_proxy_resolution_rejects_unreviewed_family_or_gate_changes() {
    let main = workflow();
    for name in ["radar-arm", "carrot-navi-arm"] {
        assert!(!commands(&main["jobs"][name]).is_empty());
        let mut unknown = main["jobs"][name].clone();
        unknown["name"] = "unreviewed ARM family".into();
        assert!(std::panic::catch_unwind(|| commands(&unknown)).is_err());
    }
}
