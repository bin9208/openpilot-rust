//! Assertion-for-assertion native migration of test_rust_isolation.py.
use openpilot_dev_tools::ci_policy::{parse, run_shell, validate_rust_gate};
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
    absent(builders[0].1, &["if", "continue-on-error"]);
    absent(step, &["if", "continue-on-error"]);
}
#[test]
fn workspace_requires_pinned_llvm20_graph_and_primitive_regressions() {
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    let steps = job["steps"].as_array().unwrap();
    let find = |name: &str| {
        steps
            .iter()
            .enumerate()
            .find(|(_, step)| step["name"] == name)
            .unwrap_or_else(|| panic!("missing required {name}"))
    };
    let (stage_index, stage) = find("Stage independently captured LLVM20 provider");
    absent(stage, &["if", "continue-on-error"]);
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
        absent(step, &["if", "continue-on-error"]);
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
    required(&commands(job), &[
        "cargo test --workspace --locked --config profile.test.package.sha2.opt-level=3",
        "cargo clippy --workspace --all-targets --locked --config profile.test.package.sha2.opt-level=3",
    ]);
}
fn commands(job: &Value) -> String {
    job["steps"]
        .as_array()
        .unwrap()
        .iter()
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
fn mandatory(job: &Value, selectors: &[&str]) {
    absent(job, &["if"]);
    for step in job["steps"].as_array().unwrap() {
        if step.get("run").is_some()
            && selectors
                .iter()
                .any(|s| step["run"].as_str().unwrap_or("").contains(s))
        {
            absent(step, &["if", "continue-on-error"]);
        }
    }
}
fn artifacts(job: &Value) {
    let selected = job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| {
            s["uses"]
                .as_str()
                .unwrap_or("")
                .starts_with("actions/upload-artifact@")
        })
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["if"], "always()");
}
fn some_artifact(job: &Value) {
    assert!(job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["if"] == "always()"
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
    let job = &data["jobs"]["webcam-runtime"];
    assert_eq!(
        job["strategy"]["matrix"]["runner"],
        serde_json::json!(["ubuntu-24.04", "ubuntu-24.04-arm"])
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
    let config = workflow();
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
            "check_cweb_policy.py",
            "check_cweb_http.py",
            "check_cweb_daemon.py",
            "check_cweb_address.py",
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
            "check_paramsd_loop.py",
            "check_paramsd_daemon.py",
            "check_paramsd_startup.py",
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
    let data = workflow();
    for name in ["workspace", "arm64", "estimation-runtime"] {
        required(
            &commands(&data["jobs"][name]),
            &[
                &format!("@{commit}#subdirectory=eigen"),
                "LOCATIOND_EIGEN_INCLUDE=",
                "PARAMSD_EIGEN_INCLUDE=",
            ],
        );
    }
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
            "check_soundd.py",
            "check_soundd_daemon.py",
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
            "check_bridge.py",
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
        "${{ github.workspace }}/rust/target/debug/openpilot-process-child"
    );
    let run = ownership["run"].as_str().unwrap();
    assert!(
        run.find("rustup component add clippy --toolchain 1.94.0")
            .expect("minimal toolchain must install Clippy")
            < run.find("cargo clippy").unwrap()
    );
}
#[test]
fn platform_runtime_requires_original_source_and_live_native_processes() {
    job_check(
        "platform-runtime",
        &[
            "check_manager.py",
            "check_manager_logging.py",
            "generate_manager_cars.py",
            "examples/manager_native",
            "examples/manager_adapters",
            "check_modem.py",
            "check_modem_process_boundary.py",
            "check_hardwared.py",
            "check_hardwared_daemon.py",
        ],
        &["python rust/tools/check_"],
        false,
    );
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
}
#[test]
fn hardware_runtime_requires_source_and_native_boundaries() {
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
            "check_hardware_info.py",
            "rust/target/debug/check-hardware-control-policy rust/target/debug/examples/hardware_control_trace",
            "rust/target/debug/check-amplifier-policy rust/target/debug/examples/amplifier_trace",
            "rust/target/debug/check-amplifier-linux rust/target/debug/examples/amplifier_linux",
            "rust/target/debug/check-hardware-command --evidence \"$RUNNER_TEMP/hardware-runtime\"",
            "bootlog_raw_file_probe.rs",
        ],
        &["python rust/tools/check_", "check-hardware-command", "check-bootlog-snapshot", "--test bootlog_snapshot_check", "check-bootlog-artifacts", "--test bootlog_artifact_check"],
        false,
    );
}
#[test]
fn startup_prerequisites_run_actual_children_and_collectors() {
    job_check(
        "startup-runtime",
        &[
            "check_process_supervision.py",
            "check_managed_entry.py",
            "check_sdk_receiver.py",
            "check_registration.py",
            "check_registration_clock.py",
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
        &["python rust/tools/check_", "check-checkout-status", "--test checkout_status_check", "check-manager-catalog", "--test manager_catalog_check"],
        true,
    );
    let run = commands(&workflow()["jobs"]["startup-runtime"]);
    assert!(!run.contains("python rust/tools/check_checkout_status.py"));
    assert!(!run.contains("python rust/tools/check_manager_catalog.py"));
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
    absent(matches[0], &["if", "continue-on-error"]);
    required(
        matches[0]["run"].as_str().unwrap(),
        &[
            "zmq_send_boundary.c",
            "--wrap=zmq_msg_send",
            "check_interrupted_send.py",
            "check_uploader_interrupted.py",
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
        let job = &data["jobs"][name];
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
            "radarcan_prepare_assets.py",
            "--bin stage-torque-numerics",
            "test_radarcan_ci.py",
            "check_radarcan_ci.py --phase reference",
            "check_radarcan_ci.py --phase ipc",
            "-p openpilot-radarcan -p openpilot-can",
            "--all-targets --locked -- -D warnings",
        ],
    );
    let arm = commands(&data["jobs"]["radar-arm"]);
    assert_eq!(data["jobs"]["radar-arm"]["runs-on"], "ubuntu-24.04-arm");
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
    assert!(!arm.contains("qemu-aarch64"));
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
        absent(step, &["if", "continue-on-error"]);
    }
    required(
        &commands(job),
        &[
            "-p openpilot-selfdrived -p openpilot-messaging --locked",
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
            "requests==2.34.2",
            "urllib3==2.7.0",
            "PyJWT==2.14.0",
            "-p openpilot-navd --features native --bins --examples --locked",
            "check_navd_policy.py",
            "check_navd_engine.py",
            "check_navd_http.py",
            "--timeouts",
            "check_navd_config.py",
            "check_navd_destination.py",
            "check_navd_ipc.py",
        ],
    );
    assert!(job["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["if"] == "always()"
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
    for key in ["WORKSPACE", "MEMORY", "ENCODER", "XIAOGE", "UI_RUNTIME"] {
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
        serde_json::json!(["ubuntu-24.04", "ubuntu-24.04-arm"])
    );
    mandatory(job, &[""]);
    required(
        &commands(job),
        &[
            "webrtc_source_requirements.txt",
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
fn xiaoge_requires_both_architectures_and_instrumented_native_libraries() {
    let data = workflow();
    assert_eq!(
        data["jobs"]["xiaoge-runtime"]["strategy"]["matrix"]["runner"],
        serde_json::json!(["ubuntu-24.04", "ubuntu-24.04-arm"])
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
    assert_eq!(arm["working-directory"], "rust");
    required(
        arm["run"].as_str().unwrap(),
        &["--features static-ffmpeg,visionipc-ion"],
    );
    absent(arm, &["if", "continue-on-error"]);
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
    job_check("panda-runtime", &["check_pandad_ci.py","build_msgq_python.py","libusb1==3.4.0","-p openpilot-pandad -p openpilot-panda-usb -p openpilot-panda-spi -p openpilot-panda-spi-linux","--message-format=json","--build-messages"], &["check_pandad_ci.py"],false);
    some_artifact(&workflow()["jobs"]["panda-runtime"]);
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
            "-p openpilot-logmessaged -p openpilot-dev-tools --bins --examples",
            "rust/target/debug/check-registration-vendor",
            "check_registration.py",
        ],
    );
    mandatory(startup, &["check-registration-vendor"]);
    let text = read(".github/workflows/rust.yml");
    assert!(!text.contains("check_registration_vendor.py"));
    assert!(!text.contains("check_msgq_sanitizers.py"));
    assert!(!text.contains("check_control_tools_policy.py"));
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
        evidence.get("if").is_none(),
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
            "python tools/check_desire_reference.py --binary target/debug/examples/desire_probe",
            "cargo build -p openpilot-dev-tools --bin check-command-reference --bin check-command-json-boundary --locked",
            "target/debug/check-command-reference --binary target/debug/examples/command_probe",
            "target/debug/check-command-json-boundary --binary target/debug/examples/command_probe",
        ],
        &["--bin check-command-reference", "target/debug/check-command-reference --binary", "target/debug/check-command-json-boundary --binary"],
        false,
    );
    let data = workflow();
    let job = &data["jobs"]["workspace"];
    let run = commands(job);
    assert!(!run.contains("python tools/check_command_reference.py"));
    assert!(!run.contains("python tools/check_command_json_boundary.py"));
    assert!(job["steps"].as_array().unwrap().iter().any(|step| {
        step["uses"] == "actions/upload-artifact@v6"
            && step.get("if").is_none()
            && step["with"]["path"]
                .as_str()
                .is_some_and(|path| path.contains("model-commands/report.json"))
    }));
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
            "--test runtime_producer --locked",
            "--test retained_facade --locked",
            "--test error_log_lifetime --locked",
            "check_version_reference.py",
            "check_version_collector.py",
        ],
        &[
            "cargo test",
            "check_version_reference.py",
            "check_version_collector.py",
        ],
        false,
    );
    let data = workflow();
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
