//! Native counterparts of all thirteen test_native_disk_guard.py cases.
use openpilot_dev_tools::{
    ci_policy::parse,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn bootstrap() -> PathBuf {
    root().join("rust/tools/check-disk-space")
}
fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/bash\n{body}")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
struct Fixture {
    _temp: tempfile::TempDir,
    path: PathBuf,
    env: BTreeMap<String, String>,
    log: PathBuf,
}
impl Fixture {
    fn new(available: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_owned();
        let commands = path.join("commands");
        fs::create_dir(&commands).unwrap();
        let log = path.join("commands.log");
        executable(&commands.join("df"),&format!("set -eu\ncase \"$*\" in *'operation space'*) printf 'Avail\\n%s\\n' \"${{GUARD_OPERATION_FREE:-{available}}}\";; *) printf 'Avail\\n{available}\\n';; esac\n"));
        executable(
            &commands.join("rustup"),
            "set -eu\nprintf 'rustup %s\\n' \"$*\" >> \"$GUARD_COMMAND_LOG\"\n",
        );
        executable(
            &commands.join("cargo"),
            r#"set -eu
test -z "${RUSTFLAGS+x}${CARGO_BUILD_TARGET+x}"
printf 'cargo %s\n' "$*" >> "$GUARD_COMMAND_LOG"
test -z "${GUARD_BUILD_FAIL:-}" || exit 17
sleep 0.1
target=''
host=''
while [ "$#" -gt 0 ]; do
 case "$1" in
 --target-dir) target=$2; shift ;;
 --target) host=$2; shift ;;
 esac
 shift
done
mkdir -p "$target/$host/debug"
printf '#!/bin/bash\nprintf "native %%s\\n" "$*" >> "$GUARD_COMMAND_LOG"\n' > "$target/$host/debug/disk-space-guard"
chmod +x "$target/$host/debug/disk-space-guard"
"#,
        );
        let runner = path.join("runner");
        fs::create_dir(&runner).unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o700)).unwrap();
        let env = BTreeMap::from([
            (
                "PATH".into(),
                format!("{}:{}", commands.display(), std::env::var("PATH").unwrap()),
            ),
            ("RUNNER_TEMP".into(), runner.display().to_string()),
            ("GUARD_COMMAND_LOG".into(), log.display().to_string()),
            (
                "CARGO_TARGET_DIR".into(),
                path.join("unrelated-target").display().to_string(),
            ),
            ("RUSTFLAGS".into(), "-Zsanitizer=address".into()),
            (
                "CARGO_BUILD_TARGET".into(),
                "aarch64-unknown-linux-gnu".into(),
            ),
        ]);
        Self {
            _temp: temp,
            path,
            env,
            log,
        }
    }
    fn invoke(&self) -> openpilot_dev_tools::qa_process::Captured {
        let script = self
            .env
            .get("GUARD_BOOTSTRAP")
            .cloned()
            .unwrap_or_else(|| bootstrap().display().to_string());
        self.invoke_args(
            &script,
            &[
                "--path",
                self.env
                    .get("GUARD_OPERATION_PATH")
                    .map(String::as_str)
                    .unwrap_or("/path with spaces/new"),
                "--growth-mib",
                "128",
            ],
        )
    }
    fn invoke_args(
        &self,
        script: &str,
        args: &[&str],
    ) -> openpilot_dev_tools::qa_process::Captured {
        let mut spec = CommandSpec::new(
            std::iter::once("bash".into())
                .chain(std::iter::once(script.into()))
                .chain(args.iter().map(|v| (*v).into()))
                .collect(),
            Duration::from_secs(3),
        );
        spec.env = self.env.clone();
        if !self.env.contains_key("RUNNER_TEMP") {
            spec.argv = std::iter::once("env".into())
                .chain(["-u".into(), "RUNNER_TEMP".into()])
                .chain(spec.argv)
                .collect();
        }
        NativeRunner.run(&spec).unwrap()
    }
    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn cache(&self) -> PathBuf {
        PathBuf::from(&self.env["RUNNER_TEMP"]).join("openpilot-disk-guard")
    }
    fn binary(&self) -> PathBuf {
        fs::read_dir(self.cache())
            .unwrap()
            .filter_map(Result::ok)
            .map(|v| v.path().join("disk-space-guard"))
            .find(|p| p.is_file())
            .unwrap()
    }
}
fn success(output: &openpilot_dev_tools::qa_process::Captured) {
    assert_eq!(
        output.code,
        0,
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn capacity_failure_precedes_rustup_and_cargo() {
    let f = Fixture::new(&(25 * 1024u64.pow(3) + 1024u64.pow(3) - 1).to_string());
    let r = f.invoke();
    assert_ne!(r.code, 0);
    assert!(String::from_utf8_lossy(&r.stderr).contains("recover at least 35 GiB"));
    assert!(!f.log.exists());
}
#[test]
fn cold_bootstrap_builds_only_host_guard_and_survives_target_cleanup() {
    let mut f = Fixture::new("999999999999");
    success(&f.invoke());
    let first = f.lines();
    assert!(first[0].starts_with("rustup toolchain install 1.94.0"));
    for needle in ["cargo +1.94.0 build", "-p openpilot-disk-space", "--locked"] {
        assert!(first[1].contains(needle));
    }
    assert_eq!(
        first[2],
        "native --path /path with spaces/new --growth-mib 128"
    );
    let _ = fs::remove_dir_all(&f.env["CARGO_TARGET_DIR"]);
    f.env
        .get_mut("CARGO_TARGET_DIR")
        .unwrap()
        .push_str("-changed");
    success(&f.invoke());
    let mut expected = first.clone();
    expected.push(first[2].clone());
    assert_eq!(f.lines(), expected);
}
fn hash(lines: &[String]) -> String {
    format!("{:x}", Sha256::digest(lines.join("\n").as_bytes()))
}
#[test]
fn workflow_guards_use_absolute_native_bootstrap_and_keep_all_growths() {
    let text = fs::read_to_string(root().join(".github/workflows/rust.yml")).unwrap();
    assert!(!text.contains("card_ci_space.py"));
    let marker = "\"$GITHUB_WORKSPACE/rust/tools/check-disk-space\"";
    assert_eq!(text.matches(marker).count(), 480);
    let workflow = parse(&text).unwrap();
    let jobs = workflow["jobs"].as_object().unwrap();
    let scripts = |job: &serde_json::Value| {
        job["steps"]
            .as_array()
            .map(|steps| {
                steps
                    .iter()
                    .map(|s| s["run"].as_str().unwrap_or("").to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        jobs.values()
            .filter(|job| scripts(job).iter().any(|s| s.contains("check-disk-space")))
            .count(),
        35
    );
    assert_eq!(
        jobs.values()
            .filter(|job| scripts(job).iter().any(|s| {
                let lines = s.lines().collect::<Vec<_>>();
                lines.iter().enumerate().any(|(i, line)| {
                    line.contains("check-disk-space")
                        && (i + 1 == lines.len()
                            || !lines[i + 1].contains("--bin stage-startup-ui-raylib"))
                })
            }))
            .count(),
        35
    );
    let lines = text.lines().collect::<Vec<_>>();
    let llvm20_start = lines
        .iter()
        .position(|line| *line == "  model-llvm20:")
        .unwrap();
    let llvm20_end = lines
        .iter()
        .enumerate()
        .skip(llvm20_start + 1)
        .find(|(_, line)| line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':'))
        .map_or(lines.len(), |(index, _)| index);
    let mut arguments = vec![];
    let mut original_msgq = vec![];
    let mut services_catalog = vec![];
    let mut manager_catalog = vec![];
    let mut qcom_assistance = vec![];
    let mut feedback = vec![];
    let mut bluetooth_input = vec![];
    let mut stats_producer = vec![];
    let mut proclog_reference = vec![];
    let mut aarch64_check = vec![];
    let mut stats_daemon = vec![];
    let mut soundd_policy = vec![];
    let mut updated_ownership = vec![];
    let mut metadata_transport = vec![];
    let mut metadata_capture = vec![];
    let mut helper_transport = vec![];
    let mut lagd_tools = vec![];
    let mut lagd_masked_allocation = vec![];
    let mut raylib = vec![];
    let mut logging_builder = vec![];
    let mut native_architecture = vec![];
    let mut encoder_builder = vec![];
    let mut encoder_checks = vec![];
    let mut model_source = vec![];
    let mut compiler_node280 = vec![];
    let mut translation_checker = vec![];
    let mut diagnostic_checkers = vec![];
    let mut artifact_list = vec![];
    let mut miri_hashes = vec![];
    let mut message_state = vec![];
    let mut egl_reference = vec![];
    let mut command_checkers = vec![];
    let mut params_reference = vec![];
    let mut proc_stat_probe = vec![];
    let mut polygon_checks = vec![];
    let mut image_codecs = vec![];
    let mut llvm_provider = vec![];
    let mut llvm20_regression = vec![];
    let mut timed_tools = vec![];
    let mut checkout_tools = vec![];
    let mut ratekeeper = vec![];
    let mut cutin = vec![];
    let mut selfdrive_state = vec![];
    let mut alert_family = vec![];
    let mut ui_diagnostics = vec![];
    let mut selfdrive_params = vec![];
    let mut forms = vec![];
    let mut camera_generators = vec![];
    let mut ui_connectivity_build = vec![];
    let mut registration_utf7 = vec![];
    let mut application_lifecycle = vec![];
    let mut journal_bootlog_checks = vec![];
    let mut dbc_native = vec![];
    let mut version_controllers = vec![];
    let mut sdk_receiver = vec![];
    let mut tombstone_controllers = vec![];
    let mut managed_controllers = vec![];
    let mut managed_runtime = vec![];
    let mut registration_clock = vec![];
    let mut registration_tools = vec![];
    let mut navd_config_tools = vec![];
    let mut stats_runtime_receipts = vec![];
    let mut connectivity_ownership = vec![];
    let mut wifi_snapshot = vec![];
    let mut bridge_controller = vec![];
    let mut soundd_daemon_controller = vec![];
    let mut soundd_finalization = vec![];
    let mut soundd_retirement = vec![];
    let mut paramsd_controller = vec![];
    let mut paramsd_fixture = vec![];
    let mut paramsd_loop_controller = vec![];
    let mut bluetooth_journal_absence = vec![];
    let mut bluetooth_files_controller = vec![];
    let mut bluetooth_config_controller = vec![];
    let mut bluetooth_config_absence = vec![];
    let mut bluetooth_bluez_policy_controller = vec![];
    let mut bluetooth_enumerate_controller = vec![];
    let mut bluetooth_gesture_cases = vec![];
    let mut bluetooth_gesture_absence = vec![];
    let mut bluetooth_sudo_fixture = vec![];
    let mut messaging_update_allocation = vec![];
    let mut eigen_staging = vec![];
    let mut paramsd_gate = vec![];
    let mut logmessaged_startup_retirement = vec![];
    let mut lpa_cookie_header = vec![];
    let mut bluetooth_journal_controller = vec![];
    let mut calibration_float_controller = vec![];
    let mut calibration_estimator_retirement = vec![];
    let mut updater_controller_retirement = vec![];
    let mut dashcam_progress_allocation = vec![];
    let mut pandad_padding_controller = vec![];
    let mut pandad_padding_finalization = vec![];
    let mut pandad_device_controller = vec![];
    let mut modem_helper_lifetime = vec![];
    let mut jetlink_owner = vec![];
    let mut recording_partial_write = vec![];
    let mut launcher_handoff = vec![];
    for (i, line) in lines.iter().enumerate() {
        if let Some((_, tail)) = line.split_once(marker) {
            let step = lines[..i]
                .iter()
                .rev()
                .find(|line| line.starts_with("      - name:"));
            if step
                == Some(
                    &"      - name: Require dashcam progress equivalence and allocation accounting",
                )
            {
                dashcam_progress_allocation.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Verify retired updater controller against all original lifecycle cases") {
                updater_controller_retirement.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Verify retired calibration estimator against full original observations",
                )
            {
                calibration_estimator_retirement.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require native Panda padding controller with sealed original observations") {
                pandad_padding_controller.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Require LPA cookie header byte parity and allocation reduction",
                )
            {
                lpa_cookie_header.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require same-ELF logmessaged startup with historical checker absent") {
                logmessaged_startup_retirement.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Compare native Bluetooth config controller with unchanged original",
            ) {
                bluetooth_config_controller.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require Bluetooth gesture generator source absence and native callers") {
                bluetooth_gesture_absence.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require Bluetooth config same-ELF source absence proof")
            {
                bluetooth_config_absence.push(tail.to_owned());
                continue;
            }
            if step
                == Some(&"      - name: Require Bluetooth journal same-ELF source absence proof")
            {
                bluetooth_journal_absence.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Require sound daemon same-ELF present and absent execution proof",
            ) {
                soundd_retirement.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Compare native paramsd loop checker against complete original observations") {
                paramsd_loop_controller.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Compare native paramsd fixture generator against all original wire inputs") {
                paramsd_fixture.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Compare full native paramsd controller against original source packets") {
                paramsd_controller.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require audio finalization baseline rejection") {
                soundd_finalization.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Check native paramsd atomic gate foundation alongside original cases")
                || step == Some(&"      - name: Require native paramsd scoped child-FD handoff")
            {
                paramsd_gate.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Require native Eigen header staging against sealed original install",
                )
            {
                eigen_staging.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Compare native Bluetooth sudo fixture with unchanged original",
                )
            {
                bluetooth_sudo_fixture.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Measure actual messaging update allocation against unchanged source") {
                messaging_update_allocation.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Compare native Bluetooth gesture corpus with unchanged original",
            ) {
                bluetooth_gesture_cases.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Compare native Bluetooth enumeration controller against sealed original observations") {
                bluetooth_enumerate_controller.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Compare native BlueZ policy controller against sealed original observations",
                )
            {
                bluetooth_bluez_policy_controller.push(tail.to_owned());
                continue;
            }
            if step
                == Some(
                    &"      - name: Measure masked lagd input allocation against unchanged source",
                )
            {
                lagd_masked_allocation.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Require cold native node280 against independent original captures",
            ) {
                compiler_node280.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require old Panda padding finalization drift failure") {
                pandad_padding_finalization.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Require bounded launcher cleanup after failed lock acknowledgement",
            ) {
                launcher_handoff.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require Jetlink owner baseline failures and bounded fixed lifecycle") {
                jetlink_owner.push(tail.to_owned());
                continue;
            }
            if step == Some(
                &"      - name: Reproduce incomplete buffered frame writes on old native adapter",
            ) {
                recording_partial_write.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features pandad-device-native") {
                pandad_device_controller.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("git fetch --no-tags --depth=1 origin 1364e48c")
                    || lines[i + 1].contains("--manifest-path \"$baseline/rust/Cargo.toml\"")
                    || lines[i + 1].contains("helper-lifetime-tests")
                    || lines[i + 1].contains(
                        "cargo build --manifest-path rust/Cargo.toml -p openpilot-process-supervision --bin openpilot-process-child --locked",
                    ))
            {
                modem_helper_lifetime.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features calibration-float-native")
                    || lines[i + 1].contains("rust/target/debug/check-calibration-float --binary"))
            {
                calibration_float_controller.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features bluetooth-journal-native")
                    || lines[i + 1].contains("rust/target/debug/check-bluetooth-journal --binary"))
            {
                bluetooth_journal_controller.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features bluetooth-files-native")
                    || lines[i + 1].contains("rust/target/debug/check-bluetooth-files --binary")
                    || lines[i + 1].contains(
                        "proof=\"$RUNNER_TEMP/ui-connectivity/bluetooth-files-retirement\"",
                    ))
            {
                bluetooth_files_controller.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features soundd-daemon-native")
                    || lines[i + 1].contains(
                        "rust/tools/fixtures/soundd/portaudio.c -o \"$SOUNDD_PORTAUDIO_LIBRARY\"",
                    )
                    || lines[i + 1].contains("\"$PWD/rust/target/debug/check-soundd-daemon\""))
            {
                soundd_daemon_controller.push(tail.to_owned());
                continue;
            }
            if step == Some(&"      - name: Require native bridge controller against sealed original observations") {
                bridge_controller.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("-p openpilot-wifi --lib --locked")
                    || lines[i + 1].contains(
                        "rustc --edition=2021 rust/tools/wifi_snapshot_allocation_probe.rs",
                    ))
            {
                wifi_snapshot.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1].contains("-p openpilot-bluetooth -p openpilot-wifi --all-targets")
            {
                connectivity_ownership.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("rust/target/debug/check-stats-runtime ")
                    || lines[i + 1].contains("rust/target/debug/check-stats-flood "))
            {
                stats_runtime_receipts.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features registration-tools-native")
            {
                registration_tools.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features navd-config-tools-native") {
                navd_config_tools.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features registration-clock-native")
            {
                registration_clock.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features managed-runtime-native")
                    || lines[i + 1]
                        .contains("--features openpilot-dev-tools/managed-runtime-native"))
            {
                managed_runtime.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features managed-entry-native")
                    || lines[i + 1].contains("--features qa-process-title"))
            {
                managed_controllers.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1].contains("--features tombstone-live-native,sdk-transport-native")
            {
                tombstone_controllers.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features sdk-receiver-native") {
                sdk_receiver.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features bluetooth-input-native") {
                bluetooth_input.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features stats-producer-native") {
                stats_producer.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features stats-daemon-native") {
                stats_daemon.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features soundd-policy-native") {
                soundd_policy.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features aarch64-check-native")
                    || lines[i + 1].contains("--features openpilot-dev-tools/aarch64-check-native"))
            {
                aarch64_check.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features proclog-reference-native") {
                proclog_reference.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features version-tools-native") {
                version_controllers.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features dbc-generator-native") {
                dbc_native.push(tail.to_owned());
                continue;
            }
            let step = lines[..i]
                .iter()
                .rev()
                .find(|line| line.starts_with("      - name:"));
            if step.is_some_and(|line| matches!(*line,
                "      - name: Build and test native lagd QA tools"
                    | "      - name: Compare delay numerical policy, full source loop, cache and native IPC"))
            {
                lagd_tools.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && [
                    "--test retained_facade",
                    "--test assistance_log_lifetime",
                    "--test params_log_lifetime",
                ]
                .iter()
                .any(|target| lines[i + 1].contains(target))
            {
                helper_transport.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--test git_capture") {
                metadata_capture.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--test runtime_producer")
                    || lines[i + 1].contains("--test error_log_lifetime"))
            {
                metadata_transport.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features command-owner-tests")
                    || lines[i + 1].contains("--lib updated_process_check::ownership_tests")
                    || lines[i + 1]
                        .contains("--lib --test updated_process_check --bin check-updated-process"))
            {
                updated_ownership.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features feedback-runtime-native") {
                feedback.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1].contains("--features qcom-assistance-tools-native")
            {
                qcom_assistance.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1].contains("--features manager-catalog-tools-native")
            {
                manager_catalog.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features checkout-tools-native") {
                checkout_tools.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features timed-live-native") {
                timed_tools.push(tail.to_owned());
                continue;
            }
            if (llvm20_start..llvm20_end).contains(&i) {
                llvm20_regression.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--features services-catalog-native") {
                services_catalog.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1].contains(
                    "python tools/build_msgq_python.py --output \"$RUNNER_TEMP/rust-msgq-python\"",
                )
            {
                original_msgq.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features journal-runtime-native --bin check-journald")
                    || lines[i + 1].contains(
                        "-p openpilot-dev-tools -p openpilot-bootlog -p openpilot-hardware-info",
                    ))
            {
                journal_bootlog_checks.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1]
                    .contains("rust/target/debug/build-ui-application-reference --output")
                    || lines[i + 1].contains("--features ui-application-lifecycle-native --test")
                    || lines[i + 1].contains("--features ui-application-lifecycle-native --lib"))
            {
                application_lifecycle.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && lines[i + 1].contains("--test encoder_codecs_check") {
                encoder_checks.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1]
                    .contains("-p openpilot-ui-framework -p openpilot-startup-ui -p openpilot-wifi")
            {
                ui_connectivity_build.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--test camera_source_generate")
                    || lines[i + 1].contains("--bin generate-camerad-requests-reference"))
            {
                camera_generators.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && lines[i + 1]
                    .contains("--test registration_utf7_check --test registration_utf7_ci")
            {
                registration_utf7.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("mkdir -p \"$RUNNER_TEMP/ui-forms-raw\"")
                    || lines[i + 1].contains("--test ui_forms_check --test ui_forms_reference"))
            {
                forms.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len()
                && (lines[i + 1].contains("--features selfdrive-params-native")
                    || lines[i + 1]
                        .contains("rust/target/debug/build-selfdrive-params-reference --output")
                    || lines[i + 1].contains("rust/target/debug/check-selfdrive-params --native"))
            {
                selfdrive_params.push(tail.to_owned());
                continue;
            }
            if i + 1 < lines.len() && (lines[i + 1].contains("--bin check-ratekeeper") || lines[i + 1].contains("rust/target/debug/check-ratekeeper --binary")) {
                ratekeeper.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("rust/target/debug/check-cutin-alert --binary") {
                cutin.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("rust/target/debug/check-selfdrive-state --binary") {
                selfdrive_state.push(tail.to_owned());
            } else if i + 1 < lines.len() && ["rust/target/debug/check-alert-manager --binary", "rust/target/debug/check-selfdrive-events --binary", "rust/target/debug/check-alert-callbacks --binary", "rust/target/debug/check-selfdrive-helpers --native"].iter().any(|command| lines[i+1].contains(command)) {
                alert_family.push(tail.to_owned());
            } else if i + 1 < lines.len() && (lines[i + 1].contains("--features image-array-native --test ui_diagnostic_reference") || lines[i + 1].contains("rust/target/debug/build-ui-diagnostic-reference --output")) {
                ui_diagnostics.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin stage-startup-ui-raylib") {
                raylib.push(tail.to_owned());
            } else if i + 1 < lines.len()
                && lines[i + 1].contains("--bin build-native-logging-reference")
            {
                logging_builder.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--test native_build_closure") {
                native_architecture.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin build-encoder-source") {
                encoder_builder.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("git lfs pull --include=openpilot/selfdrive/modeld/models/driving_supercombo.onnx") {
                model_source.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-ui-translations") {
                translation_checker.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-runtime-diagnostics") {
                diagnostic_checkers.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin list-cargo-tests") {
                artifact_list.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin hash-miri-core") {
                miri_hashes.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-message-state") {
                message_state.push(tail.to_owned());
            } else if i + 1 < lines.len()
                && lines[i + 1].contains("--features egl-reference --test ui_egl_reference")
            {
                egl_reference.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-command-reference") {
                command_checkers.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--bin check-params-reference") {
                params_reference.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("rustc --edition=2021 tools/proc_stat_allocation_probe.rs") {
                proc_stat_probe.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--features polygon-reference --test image_array") {
                polygon_checks.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("-p openpilot-image-codecs --features native-sdk") {
                image_codecs.push(tail.to_owned());
            } else if i + 1 < lines.len() && lines[i + 1].contains("--output \"$RUNNER_TEMP/llvm18.deb\"") {
                llvm_provider.push(tail.to_owned());
            } else {
                arguments.push(tail.to_owned());
            }
        }
    }
    // The removed camera pip install and Cython build had 512/1024 MiB
    // guards. Native camera operations retain every required reserve.
    assert_eq!(arguments.len(), 177);
    assert_eq!(
        dashcam_progress_allocation,
        [512, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        updater_controller_retirement,
        [512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        calibration_estimator_retirement,
        [512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        soundd_policy,
        [512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        stats_daemon,
        [512, 512, 512, 512, 512, 512, 1024, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        aarch64_check,
        [1024, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        proclog_reference,
        [1024, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        stats_producer,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 3]
    );
    assert_eq!(
        bluetooth_input,
        [1024, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        registration_tools,
        [1024, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        stats_runtime_receipts,
        [128, 128].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        navd_config_tools,
        [1024, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        connectivity_ownership,
        [512, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        wifi_snapshot,
        [512, 64].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        registration_clock,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 2]
    );
    assert_eq!(
        managed_runtime,
        [
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
        ]
    );
    assert_eq!(
        managed_controllers,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 6]
    );
    assert_eq!(
        tombstone_controllers,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 5]
    );
    assert_eq!(
        sdk_receiver,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 2]
    );
    assert_eq!(
        version_controllers,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 2]
    );
    assert_eq!(
        dbc_native,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
        ]
    );
    assert_eq!(
        checkout_tools,
        [
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 256",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 256",
        ]
    );
    assert_eq!(
        timed_tools,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 2048",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
        ]
    );
    assert_eq!(
        compiler_node280,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        llvm20_regression,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 2048",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 128",
            " --path \"$RUNNER_TEMP\" --growth-mib 256",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
        ]
    );
    assert_eq!(
        journal_bootlog_checks,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"; 2]
    );
    assert_eq!(
        application_lifecycle,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 16",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
        ]
    );
    assert_eq!(
        encoder_checks,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"; 2]
    );
    assert_eq!(
        ui_connectivity_build,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 4096"]
    );
    assert_eq!(
        camera_generators,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
        ]
    );
    assert_eq!(
        registration_utf7,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        forms,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 16",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512"
        ]
    );
    assert_eq!(
        selfdrive_params,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 2048",
        ]
    );
    assert_eq!(
        ui_diagnostics,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 512",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 16"
        ]
    );
    assert_eq!(
        alert_family,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"; 4]
    );
    assert_eq!(
        selfdrive_state,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        cutin,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        ratekeeper,
        vec![
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"
        ]
    );
    assert_eq!(
        llvm_provider,
        vec![" --path \"$RUNNER_TEMP\" --growth-mib 256"]
    );
    assert_eq!(
        polygon_checks,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024"]
    );
    assert_eq!(
        image_codecs,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        proc_stat_probe,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 64"]
    );
    assert_eq!(
        miri_hashes,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        params_reference,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        command_checkers,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        egl_reference,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        message_state,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        artifact_list,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        diagnostic_checkers,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        translation_checker,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        model_source,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 128"]
    );
    assert_eq!(
        encoder_builder,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        native_architecture,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 1024"]
    );
    assert_eq!(
        lpa_cookie_header,
        [512, 128].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        logmessaged_startup_retirement,
        [512, 512, 512, 512, 128, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_files_controller,
        [512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_sudo_fixture,
        [512, 512, 128, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        messaging_update_allocation,
        [4096, 1024, 128, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        eigen_staging,
        [256, 512, 64, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        paramsd_gate,
        [512, 512, 128, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_gesture_absence,
        [128, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_gesture_cases,
        [512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_enumerate_controller,
        [512, 512, 512, 128, 128, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_bluez_policy_controller,
        [512, 512, 512, 512, 128, 128, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_config_absence,
        [128, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_config_controller,
        [512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        soundd_daemon_controller,
        [512, 64, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_journal_absence,
        [128, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        soundd_retirement,
        [128, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        paramsd_loop_controller,
        [512, 128, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        paramsd_fixture,
        [128, 512, 128, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        paramsd_controller,
        [512, 512, 128, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        soundd_finalization,
        [4096, 512].map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bridge_controller,
        [1024, 512, 512, 512, 128, 128, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        bluetooth_journal_controller,
        [512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        calibration_float_controller,
        [512, 512, 512, 512, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        pandad_padding_finalization,
        [1024, 8192, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        pandad_padding_controller,
        [512, 128, 512, 1024, 128, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        modem_helper_lifetime,
        [2048, 1024, 512, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        pandad_device_controller,
        [512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        jetlink_owner,
        [1024, 2048, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        recording_partial_write,
        [1024, 8192, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        launcher_handoff,
        [1024, 1024, 512, 512, 512]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    // Retain the 177 still-used original guards/hash below, plus admission for
    // native builds. A new build must not replace an inherited reserve.
    assert_eq!(
        logging_builder,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"]
    );
    assert_eq!(
        raylib,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 2048"; 5]
    );
    let native = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("--bin stage-startup-ui-raylib"))
        .collect::<Vec<_>>();
    assert_eq!(native.len(), 5);
    let native_args = native
        .iter()
        .map(|(_, line)| {
            line.split_once("--bin stage-startup-ui-raylib -- ")
                .unwrap()
                .1
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hash(&native_args),
        "2efc90425a3b4e1cecaabbc410c6d13649b89ad4fbc3a9d722b33b510694b396"
    );
    for (i, line) in native {
        assert!(line.contains("cargo +1.94.0 run"));
        assert!(lines[i - 1].contains(marker));
    }
    assert!(!text.contains("stage_startup_ui_raylib.py"));
    assert_eq!(
        original_msgq,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        services_catalog,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 3]
    );
    assert_eq!(
        manager_catalog,
        [
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 1024",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 256",
            " --path \"$GITHUB_WORKSPACE\" --growth-mib 256",
        ]
    );
    assert_eq!(
        metadata_capture,
        vec![" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"]
    );
    assert_eq!(
        helper_transport,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 3]
    );
    assert_eq!(
        lagd_tools,
        [256, 1024, 512, 512, 128, 128, 128, 128, 128, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        lagd_masked_allocation,
        [8192, 1024, 128]
            .map(|growth| format!(" --path \"$GITHUB_WORKSPACE\" --growth-mib {growth}"))
    );
    assert_eq!(
        metadata_transport,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 2]
    );
    assert_eq!(
        updated_ownership,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 4]
    );
    assert_eq!(
        qcom_assistance,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 3]
    );
    assert_eq!(
        feedback,
        [" --path \"$GITHUB_WORKSPACE\" --growth-mib 512"; 3]
    );
    assert_eq!(
        hash(&arguments),
        "b0f4e20cbb89c4118d25cf1b4664642cc5222969ea807616e5f386b5679e45ae"
    );
    let template = fs::read_to_string(root().join("rust/crates/carrot-man/ci-jobs.yml")).unwrap();
    assert!(!template.contains("card_ci_space.py"));
    assert_eq!(template.matches(marker).count(), 9);
    let args = template
        .lines()
        .filter_map(|l| l.split_once(marker).map(|(_, s)| s.to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(
        hash(&args),
        "471b43e7b994adca394eb5a342a5cdbaca211fec1b306dc219cc9c0f767a03b7"
    );
}
fn find_named(path: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = vec![];
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == name {
            found.push(path.clone());
        }
        if path.is_dir() {
            found.extend(find_named(&path, name));
        }
    }
    found
}
#[test]
fn failed_build_never_publishes_guard() {
    let mut f = Fixture::new("999999999999");
    f.env.insert("GUARD_BUILD_FAIL".into(), "yes".into());
    assert_ne!(f.invoke().code, 0);
    assert!(find_named(Path::new(&f.env["RUNNER_TEMP"]), "disk-space-guard").is_empty());
    f.env.remove("GUARD_BUILD_FAIL");
    success(&f.invoke());
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        2
    );
}
#[test]
fn parallel_cold_calls_publish_once() {
    let f = Fixture::new("999999999999");
    std::thread::scope(|s| {
        let a = s.spawn(|| f.invoke());
        let b = s.spawn(|| f.invoke());
        success(&a.join().unwrap());
        success(&b.join().unwrap());
    });
    let lines = f.lines();
    assert_eq!(lines.iter().filter(|l| l.starts_with("cargo ")).count(), 1);
    assert_eq!(lines.iter().filter(|l| l.starts_with("native ")).count(), 2);
}
#[test]
fn requested_operation_filesystem_is_checked_before_toolchain_work() {
    let mut f = Fixture::new("999999999999");
    let operation = f.path.join("operation space");
    fs::create_dir(&operation).unwrap();
    f.env.insert(
        "GUARD_OPERATION_PATH".into(),
        operation.join("new output").display().to_string(),
    );
    f.env.insert(
        "GUARD_OPERATION_FREE".into(),
        (25 * 1024u64.pow(3) + 128 * 1024u64.pow(2) - 1).to_string(),
    );
    let result = f.invoke();
    assert_ne!(result.code, 0);
    assert!(String::from_utf8_lossy(&result.stderr).contains("recover at least 35 GiB"));
    assert!(!f.log.exists());
}
#[test]
fn arm_runner_bootstraps_native_host_even_with_cross_environment() {
    let mut f = Fixture::new("999999999999");
    executable(&f.path.join("commands/uname"), "printf 'aarch64\\n'\n");
    f.env.insert(
        "CARGO_BUILD_TARGET".into(),
        "x86_64-unknown-linux-gnu".into(),
    );
    success(&f.invoke());
    assert!(fs::read_to_string(&f.log)
        .unwrap()
        .contains("--target aarch64-unknown-linux-gnu"));
}
#[test]
fn bad_df_output_fails_closed_before_install() {
    let f = Fixture::new("not-a-number");
    assert_ne!(f.invoke().code, 0);
    assert!(!f.log.exists());
}
#[test]
fn invalid_cli_never_installs_or_compiles() {
    let f = Fixture::new("999999999999");
    for args in [
        &["--path", ".", "--path", ".", "--growth-mib", "0"][..],
        &["--path", ".", "--growth-mib", "0", "--growth-mib", "0"],
        &["--growth-mib", "0"],
        &["--path", ".", "--growth-mib", "0", "--unknown"],
    ] {
        let result = f.invoke_args(bootstrap().to_str().unwrap(), args);
        assert_ne!(result.code, 0, "{args:?}");
        assert!(result.stdout.is_empty());
        assert!(!f.log.exists());
    }
}
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).unwrap();
        }
    }
}
#[test]
fn changed_source_invalidates_cached_executable() {
    let mut f = Fixture::new("999999999999");
    let repo = f.path.join("repository");
    fs::create_dir_all(repo.join("rust/tools")).unwrap();
    fs::copy(bootstrap(), repo.join("rust/tools/check-disk-space")).unwrap();
    for name in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(root().join("rust").join(name), repo.join("rust").join(name)).unwrap();
    }
    copy_tree(
        &root().join("rust/crates/disk-space"),
        &repo.join("rust/crates/disk-space"),
    );
    f.env.insert(
        "GUARD_BOOTSTRAP".into(),
        repo.join("rust/tools/check-disk-space")
            .display()
            .to_string(),
    );
    success(&f.invoke());
    for name in [
        "rust/crates/disk-space/src/lib.rs",
        "rust/Cargo.lock",
        "rust/Cargo.toml",
    ] {
        let path = repo.join(name);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push('\n');
        fs::write(path, text).unwrap();
        success(&f.invoke());
    }
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        4
    );
}
#[test]
fn interrupted_publication_exposes_no_partial_executable() {
    let f = Fixture::new("999999999999");
    let copier = f.path.join("commands/cp");
    executable(&copier,"for argument do destination=$argument; done\nprintf 'partial executable' > \"$destination\"\nexit 17\n");
    assert_ne!(f.invoke().code, 0);
    for entry in fs::read_dir(f.cache()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert!(!path.join("disk-space-guard").exists());
            assert!(!fs::read_dir(path).unwrap().any(|v| v
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".publish.")));
        }
    }
    assert!(!f.lines().iter().any(|l| l.starts_with("native ")));
    fs::remove_file(copier).unwrap();
    success(&f.invoke());
}
#[test]
fn cache_rejects_unsafe_entries_without_executing_or_building() {
    for kind in [
        "base-mode",
        "base-symlink",
        "lock-fifo",
        "lock-symlink",
        "binary-mode",
        "binary-symlink",
        "key-mode",
        "lock-mode",
        "base-owner",
    ] {
        let f = Fixture::new("999999999999");
        let base = f.cache();
        if kind.starts_with("binary") || kind == "key-mode" {
            success(&f.invoke());
            let binary = f.binary();
            if kind == "key-mode" {
                fs::set_permissions(binary.parent().unwrap(), fs::Permissions::from_mode(0o777))
                    .unwrap();
            } else if kind == "binary-mode" {
                fs::set_permissions(binary, fs::Permissions::from_mode(0o777)).unwrap();
            } else {
                let target = f.path.join("sentinel");
                fs::rename(&binary, &target).unwrap();
                symlink(target, binary).unwrap();
            }
            fs::remove_file(&f.log).unwrap();
        } else if kind == "base-symlink" {
            let target = f.path.join("sentinel");
            fs::create_dir(&target).unwrap();
            symlink(target, &base).unwrap();
        } else {
            fs::create_dir(&base).unwrap();
            fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
            match kind {
                "base-mode" => {
                    fs::set_permissions(&base, fs::Permissions::from_mode(0o777)).unwrap()
                }
                "lock-mode" => {
                    let lock = base.join("bootstrap.lock");
                    fs::write(&lock, "untouched").unwrap();
                    fs::set_permissions(lock, fs::Permissions::from_mode(0o666)).unwrap();
                }
                "base-owner" => executable(
                    &f.path.join("commands/stat"),
                    "printf '%s 700\\n' \"$(( $(id -u) + 1 ))\"\n",
                ),
                "lock-fifo" => {
                    let result = std::process::Command::new("mkfifo")
                        .arg(base.join("bootstrap.lock"))
                        .status()
                        .unwrap();
                    assert!(result.success());
                }
                _ => {
                    let target = f.path.join("sentinel");
                    fs::write(&target, "untouched").unwrap();
                    symlink(target, base.join("bootstrap.lock")).unwrap();
                }
            }
        }
        let result = f.invoke_args(
            bootstrap().to_str().unwrap(),
            &["--path", ".", "--growth-mib", "0"],
        );
        assert_ne!(result.code, 0, "{kind}");
        assert!(!f.log.exists(), "{kind}");
        if kind == "lock-symlink" {
            assert_eq!(
                fs::read_to_string(f.path.join("sentinel")).unwrap(),
                "untouched"
            );
        }
    }
}
#[test]
fn local_cache_uses_private_user_cache_and_secure_modes() {
    let mut f = Fixture::new("999999999999");
    f.env.remove("RUNNER_TEMP");
    f.env.insert(
        "XDG_CACHE_HOME".into(),
        f.path.join("user-cache").display().to_string(),
    );
    success(&f.invoke());
    let base = f.path.join("user-cache/openpilot-disk-guard");
    let binary = find_named(&base, "disk-space-guard")
        .into_iter()
        .find(|p| p.parent().unwrap().parent().unwrap() == base)
        .unwrap();
    for path in [&base, binary.parent().unwrap(), &binary] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    success(&f.invoke());
    assert_eq!(
        f.lines().iter().filter(|l| l.starts_with("cargo ")).count(),
        1
    );
}
