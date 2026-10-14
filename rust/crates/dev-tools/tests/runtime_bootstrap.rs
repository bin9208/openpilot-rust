use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::digest,
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const HELPER: &str = "openpilot-runtime-install-aarch64";
fn executable(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
struct Checkout {
    _temp: tempfile::TempDir,
    root: PathBuf,
    remote: PathBuf,
    commands: PathBuf,
    env: BTreeMap<String, String>,
}
impl Checkout {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout with spaces");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        git(
            &root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "fixture",
            ],
        );
        let remote = temp.path().join("remote");
        fs::create_dir(&remote).unwrap();
        let helper = remote.join(HELPER);
        executable(&helper,"#!/bin/bash\nset -eu\n[[ \"$1\" == --root && \"$2\" == \"$FIXTURE_ROOT\" ]]\nprintf 'installed\\n' > \"$2/installed\"\n");
        fs::write(
            remote.join(format!("{HELPER}.sha256")),
            format!("{}  {HELPER}\n", digest(&helper).unwrap()),
        )
        .unwrap();
        let commands = temp.path().join("commands");
        executable(
            &commands.join("curl"),
            r##"#!/bin/bash
set -eu
output=""
for ((i=1; i<=$#; i++)); do
  if [[ "${!i}" == --output ]]; then j=$((i+1)); output="${!j}"; fi
done
url="${!#}"
expected="https://github.com/bin9208/openpilot-rust/releases/download/runtime-$(git -C "$FIXTURE_ROOT" rev-parse HEAD)/"
[[ "$url" == "$expected"* ]]
printf '%s\n' "$url" >> "$FIXTURE_ROOT/requests"
if [[ -n ${FIXTURE_FAILURES:-} ]]; then
  count=$(wc -l < "$FIXTURE_ROOT/requests")
  IFS=, read -ra failures <<< "$FIXTURE_FAILURES"
  if (( count <= ${#failures[@]} )); then
    printf partial > "$output"
    printf '%s' "${FIXTURE_HTTP:-000}"
    exit "${failures[count-1]}"
  fi
fi
if [[ ! -f "$FIXTURE_REMOTE/${url##*/}" ]]; then exit 22; fi
cp "$FIXTURE_REMOTE/${url##*/}" "$output"
"##,
        );
        let env = BTreeMap::from([
            (
                "PATH".into(),
                format!("{}:{}", commands.display(), std::env::var("PATH").unwrap()),
            ),
            ("FIXTURE_ROOT".into(), root.to_str().unwrap().into()),
            ("FIXTURE_REMOTE".into(), remote.to_str().unwrap().into()),
        ]);
        Self {
            _temp: temp,
            root,
            remote,
            commands,
            env,
        }
    }
    fn script() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/ensure_runtime.sh")
    }
    fn run(&self) -> Captured {
        let mut command = CommandSpec::new(
            vec![
                "/bin/bash".into(),
                Self::script().to_str().unwrap().into(),
                "--root".into(),
                self.root.to_str().unwrap().into(),
            ],
            Duration::from_secs(35),
        );
        command.env = self.env.clone();
        NativeRunner.run(&command).unwrap()
    }
    fn requests(&self) -> usize {
        fs::read_to_string(self.root.join("requests"))
            .unwrap_or_default()
            .lines()
            .count()
    }
    fn cached(&self) {
        let cache = self
            .root
            .join("rust/.runtime-bootstrap")
            .join(git(&self.root, &["rev-parse", "HEAD"]));
        fs::create_dir_all(&cache).unwrap();
        for name in [HELPER.to_owned(), format!("{HELPER}.sha256")] {
            fs::copy(self.remote.join(&name), cache.join(name)).unwrap();
        }
    }
    fn no_partial(&self) {
        for entry in fs::read_dir(self.root.join("rust/.runtime-bootstrap")).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                assert!(!fs::read_dir(path).unwrap().any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("download.")));
            }
        }
    }
}
fn success(result: &Captured) {
    assert_eq!(
        result.code,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn fresh_checkout_downloads_verified_exact_source_helper() {
    let c = Checkout::new();
    success(&c.run());
    assert_eq!(fs::read(c.root.join("installed")).unwrap(), b"installed\n");
    assert_eq!(c.requests(), 2);
}
#[test]
fn bad_helper_hash_prevents_execution() {
    let c = Checkout::new();
    fs::write(
        c.remote.join(HELPER),
        "#!/bin/bash\ntouch \"$FIXTURE_ROOT/installed\"\n",
    )
    .unwrap();
    assert_ne!(c.run().code, 0);
    assert!(!c.root.join("installed").exists());
    assert_eq!(c.requests(), 2);
}
#[test]
fn valid_installed_package_needs_no_network() {
    let c = Checkout::new();
    executable(&c.root.join("rust/bin/openpilot-launch"),"#!/bin/bash\n[[ \"$1\" == --verify-package && \"$2\" == --root && \"$3\" == \"$FIXTURE_ROOT\" ]]\n");
    success(&c.run());
    assert_eq!(c.requests(), 0);
}
#[test]
fn missing_release_keeps_existing_runtime() {
    let c = Checkout::new();
    let binary = c.root.join("rust/bin/openpilot-launch");
    executable(&binary, "#!/bin/bash\nexit 1\n");
    let before = fs::read(&binary).unwrap();
    fs::remove_file(c.remote.join(format!("{HELPER}.sha256"))).unwrap();
    let r = c.run();
    assert_ne!(r.code, 0);
    assert_eq!(fs::read(binary).unwrap(), before);
    assert!(!c.root.join("installed").exists());
    assert_eq!(c.requests(), 1);
    assert!(String::from_utf8_lossy(&r.stderr).contains("curl_exit=22"));
}
#[test]
fn interrupted_installs_use_cached_helper_even_when_runtime_verifies() {
    for residue in [".runtime-install-journal.json", ".runtime-install-rollback"] {
        let c = Checkout::new();
        executable(
            &c.root.join("rust/bin/openpilot-launch"),
            "#!/bin/bash\nexit 0\n",
        );
        fs::write(c.root.join("rust").join(residue), []).unwrap();
        c.cached();
        success(&c.run());
        assert!(c.root.join("installed").is_file());
        assert_eq!(c.requests(), 0);
    }
}
#[test]
fn network_and_clock_recovery_outlast_old_retry_budget() {
    for (failures, status) in [("6,7,60,18", "000"), ("22,22,22,22", "503")] {
        let mut c = Checkout::new();
        c.env.insert("FIXTURE_FAILURES".into(), failures.into());
        c.env.insert("FIXTURE_HTTP".into(), status.into());
        let started = Instant::now();
        let r = c.run();
        success(&r);
        assert!(started.elapsed() >= Duration::from_secs(20));
        assert_eq!(fs::read(c.root.join("installed")).unwrap(), b"installed\n");
        assert_eq!(c.requests(), 6);
        assert!(String::from_utf8_lossy(&r.stdout)
            .to_lowercase()
            .contains("retry"));
        c.no_partial();
    }
}
struct Owned(Child, bool);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.1 {
            let pid = rustix::process::Pid::from_raw(self.0.id() as i32).unwrap();
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            let _ = self.0.wait();
        }
    }
}
#[test]
fn cancellation_reaps_download_or_backoff_and_never_executes_partial() {
    for phase in ["download", "backoff"] {
        let mut c = Checkout::new();
        executable(
            &c.commands.join("curl"),
            r##"#!/bin/bash
set -eu
printf '%s' "$$" > "$FIXTURE_ROOT/download-pid"
trap 'exit 143' TERM
for ((i=1; i<=$#; i++)); do
  if [[ "${!i}" == --output ]]; then j=$((i+1)); printf partial > "${!j}"; fi
done
if [[ "$FIXTURE_PHASE" == backoff ]]; then exit 6; fi
while :; do sleep 0.05; done
"##,
        );
        c.env.insert("FIXTURE_PHASE".into(), phase.into());
        let stdout = c.root.join("bootstrap.stdout");
        let child = Command::new("/bin/bash")
            .arg(Checkout::script())
            .arg("--root")
            .arg(&c.root)
            .envs(&c.env)
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(c.root.join("bootstrap.stderr")).unwrap())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut owned = Owned(child, false);
        let start = Instant::now();
        loop {
            if c.root.join("download-pid").exists()
                && (phase == "download"
                    || fs::read_to_string(&stdout)
                        .unwrap()
                        .contains("retry_seconds="))
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "bootstrap readiness timeout"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let pid: i32 = fs::read_to_string(c.root.join("download-pid"))
            .unwrap()
            .parse()
            .unwrap();
        rustix::process::kill_process(
            rustix::process::Pid::from_raw(owned.0.id() as i32).unwrap(),
            rustix::process::Signal::TERM,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = owned.0.try_wait().unwrap() {
                owned.1 = true;
                break status;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(143));
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        assert!(!c.root.join("installed").exists());
        c.no_partial();
    }
}
#[test]
fn cached_verified_helper_runs_without_network() {
    let c = Checkout::new();
    c.cached();
    success(&c.run());
    assert_eq!(fs::read(c.root.join("installed")).unwrap(), b"installed\n");
    assert_eq!(c.requests(), 0);
}
