//! Kernel tests require an explicitly delegated, job-owned memory controller.
//! The focused Actions job runs the ignored test with that real controller.
use openpilot_runtime_build::{
    memory_cgroup,
    resources::{DiskBudget, Monitor, Policy},
    supervision::{self, WorkerGuard},
};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const ROLE: &str = "OPENPILOT_RESOURCE_TEST_ROLE";

fn child(role: &str, root: &Path, case: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "real_memory_and_filesystem_limits_preserve_normal_execution",
            "--ignored",
            "--nocapture",
        ])
        .env(ROLE, role)
        .env("RESOURCE_ROOT", root)
        .env("RESOURCE_CASE", case);
    command
}

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Preserve only small fixture evidence, including when a test panics. The
/// TempDir still removes the large allocation file and all reproducible data.
struct Evidence(PathBuf);
impl Drop for Evidence {
    fn drop(&mut self) {
        let Some(output) = std::env::var_os("NATIVE_TEST_EVIDENCE_DIRECTORY") else {
            return;
        };
        let Ok(entries) = fs::read_dir(&self.0) else {
            return;
        };
        for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
            let destination = PathBuf::from(&output).join(entry.file_name());
            if fs::create_dir_all(&destination).is_err() {
                continue;
            }
            for name in ["resources.jsonl", "fixture.log", "policy.json"] {
                if let Err(error) = fs::copy(entry.path().join(name), destination.join(name)) {
                    eprintln!("fixture evidence {name}: {error}");
                }
            }
        }
    }
}

fn alive(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            s.rsplit_once(") ")
                .map(|(_, s)| !s.starts_with('Z') && !s.starts_with('X'))
        })
        .unwrap_or(false)
}

