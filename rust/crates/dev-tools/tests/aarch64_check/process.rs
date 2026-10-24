//! Labelled fake-qemu controller controls. It runs the actual host probe with
//! real msgq; these are lifecycle tests, not evidence of AArch64 execution.
use super::*;
use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    time::Duration,
};
const RUNNER: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$A64_LAUNCHES"
binary="$1"
root="$3"
mode="$4"
if [ "$A64_MODE" = 'fail' ]; then exit 7; fi
if [ "$A64_MODE" = 'hang' ]; then
 /bin/sleep 60 &
 printf '%s %s %s\n' "$A64_TOKEN" "$$" "$!" > "$A64_MARKER"
 wait
 exit 0
fi
shift
"$A64_HOST_PROBE" "$@"
if [ "$mode" = '--publish' ]; then
 case "$A64_MODE" in
  source-content) printf '\n# mutation\n' >> "$A64_SOURCE";;
  binary-content) printf '\n# mutation\n' >> "$binary";;
  runner-content) printf '\n# mutation\n' >> "$A64_RUNNER";;
  binary-retarget) /bin/rm "$A64_ARGUMENT"; /bin/ln -s "$A64_OTHER_BINARY" "$A64_ARGUMENT";;
  runner-retarget) /bin/rm "$A64_RUNNER_ARGUMENT"; /bin/ln -s "$A64_OTHER_RUNNER" "$A64_RUNNER_ARGUMENT";;
  replace-shm|replace-fixture)
   if [ "$A64_MODE" = 'replace-shm' ]; then path="/dev/shm/msgq_${OPENPILOT_PREFIX}"; else path="$root"; fi
   printf '%s' "$A64_TOKEN" > "$path/.test-owned-$A64_TOKEN"
   saved="$path.saved-$A64_TOKEN"
   /bin/mv "$path" "$saved"
   /bin/mkdir "$path"
   printf '%s' "$A64_TOKEN" > "$path/foreign-sentinel"
   printf '%s\n%s\n' "$path" "$saved" > "$A64_REPLACED";;
 esac
