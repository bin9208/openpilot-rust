//! Hosted-only actual CLI/library/process-group checks. No Python provider.
#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use openpilot_dev_tools::changed_files_check::{run_controlled, RunControl};
use rustix::process::{getpid, kill_process, Pid, Signal};
use serde_json::{json, Value};
use std::{
    ffi::OsStr,
    fs::{self, File},
    io::Write,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
// Three generations in the Git-owned group; every identity includes Linux
// starttime so a test failure's cleanup cannot kill a reused PID. Descendants
// hold both output pipes, including when the Git leader exits successfully.
const PEER: &str = r#"#!/bin/sh
set -eu
note() {
  IFS=' ' read -r pid comm state ppid pgrp session tty tpg flags minflt cminflt majflt cmajflt utime stime cutime cstime priority nice threads itreal starttime rest < /proc/$$/stat
  printf '%s %s %s\n' "$$" "$pgrp" "$starttime" > "$AST_LIFECYCLE_ROOT/$1.pid"
}
if [ "${1-}" = descendant ]; then
  note "descendant-$2"
  /bin/sh "$AST_LIFECYCLE_ROOT/peer" grandchild "$2" &
  wait
  exit 0
fi
if [ "${1-}" = grandchild ]; then
  note "grandchild-$2"
  exec /bin/sleep 30
fi
printf '%s\000' "$@" >> "$AST_LIFECYCLE_ROOT/argv"
case "$1:${2-}" in
  merge-base:*) stage=merge ;;
  diff:--check) stage=whitespace ;;
  diff:--name-only) stage=paths ;;
  *) exit 97 ;;
esac
note "leader-$stage"
if [ "$stage" = "$AST_LIFECYCLE_STAGE" ]; then
  /bin/sh "$AST_LIFECYCLE_ROOT/peer" descendant "$stage" &
  while [ ! -s "$AST_LIFECYCLE_ROOT/grandchild-$stage.pid" ]; do /bin/sleep 0.01; done
  printf 'ready\n' > "$AST_LIFECYCLE_ROOT/ready"
  case "$AST_LIFECYCLE_BEHAVIOR" in
    block) wait ;;
    early) : ;;
    stdout-limit) exec /usr/bin/head -c 67108865 /dev/zero ;;
    stderr-limit) exec /usr/bin/head -c 1048577 /dev/zero >&2 ;;
    nonzero) printf '\377provider-error\n' >&2; exit 23 ;;
    whitespace-io)
      IFS= read -r input
      printf '%s\n' "$input" > "$AST_LIFECYCLE_ROOT/stdin"
      printf 'whitespace:\377\n'
      printf '\376stderr\n' >&2
      exit 2 ;;
    raw-paths) printf 'valid-\377.py\000line\nname.py\000'; exit 0 ;;
    *) exit 98 ;;
  esac
