//! Actual Cargo/build-script/grandchild processes. Executed in Actions only.
use openpilot_runtime_build::supervision::{self, WorkerGuard};
use rustix::process::{kill_process, kill_process_group, Pid, Signal};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "OPENPILOT_GUARD_FIXTURE_ROLE";
const ROOT: &str = "OPENPILOT_GUARD_FIXTURE_ROOT";
const CASE: &str = "OPENPILOT_GUARD_FIXTURE_CASE";

struct OwnedChild {
    child: Child,
    group: Option<Pid>,
    witnesses: Vec<(u32, String)>,
}
impl std::ops::Deref for OwnedChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}
impl std::ops::DerefMut for OwnedChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
fn identity(pid: u32) -> Option<(String, String)> {
    let data = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = data.rsplit_once(") ")?.1.split_whitespace().collect();
    Some((fields.get(2)?.to_string(), fields.get(19)?.to_string()))
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(group) = self.group {
            let number = group.as_raw_nonzero().get().to_string();
            if group != Pid::INIT
                && self.witnesses.iter().any(|(pid, start)| {
                    identity(*pid).is_some_and(|(pgid, seen)| pgid == number && seen == *start)
                })
            {
                let _ = kill_process_group(group, Signal::KILL);
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn child(role: &str, root: &Path, case: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "supervision_process_tree", "--nocapture"])
        .env(ROLE, role)
        .env(ROOT, root)
        .env(CASE, case);
    command
}

fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(") ")
                .map(|(_, tail)| tail.starts_with('Z') || tail.starts_with('X'))
        })
        .is_some_and(|dead| !dead)
}