fi
"#;
struct Fixture {
    temp: tempfile::TempDir,
    source: PathBuf,
    binary: PathBuf,
    argument: PathBuf,
    runner: PathBuf,
    runner_argument: PathBuf,
    report: PathBuf,
    mode: String,
}
impl Fixture {
    fn new(mode: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let manifest: Value = serde_json::from_slice(
            &fs::read(
                repo.join("rust/crates/dev-tools/tests/fixtures/aarch64-check/manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        for (name, _) in manifest["source_pins"].as_object().unwrap() {
            let target = source.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            match fs::copy(repo.join(name), &target) {
                Ok(_) => {}
                Err(e)
                    if matches!(
                        name.as_str(),
                        "rust/tools/check_aarch64.py" | "rust/tools/check_proclog_reference.py"
                    ) && e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => panic!("{name}: {e}"),
            }
        }
        let rows = fixture("elf-controls.json");
        let elf = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["case"] == "static")
            .unwrap();
        let binary = temp.path().join("candidate");
        fs::write(&binary, unhex(elf["hex"].as_str().unwrap())).unwrap();
        let argument = temp
            .path()
            .join(OsString::from_vec(b"binary-\xff".to_vec()));
        symlink(&binary, &argument).unwrap();
        let runner = temp.path().join("runner");
        fs::write(&runner, RUNNER).unwrap();
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o700)).unwrap();
        let runner_argument = temp
            .path()
            .join(OsString::from_vec(b"runner-\xfe".to_vec()));
        symlink(&runner, &runner_argument).unwrap();
        fs::copy(&binary, temp.path().join("other-binary")).unwrap();
        fs::copy(&runner, temp.path().join("other-runner")).unwrap();
        let report = temp
            .path()
            .join(OsString::from_vec(b"report-\xfd.json".to_vec()));
        Self {
            temp,
            source,
            binary,
            argument,
            runner,
            runner_argument,
            report,
            mode: mode.into(),
        }
    }
    fn token(&self) -> &str {
        self.temp.path().file_name().unwrap().to_str().unwrap()
    }
    fn spec(&self) -> CommandSpec {
        let host = std::env::var_os("AARCH64_HOST_PROBE").expect(
            "mandatory fresh host openpilot-proclogd for labelled fake-qemu lifecycle tests",
        );
        let mut spec = CommandSpec::new_os(
            vec![
                env!("CARGO_BIN_EXE_check-aarch64").into(),
                "--binary".into(),
                self.argument.clone().into_os_string(),
                "--runner".into(),
                self.runner_argument.clone().into_os_string(),
                "--report".into(),
                self.report.clone().into_os_string(),
            ],
            Duration::from_secs(25),
        );
        let mut env = std::collections::BTreeMap::new();
        for (name, path) in [
            ("OPENPILOT_RUST_ROOT", self.source.clone()),
            ("A64_HOST_PROBE", host.into()),
            ("A64_LAUNCHES", self.temp.path().join("launches")),
            ("A64_MARKER", self.temp.path().join("marker")),
            ("A64_REPLACED", self.temp.path().join("replaced")),
            (
                "A64_SOURCE",
                self.source.join("openpilot/system/proclogd.py"),
            ),
            ("A64_RUNNER", self.runner.clone()),
            ("A64_ARGUMENT", self.argument.clone()),
            ("A64_RUNNER_ARGUMENT", self.runner_argument.clone()),
            ("A64_OTHER_BINARY", self.temp.path().join("other-binary")),
            ("A64_OTHER_RUNNER", self.temp.path().join("other-runner")),
        ] {
            env.insert(OsString::from(name), path.into_os_string());
        }
        env.insert("A64_TOKEN".into(), self.token().into());
        env.insert("A64_MODE".into(), self.mode.clone().into());
        env.insert("PATH".into(), "".into());
        spec.set_os_env(env);
        spec
    }
    fn run(&self) -> Captured {
        NativeRunner.run(&self.spec()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Only our private test's current nonce authorizes cleanup, including a failure
        // before assertions. Never trust an arbitrary path without its owned tag.
        if let Ok(text) = fs::read_to_string(self.temp.path().join("replaced")) {
            for name in text.lines() {
                let path = Path::new(name);
                if [
                    path.join(format!(".test-owned-{}", self.token())),
                    path.join("foreign-sentinel"),
                ]
                .iter()
                .any(|p| fs::read(p).ok().as_deref() == Some(self.token().as_bytes()))
                {
                    let _ = fs::remove_dir_all(path);
                }
            }
        }
    }
}
#[test]
fn actual_fake_qemu_uses_real_ipc_and_raw_cli_paths_without_changing_parent_environment() {
    let prefix = std::env::var_os("OPENPILOT_PREFIX");
    let fake = std::env::var_os("CEREAL_FAKE");
    let f = Fixture::new("exact");
    let result = f.run();
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&fs::read(&f.report).unwrap()).unwrap();
    assert_eq!(report["frames"], 3);
    assert_eq!(
        report["binary_sha256"],
        format!("{:x}", Sha256::digest(fs::read(&f.binary).unwrap()))
    );
    assert_eq!(report["device_validation"], "not_run");
    assert_eq!(
        fs::read_to_string(f.temp.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert_eq!(std::env::var_os("OPENPILOT_PREFIX"), prefix);
    assert_eq!(std::env::var_os("CEREAL_FAKE"), fake);
}
#[test]
fn actual_source_binary_runner_mutations_and_alias_retargets_cannot_publish_report() {
    for mode in [
        "source-content",
        "binary-content",
        "runner-content",
        "binary-retarget",
        "runner-retarget",
    ] {
        let f = Fixture::new(mode);
        let result = f.run();
        assert_eq!(
            result.code,
            1,
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!f.report.exists(), "{mode}: false PASS report");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("checker input changed"),
            "{mode}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read_to_string(f.temp.path().join("launches"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }
}
#[test]
fn actual_source_executable_and_runner_fifos_reject_before_child_launch() {
    use rustix::fs::{mkfifoat, Mode, CWD};
    for name in ["source", "binary", "runner"] {
        let f = Fixture::new("exact");
        let path = match name {
            "source" => f.source.join("rust/tools/device_probe.py"),
            "binary" => f.binary.clone(),
            _ => f.runner.clone(),
        };
        fs::remove_file(&path).unwrap();
        mkfifoat(CWD, &path, Mode::RUSR | Mode::WUSR | Mode::XUSR).unwrap();
        let result = f.run();
        assert_eq!(result.code, 1, "{name}");
        assert!(!f.report.exists());
        assert!(!f.temp.path().join("launches").exists());
    }
}
#[test]
fn actual_failed_admission_preserves_existing_reports_and_source_aliases() {
    let original = fixture("original-report-failures.json");
    for row in original["rows"].as_array().unwrap() {
        let mut f = Fixture::new("fail");
        let before = if row["target"] == "source-alias" {
            // Keep the source-alias control live after the two checkers retire.
            f.report = f.source.join("rust/tools/device_probe.py");
            fs::read(&f.report).unwrap()
        } else {
            let bytes = unhex(row["before_hex"].as_str().unwrap());
            fs::write(&f.report, &bytes).unwrap();
            bytes
        };
        if row["stage"] == "preflight" {
            fs::write(&f.binary, b"invalid ELF").unwrap();
        }
        let result = f.run();
        assert_eq!(result.code, row["exit"].as_i64().unwrap() as i32);
        assert_eq!(
            fs::read(&f.report).ok().as_deref(),
            Some(before.as_slice()),
            "{} {}: failed admission changed caller-owned output/source",
            row["stage"],
            row["target"]
        );
        assert!(row["preserved"].as_bool().unwrap());
    }
}
#[test]
fn actual_cleanup_does_not_adopt_foreign_replacement_directories() {
    for mode in ["replace-shm", "replace-fixture"] {
        let f = Fixture::new(mode);
        let result = f.run();
        let names = fs::read_to_string(f.temp.path().join("replaced"))
            .expect("runner must acknowledge actual replacement");
        let replaced = Path::new(names.lines().next().unwrap());
        assert_eq!(
            fs::read(replaced.join("foreign-sentinel")).ok().as_deref(),
            Some(f.token().as_bytes()),
            "{mode}: cleanup deleted foreign replacement"
        );
        assert_eq!(result.code, 1, "{mode}: replacement accepted");
        assert!(!f.report.exists());
    }
}
#[test]
fn actual_leader_only_sigint_cleans_acknowledged_qemu_and_sleeper() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Command, Stdio},
        time::Instant,
    };
    struct Checker {
        child: Child,
        id: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.id, Signal::INT);
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.id, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    struct Marked<'a>(&'a Fixture);
    impl Drop for Marked<'_> {
        fn drop(&mut self) {
            if let Ok(text) = fs::read_to_string(self.0.temp.path().join("marker")) {
                let fields: Vec<_> = text.split_whitespace().collect();
                if fields.first().copied() != Some(self.0.token()) {
                    return;
                }
                for p in fields.iter().skip(1) {
                    let Some(pid) = p.parse::<i32>().ok().and_then(Pid::from_raw) else {
                        continue;
                    };
                    let Ok(id) = pidfd_open(pid, PidfdFlags::empty()) else {
                        continue;
                    };
                    let env =
                        fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap_or_default();
                    if env
                        .split(|b| *b == 0)
                        .any(|v| v == format!("A64_TOKEN={}", self.0.token()).as_bytes())
                    {
                        let _ = pidfd_send_signal(&id, Signal::KILL);
                    }
                }
            }
        }
    }
    let f = Fixture::new("hang");
    let _fallback = Marked(&f);
    let spec = f.spec();
    let argv = spec.effective_argv();
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .envs(spec.effective_env())
        .stdin(Stdio::null())
        .stdout(fs::File::create(f.temp.path().join("stdout")).unwrap())
        .stderr(fs::File::create(f.temp.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, id };
    let mut ids = vec![];
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(text) = fs::read_to_string(f.temp.path().join("marker")) {
            let fields: Vec<_> = text.split_whitespace().collect();
            if fields.len() == 3 && fields[0] == f.token() {
                for raw in &fields[1..] {
                    let pid = Pid::from_raw(raw.parse().unwrap()).unwrap();
                    let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap();
                    assert!(env
                        .split(|b| *b == 0)
                        .any(|v| v == format!("A64_TOKEN={}", f.token()).as_bytes()));
                    ids.push(id);
                }
                break;
            }
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before acknowledgement"
        );
        assert!(Instant::now() < deadline, "qemu peer never acknowledged");
        std::thread::sleep(Duration::from_millis(5));
    }
    pidfd_send_signal(&checker.id, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = checker.child.try_wait().unwrap();
        let gone = ids.iter().all(|id| {
            poll(
                &mut [PollFd::new(id, PollFlags::IN)],
                Some(&Timespec::default()),
            )
            .unwrap()
                == 1
        });
        if let Some(status) = status {
            assert!(!status.success());
            if gone {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "leader-only cancellation left live owned descendants"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!f.report.exists());
}