fi
if [ "$stage" = merge ]; then printf 'fixture-base\n'; fi
"#;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn read(path: impl AsRef<Path>) -> Vec<u8> {
    fs::read(path).unwrap()
}
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        if predicate() {
            return;
        }
        assert!(Instant::now() < deadline, "condition exceeded {WAIT:?}");
        thread::sleep(Duration::from_millis(10));
    }
}
#[derive(Clone, Debug)]
struct Identity {
    pid: Pid,
    group: i32,
    start: u64,
}
impl Identity {
    fn read(path: &Path) -> Option<Self> {
        let raw = fs::read_to_string(path).ok()?;
        let words: Vec<_> = raw.split_whitespace().collect();
        if words.len() != 3 {
            return None;
        }
        Some(Self {
            pid: Pid::from_raw(words[0].parse().ok()?)?,
            group: words[1].parse().ok()?,
            start: words[2].parse().ok()?,
        })
    }
    fn state(&self) -> Option<char> {
        let raw = fs::read_to_string(format!("/proc/{}/stat", self.pid.as_raw_pid())).ok()?;
        let (_, suffix) = raw.rsplit_once(") ")?;
        let words: Vec<_> = suffix.split_whitespace().collect();
        if words.get(2)?.parse::<i32>().ok()? != self.group
            || words.get(19)?.parse::<u64>().ok()? != self.start
        {
            return None;
        }
        words[0].chars().next()
    }
    fn alive(&self) -> bool {
        self.state()
            .is_some_and(|state| state != 'Z' && state != 'X')
    }
    fn evidence(&self) -> Value {
        json!({"pid":self.pid.as_raw_pid(),"group":self.group,"starttime":self.start,"state":self.state().map(|s|s.to_string()),"alive":self.alive()})
    }
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl OwnedChild {
    fn finish(&mut self) -> ExitStatus {
        let mut status = None;
        until(|| {
            status = self.0.try_wait().unwrap();
            status.is_some()
        });
        status.unwrap()
    }
}
struct Case {
    root: PathBuf,
    bystander: OwnedChild,
}
impl Drop for Case {
    fn drop(&mut self) {
        // Also runs on assertions, before the temporary directory disappears.
        for entry in fs::read_dir(&self.root).unwrap().flatten() {
            if entry.path().extension() == Some(OsStr::new("pid")) {
                if let Some(identity) = Identity::read(&entry.path()) {
                    if identity.alive() {
                        let _ = kill_process(identity.pid, Signal::KILL);
                    }
                }
            }
        }
    }
}
impl Case {
    fn new() -> Self {
        let cases =
            PathBuf::from(std::env::var_os("AST_LIFECYCLE_PROOF").expect("hosted proof directory"))
                .join("cases");
        fs::create_dir_all(&cases).unwrap();
        // Retain failure diagnostics and provider scripts in the hosted artifact.
        let root = tempfile::Builder::new()
            .prefix("case-")
            .tempdir_in(cases)
            .unwrap()
            .keep();
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(root.join("peer"), PEER).unwrap();
        let git = root.join("bin/git");
        fs::write(&git, PEER).unwrap();
        fs::set_permissions(git, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            root,
            bystander: OwnedChild(
                Command::new("/bin/sleep")
                    .arg("30")
                    .process_group(0)
                    .spawn()
                    .unwrap(),
            ),
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
    fn command(&self, cli: bool, stage: &str, behavior: &str) -> Command {
        let mut command = if cli {
            Command::new(env!("CARGO_BIN_EXE_check-changed-files"))
        } else {
            let mut cmd = Command::new(std::env::current_exe().unwrap());
            cmd.args(["--ignored", "--exact", "library_worker", "--nocapture"]);
            cmd
        };
        command
            .current_dir(&self.root)
            .env_remove("BASE_SHA")
            .env_remove("AST_LIFECYCLE_CONTROL")
            .env("PATH", self.path("bin"))
            .env("AST_LIFECYCLE_ROOT", &self.root)
            .env("AST_LIFECYCLE_STAGE", stage)
            .env("AST_LIFECYCLE_BEHAVIOR", behavior)
            .stdin(Stdio::null())
            .stdout(File::create(self.path("stdout")).unwrap())
            .stderr(File::create(self.path("stderr")).unwrap());
        command
    }
    fn ready(&self) {
        until(|| fs::read(self.path("ready")).is_ok_and(|bytes| bytes == b"ready\n"));
    }
    fn identities(&self, stage: &str) -> Vec<Identity> {
        let result: Vec<_> = ["leader", "descendant", "grandchild"]
            .into_iter()
            .map(|role| Identity::read(&self.path(&format!("{role}-{stage}.pid"))).unwrap())
            .collect();
        assert_eq!(result[0].group, result[0].pid.as_raw_pid());
        assert_ne!(result[0].group, getpid().as_raw_pid());
        for identity in &result {
            assert_eq!(identity.group, result[0].group);
        }
        result
    }
    fn cleaned(&mut self, stage: &str) -> Vec<Value> {
        let identities = self.identities(stage);
        until(|| identities.iter().all(|id| !id.alive()));
        // Successful cleanup cannot include the separate process group.
        assert!(self.bystander.0.try_wait().unwrap().is_none());
        identities.iter().map(Identity::evidence).collect()
    }
    fn result(&self) -> Value {
        serde_json::from_slice(&read(self.path("result.json"))).unwrap()
    }
    fn evidence(&self, details: Value) -> Value {
        let bystander =
            fs::read_to_string(format!("/proc/{}/stat", self.bystander.0.id())).unwrap();
        let (_, suffix) = bystander.rsplit_once(") ").unwrap();
        let state = suffix.split_whitespace().next().unwrap();
        assert!(!["Z", "X"].contains(&state));
        json!({"details":details,"case_root":self.root,"bystander_pid":self.bystander.0.id(),"bystander_state":state,"stdout_hex":hex(&read(self.path("stdout"))),"stderr_hex":hex(&read(self.path("stderr"))),"argv_hex":fs::read(self.path("argv")).ok().map(|v|hex(&v))})
    }
}
fn save(name: &str, rows: Value) {
    let proof =
        PathBuf::from(std::env::var_os("AST_LIFECYCLE_PROOF").expect("hosted proof directory"));
    fs::create_dir_all(&proof).unwrap();
    fs::write(proof.join(name), serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
}

#[test]
#[ignore = "isolated subprocess entrypoint, invoked by lifecycle checks"]
fn library_worker() {
    let root = PathBuf::from(std::env::var_os("AST_LIFECYCLE_ROOT").unwrap());
    let mode = std::env::var("AST_LIFECYCLE_CONTROL").unwrap_or_else(|_| "normal".into());
    let timeout = if mode == "timeout" {
        Duration::from_secs(2)
    } else if mode == "expired" {
        Duration::from_nanos(1)
    } else {
        WAIT
    };
    let control = RunControl::new(timeout).unwrap();
    if mode == "pre-cancel" {
        control.cancel();
    }
    let cancel = if mode == "cancel" {
        let control = control.clone();
        let ready = root.join("ready");
        Some(thread::spawn(move || {
            until(|| fs::read(&ready).is_ok_and(|bytes| bytes == b"ready\n"));
            control.cancel();
        }))
    } else {
        None
    };
    let started = Instant::now();
    let result = run_controlled(&root, None, &control);
    if let Some(cancel) = cancel {
        cancel.join().unwrap();
    }
    let result = match result {
        Ok(report) => {
            json!({"ok":true,"base_hex":hex(report.base.as_bytes()),"paths_hex":report.checked_paths.iter().map(|p|hex(p.as_os_str().as_bytes())).collect::<Vec<_>>()})
        }
        Err(error) => {
            json!({"ok":false,"phase":format!("{:?}",error.phase),"message":error.to_string(),"path_hex":error.path.map(|p|hex(p.as_os_str().as_bytes()))})
        }
    };
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(
            &json!({"result":result,"duration_ms":started.elapsed().as_millis()}),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn cli_int_term_cancel_every_git_phase_and_kill_owned_descendants() {
    let mut rows = vec![];
    for stage in ["merge", "whitespace", "paths"] {
        for signal in [Signal::INT, Signal::TERM] {
            let mut case = Case::new();
            let mut cli = OwnedChild(case.command(true, stage, "block").spawn().unwrap());
            case.ready();
            let identities = case.identities(stage);
            assert!(identities.iter().all(Identity::alive));
            kill_process(
                Pid::from_raw(i32::try_from(cli.0.id()).unwrap()).unwrap(),
                signal,
            )
            .unwrap();
            assert!(!cli.finish().success());
            assert!(read(case.path("stderr"))
                .windows(9)
                .any(|w| w == b"Cancelled"));
            let cleanup = case.cleaned(stage);
            rows.push(
                case.evidence(
                    json!({"stage":stage,"signal":format!("{signal:?}"),"cleanup":cleanup}),
                ),
            );
            save("cli-signals.json", json!(rows));
        }
    }
}

#[test]
fn library_cancel_and_shared_deadline_clean_every_git_phase() {
    let mut rows = vec![];
    for stage in ["merge", "whitespace", "paths"] {
        for (mode, phase) in [("cancel", "Cancelled"), ("timeout", "Timeout")] {
            let mut case = Case::new();
            let mut worker = OwnedChild(
                case.command(false, stage, "block")
                    .env("AST_LIFECYCLE_CONTROL", mode)
                    .spawn()
                    .unwrap(),
            );
            assert!(worker.finish().success());
            let result = case.result();
            assert_eq!(result["result"]["phase"], phase);
            let cleanup = case.cleaned(stage);
            rows.push(
                case.evidence(
                    json!({"stage":stage,"mode":mode,"observed":result,"cleanup":cleanup}),
                ),
            );
            save("library-control.json", json!(rows));
        }
    }
}

#[test]
fn exited_unreaped_git_leaders_do_not_leave_pipe_holding_descendants() {
    let mut rows = vec![];
    for stage in ["merge", "whitespace", "paths"] {
        let mut case = Case::new();
        let mut worker = OwnedChild(case.command(false, stage, "early").spawn().unwrap());
        assert!(worker.finish().success());
        let result = case.result();
        assert_eq!(result["result"]["ok"], true);
        let cleanup = case.cleaned(stage);
        rows.push(case.evidence(json!({"stage":stage,"observed":result,"cleanup":cleanup})));
        save("exited-leaders.json", json!(rows));
    }
}

#[test]
fn captured_output_limits_reject_without_partial_admission_and_clean_group() {
    let mut rows = vec![];
    for stage in ["merge", "paths"] {
        for (behavior, stream, limit) in [
            ("stdout-limit", "stdout", 67108864),
            ("stderr-limit", "stderr", 1048576),
        ] {
            let mut case = Case::new();
            let mut worker = OwnedChild(case.command(false, stage, behavior).spawn().unwrap());
            assert!(worker.finish().success());
            let result = case.result();
            assert_eq!(result["result"]["phase"], "Git");
            let message = result["result"]["message"].as_str().unwrap();
            assert!(
                message.contains(&format!("{stream} exceeds {limit} captured bytes")),
                "{message}"
            );
            let cleanup = case.cleaned(stage);
            rows.push(case.evidence(json!({"stage":stage,"stream":stream,"limit":limit,"observed":result,"cleanup":cleanup})));
            save("output-limits.json", json!(rows));
        }
    }
}

#[test]
fn nonzero_git_errors_and_whitespace_inherited_bytes_keep_failure_order() {
    let mut rows = vec![];
    for stage in ["merge", "paths"] {
        let mut case = Case::new();
        let mut worker = OwnedChild(case.command(false, stage, "nonzero").spawn().unwrap());
        assert!(worker.finish().success());
        let result = case.result();
        assert_eq!(result["result"]["phase"], "Git");
        assert!(result["result"]["message"]
            .as_str()
            .unwrap()
            .contains("exited 23"));
        let cleanup = case.cleaned(stage);
        rows.push(case.evidence(json!({"stage":stage,"observed":result,"cleanup":cleanup})));
    }
    let mut case = Case::new();
    fs::write(case.path("broken.py"), b"def broken(:\n").unwrap();
    let mut cli = OwnedChild(
        case.command(true, "whitespace", "whitespace-io")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    cli.0
        .stdin
        .take()
        .unwrap()
        .write_all(b"inherited stdin\n")
        .unwrap();
    assert!(!cli.finish().success());
    assert_eq!(read(case.path("stdin")), b"inherited stdin\n");
    assert_eq!(read(case.path("stdout")), b"whitespace:\xff\n");
    let stderr = read(case.path("stderr"));
    assert!(stderr.starts_with(b"\xfestderr\n"));
    let diagnostic = String::from_utf8_lossy(&stderr);
    assert!(diagnostic.contains("Whitespace"));
    assert!(!diagnostic.contains("Syntax"));
    assert!(!String::from_utf8_lossy(&read(case.path("argv"))).contains("--name-only"));
    let cleanup = case.cleaned("whitespace");
    rows.push(case.evidence(
        json!({"stage":"whitespace","stdin_hex":hex(&read(case.path("stdin"))),"cleanup":cleanup}),
    ));
    save("nonzero-and-inherited-io.json", json!(rows));
}

#[test]
fn cli_bad_options_help_and_spawn_errors_do_not_admit_files() {
    let mut rows = vec![];
    for args in [
        vec!["--unknown"],
        vec!["--base"],
        vec!["--base", "a", "--base", "b"],
        vec!["--base", "a", "--unknown"],
    ] {
        let case = Case::new();
        let mut cli = OwnedChild(
            case.command(true, "paths", "early")
                .args(&args)
                .spawn()
                .unwrap(),
        );
        assert!(!cli.finish().success());
        assert!(!case.path("argv").exists());
        assert!(read(case.path("stdout")).is_empty());
        rows.push(case.evidence(json!({"args":args,"provider_spawned":false})));
    }
    let case = Case::new();
    let mut cli = OwnedChild(
        case.command(true, "paths", "early")
            .arg("--help")
            .spawn()
            .unwrap(),
    );
    assert!(cli.finish().success());
    assert!(!case.path("argv").exists());
    assert_eq!(
        read(case.path("stdout")),
        b"Usage: check-changed-files [--base SHA] (BASE_SHA or origin/dev fallback)\n"
    );
    rows.push(case.evidence(json!({"args":["--help"],"provider_spawned":false})));
    let case = Case::new();
    fs::remove_file(case.path("bin/git")).unwrap();
    let mut worker = OwnedChild(case.command(false, "paths", "early").spawn().unwrap());
    assert!(worker.finish().success());
    assert_eq!(case.result()["result"]["phase"], "Git");
    assert!(!case.path("argv").exists());
    rows.push(case.evidence(json!({"case":"spawn-failure","observed":case.result()})));
    save("cli-options-and-spawn.json", json!(rows));
}

#[test]
fn precancelled_and_expired_library_controls_prevent_provider_spawn() {
    let mut rows = vec![];
    assert!(RunControl::new(Duration::ZERO).is_err());
    assert!(RunControl::new(Duration::MAX).is_err());
    for (mode, phase) in [("pre-cancel", "Cancelled"), ("expired", "Timeout")] {
        let case = Case::new();
        let mut worker = OwnedChild(
            case.command(false, "paths", "early")
                .env("AST_LIFECYCLE_CONTROL", mode)
                .spawn()
                .unwrap(),
        );
        assert!(worker.finish().success());
        assert_eq!(case.result()["result"]["phase"], phase);
        assert!(!case.path("argv").exists());
        rows.push(
            case.evidence(json!({"mode":mode,"observed":case.result(),"provider_spawned":false})),
        );
    }
    save("pre-spawn-control.json", json!(rows));
}

#[test]
fn raw_nul_paths_and_exact_git_argv_survive_process_layer() {
    let mut case = Case::new();
    for name in [
        OsStr::from_bytes(b"valid-\xff.py"),
        OsStr::new("line\nname.py"),
    ] {
        fs::write(case.root.join(name), b"x=1\n").unwrap();
    }
    let mut worker = OwnedChild(case.command(false, "paths", "raw-paths").spawn().unwrap());
    assert!(worker.finish().success());
    let result = case.result();
    assert_eq!(result["result"]["ok"], true);
    assert_eq!(result["result"]["base_hex"], hex(b"fixture-base"));
    assert_eq!(
        result["result"]["paths_hex"],
        json!([hex(b"valid-\xff.py"), hex(b"line\nname.py")])
    );
    assert_eq!(read(case.path("argv")), b"merge-base\0origin/dev\0HEAD\0diff\0--check\0fixture-base\0HEAD\0diff\0--name-only\0-z\0--diff-filter=ACMR\0fixture-base\0HEAD\0");
    let cleanup = case.cleaned("paths");
    save(
        "raw-nul-paths.json",
        case.evidence(json!({"observed":result,"cleanup":cleanup})),
    );
}