#[test]
fn supervision_process_tree() {
    match std::env::var(ROLE).ok().as_deref() {
        Some("frontend") => {
            let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
            let case = std::env::var(CASE).unwrap();
            let status = supervision::run(child("worker", &root, &case)).unwrap();
            assert_eq!(status.success(), case == "normal");
            if !status.success() {
                std::process::exit(1);
            }
            return;
        }
        Some("worker") => {
            let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
            let _guard = WorkerGuard::start(&root.join("params")).unwrap();
            let lock = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(root.join("workspace.lock"))
                .unwrap();
            rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
            let mut cargo = Command::new("cargo")
                .args(["build", "--locked", "--offline", "--manifest-path"])
                .arg(root.join("crate/Cargo.toml"))
                .arg("--target-dir")
                .arg(root.join("target"))
                .env("NATIVE_FIXTURE_READY", root.join("script.ready"))
                .env(
                    "NATIVE_FIXTURE_NORMAL",
                    if std::env::var(CASE).unwrap() == "normal" {
                        "1"
                    } else {
                        "0"
                    },
                )
                .stdin(Stdio::null())
                .spawn()
                .unwrap();
            fs::write(
                root.join("worker.ready"),
                format!("{} {}\n", std::process::id(), cargo.id()),
            )
            .unwrap();
            assert!(cargo.wait().unwrap().success());
            return;
        }
        Some(other) => panic!("unknown owned fixture role {other}"),
        None => (),
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("params/d")).unwrap();
    fs::create_dir_all(root.join("crate/src")).unwrap();
    fs::write(
        root.join("crate/Cargo.toml"),
        "[package]\nname='native-guard-fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
    )
    .unwrap();
    fs::write(
        root.join("crate/Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"native-guard-fixture\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(root.join("crate/src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("crate/build.rs"),r#"
fn main() {
    let child=std::process::Command::new("sleep").arg("300")
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
    std::fs::write(std::env::var_os("NATIVE_FIXTURE_READY").unwrap(),format!("{} {}\n",std::process::id(),child.id())).unwrap();
    if std::env::var("NATIVE_FIXTURE_NORMAL").unwrap()!="1" { loop { std::thread::sleep(std::time::Duration::from_secs(1)); } }
}
"#).unwrap();
    fs::write(root.join("active.json"), b"previous validated generation A").unwrap();
    fs::write(root.join("normal.log"), b"previous runtime log").unwrap();
    let mut normal = OwnedChild {
        child: Command::new("sleep").arg("300").spawn().unwrap(),
        group: None,
        witnesses: Vec::new(),
    };
    for case in ["term", "frontend-kill", "worker-kill", "onroad", "normal"] {
        fs::write(root.join("params/d/IsOnroad"), b"0").unwrap();
        for path in ["worker.ready", "script.ready"] {
            let _ = fs::remove_file(root.join(path));
        }
        let log = fs::File::create(root.join(format!("{case}.log"))).unwrap();
        let mut command = child("frontend", root, case);
        command
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log));
        let mut frontend = OwnedChild {
            child: command.spawn().unwrap(),
            group: None,
            witnesses: Vec::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(180);
        while !root.join("worker.ready").is_file() || !root.join("script.ready").is_file() {
            if let Some(status) = frontend.try_wait().unwrap() {
                if root.join("worker.ready").is_file() && root.join("script.ready").is_file() {
                    break;
                }
                panic!(
                    "{case}: frontend exited before ready: {status}: {}",
                    fs::read_to_string(root.join(format!("{case}.log"))).unwrap()
                );
            }
            assert!(
                Instant::now() < deadline,
                "{case}: actual Cargo fixture did not become ready"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let parse = |path: &str| {
            let text = fs::read_to_string(root.join(path)).unwrap();
            if !text.ends_with('\n') {
                return Vec::new();
            }
            text.split_whitespace()
                .map(|p| p.parse::<u32>().unwrap())
                .collect::<Vec<_>>()
        };
        // Small ready files can become visible just before their write finishes.
        while parse("worker.ready").len() != 2 || parse("script.ready").len() != 2 {
            assert!(
                Instant::now() < deadline,
                "{case}: incomplete ready receipt"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let worker = parse("worker.ready");
        let script = parse("script.ready");
        let worker_pid = Pid::from_raw(i32::try_from(worker[0]).unwrap()).unwrap();
        frontend.group = Some(worker_pid);
        frontend.witnesses = [worker[0], worker[1], script[0], script[1]]
            .into_iter()
            .filter_map(|pid| identity(pid).map(|(_, start)| (pid, start)))
            .collect();
        match case {
            "term" => kill_process(
                Pid::from_raw(i32::try_from(frontend.id()).unwrap()).unwrap(),
                Signal::TERM,
            )
            .unwrap(),
            "frontend-kill" => frontend.kill().unwrap(),
            "worker-kill" => kill_process(worker_pid, Signal::KILL).unwrap(),
            "onroad" => fs::write(root.join("params/d/IsOnroad"), b"1").unwrap(),
            "normal" => (),
            _ => unreachable!(),
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = frontend.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = kill_process_group(worker_pid, Signal::KILL);
                let _ = frontend.kill();
                let _ = frontend.wait();
                panic!("{case}: native frontend did not stop");
            }
            thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.success(), case == "normal", "{case}");
        let pids = [worker[0], worker[1], script[0], script[1]];
        while pids.iter().any(|&pid| alive(pid)) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            pids.iter().all(|&pid| !alive(pid)),
            "{case}: live build descendant remains: {pids:?}"
        );
        assert!(
            normal.try_wait().unwrap().is_none(),
            "{case}: unrelated normal process stopped"
        );
        let lock = fs::OpenOptions::new()
            .write(true)
            .open(root.join("workspace.lock"))
            .unwrap();
        rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
        assert_eq!(
            fs::read(root.join("active.json")).unwrap(),
            b"previous validated generation A"
        );
        assert_eq!(
            fs::read(root.join("normal.log")).unwrap(),
            b"previous runtime log"
        );
        writeln!(std::io::stdout(),"case={case} actual_cargo_descendants_stopped=true unrelated_process_preserved=true workspace_lock_released=true").unwrap();
    }
    normal.kill().unwrap();
    normal.wait().unwrap();
}
