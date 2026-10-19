//! Fault controls run real native controller processes with owned private inputs.
use super::*;
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    time::Duration,
};
fn executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn copy_sources(destination: &Path) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest: Value =
        serde_json::from_str(include_str!("../fixtures/stats-daemon/manifest.json")).unwrap();
    let params: Value =
        serde_json::from_str(include_str!("../fixtures/stats-daemon/params-sources.json")).unwrap();
    let mut paths = manifest["source_sha256"].as_object().unwrap().clone();
    paths.extend(params.as_object().unwrap().clone());
    for name in paths.keys() {
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(root.join(name), target) {
            Ok(_) => (),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && matches!(
                        name.as_str(),
                        "rust/tools/check_stats_daemon.py"
                            | "rust/tools/check_stats_failures.py"
                            | "rust/tools/check_stats_log_fault.py"
                            | "rust/tools/stats_source.py"
                    ) => {}
            Err(e) => panic!("{name}: {e}"),
        }
    }
}
#[test]
fn source_or_candidate_fifo_is_rejected_without_waiting_or_receipt() {
    for kind in ["source", "candidate", "binding"] {
        let temp = tempfile::tempdir().unwrap();
        let (mut options, worker) = configured(&temp.path().join("output"));
        options.root = temp.path().join("source");
        copy_sources(&options.root);
        options.binaries = temp.path().join("candidate");
        fs::create_dir_all(options.binaries.join("examples")).unwrap();
        for name in ["stats_daemon", "stats_producer"] {
            executable(
                &options.binaries.join("examples").join(name),
                b"#!/bin/sh\nexit 99\n",
            );
        }
        let path = match kind {
            "source" => options.root.join("openpilot/system/statsd.py"),
            "candidate" => options.binaries.join("examples/stats_daemon"),
            _ => {
                options.binding = temp.path().join("binding");
                options.binding.clone()
            }
        };
        if path.exists() {
            fs::remove_file(&path).unwrap();
        }
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &path,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        let mut spec = CommandSpec::new_os(
            vec![
                worker.into_os_string(),
                options.binaries.into_os_string(),
                options.binding.into_os_string(),
                options.output.clone().into_os_string(),
            ],
            Duration::from_secs(3),
        );
        spec.env.insert(
            "OPENPILOT_RUST_ROOT".into(),
            options.root.display().to_string(),
        );
        let result = NativeRunner.run(&spec).unwrap();
        assert_eq!(result.code, 1, "{kind}");
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("not a regular file"),
            "{kind}: {:?}",
            result.stderr
        );
        assert!(!options.output.exists());
    }
}
#[test]
fn identity_change_after_worker_result_cannot_write_success() {
    for mode in ["content", "replacement", "symlink"] {
        let temp = tempfile::tempdir().unwrap();
        let (mut options, _) = configured(&temp.path().join("output"));
        options.root = temp.path().join("source");
        copy_sources(&options.root);
        let source = options.root.join("openpilot/system/statsd.py");
        let first = temp.path().join("first");
        let other = temp.path().join("other");
        fs::copy(&source, &other).unwrap();
        if mode == "symlink" {
            fs::rename(&source, &first).unwrap();
            symlink(&first, &source).unwrap();
        }
        let payload = temp.path().join("payload");
        fs::write(
            &payload,
            serde_json::to_vec(&original()["log-fault"]).unwrap(),
        )
        .unwrap();
        let mutation = match mode {
            "content" => format!("printf '\\n# changed\\n' >> '{}'", source.display()),
            "replacement" => format!("/bin/mv '{}' '{}'", other.display(), source.display()),
            _ => format!(
                "/bin/rm '{}'; /bin/ln -s '{}' '{}'",
                source.display(),
                other.display(),
                source.display()
            ),
        };
        let worker = temp.path().join("worker");
        executable(
            &worker,
            format!(
                "#!/bin/sh\nset -eu\n{mutation}\n/bin/cat '{}'\n",
                payload.display()
            )
            .as_bytes(),
        );
        let error = checker::run_case(&options, "log-fault", &worker)
            .unwrap_err()
            .to_string();
        assert!(error.contains("input changed"), "{mode}: {error}");
        assert!(!options.output.join("result.json").exists());
        assert!(!fs::read(options.output.join("worker.stdout"))
            .unwrap()
            .is_empty());
    }
}
#[test]
fn leader_sigint_stops_acknowledged_tree_and_cleans_private_namespaces() {
    cancellation(false);
}
#[test]
fn cancellation_preserves_replacement_foreign_collector_endpoint() {
    cancellation(true);
}
fn cancellation(replace_endpoint: bool) {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Command, Stdio},
        thread,
        time::Instant,
    };
    struct Leader {
        child: Child,
        id: OwnedFd,
    }
    impl Drop for Leader {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.id, Signal::INT);
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.id, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    struct Descendants(Vec<OwnedFd>);
    impl Drop for Descendants {
        fn drop(&mut self) {
            for id in &self.0 {
                let _ = pidfd_send_signal(id, Signal::KILL);
            }
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let (mut options, worker) = configured(&temp.path().join("output"));
    options.binaries = temp.path().join("candidate");
    let examples = options.binaries.join("examples");
    fs::create_dir_all(&examples).unwrap();
    executable(
        &examples.join("stats_daemon"),
        br#"#!/bin/sh
/bin/sleep 60 &
printf '%s\n' "$$" "$!" "$PPID" "$OPENPILOT_PREFIX" "$1" > "$STATS_CHECK_READY"
wait
"#,
    );
    executable(&examples.join("stats_producer"), b"#!/bin/sh\nexit 99\n");
    let ready = temp.path().join("ready");
    let token = temp.path().file_name().unwrap().to_str().unwrap();
    let child = Command::new(worker)
        .args([&options.binaries, &options.binding, &options.output])
        .env("OPENPILOT_RUST_ROOT", &options.root)
        .env("STATS_CHECK_READY", &ready)
        .env("STATS_CHECK_TOKEN", token)
        .stdout(fs::File::create(temp.path().join("stdout")).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut leader = Leader { child, id };
    let mut descendants = Descendants(vec![]);
    let deadline = Instant::now() + Duration::from_secs(15);
    let (shm, socket, log) = loop {
        if let Ok(text) = fs::read_to_string(&ready) {
            let fields: Vec<_> = text.lines().collect();
            if fields.len() == 5 {
                for pid in &fields[..3] {
                    let pid = Pid::from_raw(pid.parse().unwrap()).unwrap();
                    let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap();
                    assert!(env
                        .split(|b| *b == 0)
                        .any(|b| b == format!("STATS_CHECK_TOKEN={token}").as_bytes()));
                    descendants.0.push(id);
                }
                break (
                    PathBuf::from(format!("/dev/shm/msgq_{}", fields[3])),
                    PathBuf::from(fields[4].strip_prefix("ipc://").unwrap())
                        .parent()
                        .unwrap()
                        .to_owned(),
                    PathBuf::from(format!("/tmp/logmessage{}", fields[3])),
                );
            }
        }
        assert!(
            leader.child.try_wait().unwrap().is_none(),
            "checker exited before acknowledgement"
        );
        assert!(Instant::now() < deadline, "candidate did not acknowledge");
        thread::sleep(Duration::from_millis(5));
    };
    assert!(shm.exists() && socket.exists() && log.exists());
    let replacement = if replace_endpoint {
        fs::remove_file(&log).unwrap();
        Some(std::os::unix::net::UnixListener::bind(&log).unwrap())
    } else {
        None
    };
    pidfd_send_signal(&leader.id, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = leader.child.try_wait().unwrap();
        let gone = descendants.0.iter().all(|id| {
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
            "acknowledged descendant survived leader cancellation"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !shm.exists() && !socket.exists(),
        "private IPC namespace leaked"
    );
    if replacement.is_some() {
        let survived = log.exists();
        if survived {
            fs::remove_file(&log).unwrap();
        }
        assert!(survived, "foreign replacement endpoint deleted");
    } else {
        assert!(!log.exists(), "collector endpoint leaked");
    }
    assert!(!options.output.join("result.json").exists());
    assert!(!options.output.join("comparison.json").exists());
    assert!(fs::read(temp.path().join("stdout")).unwrap().is_empty());
}