fn wait_file(path: &Path, process: &mut Child) {
    let until = Instant::now() + Duration::from_secs(45);
    while !fs::read_to_string(path).is_ok_and(|s| s.ends_with('\n')) {
        assert!(
            process.try_wait().unwrap().is_none(),
            "fixture exited before {}",
            path.display()
        );
        assert!(
            Instant::now() < until,
            "fixture readiness timeout: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn ordinary_directory_cannot_fake_a_memory_controller_or_start_a_build() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("normal.log"), b"normal runtime").unwrap();
    assert!(memory_cgroup::Group::create(temp.path(), 64 * 1024 * 1024).is_err());
    assert_eq!(
        fs::read(temp.path().join("normal.log")).unwrap(),
        b"normal runtime"
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
#[ignore = "requires the actual delegated memory controller prepared by native-resource-guard.yml"]
fn real_memory_and_filesystem_limits_preserve_normal_execution() {
    let role = std::env::var(ROLE).ok();
    if let Some(role) = role {
        let root = PathBuf::from(std::env::var_os("RESOURCE_ROOT").unwrap());
        let case = std::env::var("RESOURCE_CASE").unwrap();
        let policy: Policy =
            serde_json::from_reader(fs::File::open(root.join("policy.json")).unwrap()).unwrap();
        match role.as_str() {
            "launcher" => {
                let status = child("frontend", &root, &case)
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
                std::process::exit(if status.success() { 0 } else { 1 });
            }
            "frontend" => {
                let mut monitor = Monitor::start(
                    &policy,
                    &[root.as_path()],
                    &root.join("resources.jsonl"),
                    serde_json::json!({"case":case}),
                )
                .unwrap();
                fs::write(
                    root.join("frontend.ready"),
                    format!("{}\n", std::process::id()),
                )
                .unwrap();
                fs::write(
                    root.join("group-path"),
                    format!("{}\n", monitor.group.path.display()),
                )
                .unwrap();
                let mut command = child("worker", &root, &case);
                command.env(memory_cgroup::ENV, &monitor.group.path);
                let result = supervision::run_observed(command, || monitor.observe());
                let success = result.as_ref().is_ok_and(|s| s.success());
                let record = monitor.finish(success, result.as_ref().err().map(|e| e.to_string()));
                assert_eq!(success && record.is_ok(), case == "normal", "{case}");
                if !success || record.is_err() {
                    std::process::exit(1);
                }
            }
            "worker" => {
                let _guard = WorkerGuard::start(&root.join("params")).unwrap();
                memory_cgroup::enter(&policy.memory_cgroup_parent, policy.maximum_memory_bytes)
                    .unwrap();
                fs::write(
                    root.join("worker.ready"),
                    format!("{}\n", std::process::id()),
                )
                .unwrap();
                // A new session/process group must not escape memory accounting
                // or cancellation through the private cgroup.
                let mut detached = child("leaf", &root, &case);
                detached.process_group(0);
                assert!(detached.spawn().unwrap().wait().unwrap().success());
            }
            "leaf" => {
                fs::write(root.join("leaf.ready"), format!("{}\n", std::process::id())).unwrap();
                while !root.join("go").exists() {
                    thread::sleep(Duration::from_millis(10));
                }
                match case.as_str() {
                    "memory" => {
                        let mut chunks = Vec::new();
                        for _ in 0..48 {
                            chunks.push(vec![0x5a_u8; 16 * 1024 * 1024]);
                            std::hint::black_box(&chunks);
                            thread::sleep(Duration::from_millis(10));
                        }
                        panic!("memory cgroup failed to contain real 768 MiB allocation");
                    }
                    "disk" => {
                        use std::io::Write;
                        let mut file = fs::File::create(root.join("bounded-growth")).unwrap();
                        let block = vec![0x5a_u8; 1024 * 1024];
                        for _ in 0..160 {
                            file.write_all(&block).unwrap();
                            thread::sleep(Duration::from_millis(5));
                        }
                        loop {
                            thread::sleep(Duration::from_secs(1));
                        }
                    }
                    "normal" => {
                        let data = vec![0x5a_u8; 32 * 1024 * 1024];
                        std::hint::black_box(&data);
                        thread::sleep(Duration::from_millis(120));
                    }
                    _ => loop {
                        thread::sleep(Duration::from_secs(1));
                    },
                }
            }
            _ => panic!("unknown owned fixture role"),
        }
        return;
    }
    let parent = PathBuf::from(
        std::env::var_os("NATIVE_TEST_CGROUP_PARENT")
            .expect("real delegated memory controller is mandatory"),
    );
    let temp = tempfile::tempdir().unwrap();
    let _evidence = Evidence(temp.path().to_owned());
    let mut normal = Owned(Command::new("sleep").arg("300").spawn().unwrap());
    fs::write(
        temp.path().join("active.json"),
        b"previous verified generation A",
    )
    .unwrap();
    fs::write(temp.path().join("normal.log"), b"original runtime log").unwrap();
    for case in [
        "normal",
        "memory",
        "disk",
        "worker-kill",
        "frontend-kill",
        "launcher-kill",
    ] {
        let root = temp.path().join(case);
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(root.join("params/d")).unwrap();
        fs::write(root.join("params/d/IsOnroad"), b"0").unwrap();
        let policy = Policy {
            schema: 1,
            memory_cgroup_parent: parent.clone(),
            maximum_memory_bytes: 256 * 1024 * 1024,
            minimum_available_memory_bytes: 32 * 1024 * 1024,
            filesystems: BTreeMap::from([(
                root.clone(),
                DiskBudget {
                    maximum_growth_bytes: 64 * 1024 * 1024,
                    minimum_free_bytes: 32 * 1024 * 1024,
                },
            )]),
        };
        fs::write(
            root.join("policy.json"),
            serde_json::to_vec(&policy).unwrap(),
        )
        .unwrap();
        let log = fs::File::create(root.join("fixture.log")).unwrap();
        let mut command = child(
            if case == "launcher-kill" {
                "launcher"
            } else {
                "frontend"
            },
            &root,
            case,
        );
        command
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log));
        let mut frontend = Owned(command.spawn().unwrap());
        wait_file(&root.join("worker.ready"), &mut frontend.0);
        wait_file(&root.join("leaf.ready"), &mut frontend.0);
        let actual_frontend: u32 = fs::read_to_string(root.join("frontend.ready"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let worker: u32 = fs::read_to_string(root.join("worker.ready"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let leaf: u32 = fs::read_to_string(root.join("leaf.ready"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        match case {
            "frontend-kill" | "launcher-kill" => frontend.0.kill().unwrap(),
            "worker-kill" => rustix::process::kill_process(
                rustix::process::Pid::from_raw(i32::try_from(worker).unwrap()).unwrap(),
                rustix::process::Signal::KILL,
            )
            .unwrap(),
            _ => fs::write(root.join("go"), b"1").unwrap(),
        }
        let until = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = frontend.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < until, "{case}: frontend stop timeout");
            thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            status.success(),
            case == "normal",
            "{case}: {}",
            fs::read_to_string(root.join("fixture.log")).unwrap()
        );
        while (alive(actual_frontend) || alive(worker) || alive(leaf)) && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !alive(actual_frontend) && !alive(worker) && !alive(leaf),
            "{case}: detached build process survived"
        );
        assert!(
            normal.0.try_wait().unwrap().is_none(),
            "{case}: unrelated normal runtime was stopped"
        );
        assert_eq!(
            fs::read(temp.path().join("active.json")).unwrap(),
            b"previous verified generation A"
        );
        assert_eq!(
            fs::read(temp.path().join("normal.log")).unwrap(),
            b"original runtime log"
        );
        let journal = fs::read_to_string(root.join("resources.jsonl")).unwrap();
        let rows: Vec<serde_json::Value> = journal
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(!rows.is_empty());
        if case != "frontend-kill" {
            let last = rows.last().unwrap();
            assert_eq!(
                last["state"],
                if case == "normal" {
                    "success"
                } else {
                    "failed"
                }
            );
            assert!(last["memory"]["kernel_peak_bytes"].as_u64().unwrap() > 0);
            if case == "memory" {
                assert!(last["memory"]["limit_events"].as_u64().unwrap() > 0);
            }
        } else {
            assert!(rows.iter().all(|row| row["state"] != "success"));
            let group = PathBuf::from(fs::read_to_string(root.join("group-path")).unwrap().trim());
            assert_eq!(group.parent(), Some(parent.as_path()));
            assert!(fs::read_to_string(group.join("cgroup.procs"))
                .unwrap()
                .trim()
                .is_empty());
            fs::remove_dir(group).unwrap();
        }
        println!("case={case} actual_kernel_budget=true detached_descendants_stopped=true normal_runtime_and_logs_preserved=true");
    }
    normal.0.kill().unwrap();
    normal.0.wait().unwrap();
}
