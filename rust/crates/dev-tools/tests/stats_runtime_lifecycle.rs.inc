// Included by stats_runtime_check: one shared lock serializes literal /tmp/stats.
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    os::{
        fd::OwnedFd,
        unix::{fs::symlink, net::UnixListener},
    },
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Running {
    child: Child,
    id: OwnedFd,
    descendants: Vec<OwnedFd>,
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.id, Signal::INT);
        let end = Instant::now() + Duration::from_secs(3);
        while Instant::now() < end {
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = pidfd_send_signal(&self.id, Signal::KILL);
        for id in &self.descendants {
            let _ = pidfd_send_signal(id, Signal::KILL);
        }
        let _ = self.child.wait();
    }
}
fn cancellation(replace: bool) {
    let temp = tempfile::tempdir().unwrap();
    let mut configured = options(&temp.path().join("output"));
    let originals = configured.binaries.clone();
    configured.binaries = temp.path().join("bin");
    fs::create_dir_all(configured.binaries.join("examples")).unwrap();
    symlink(
        originals.join("statsd-rs"),
        configured.binaries.join("statsd-rs"),
    )
    .unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stats-runtime/idle-producer");
    symlink(fixture, configured.binaries.join("examples/stats_flood")).unwrap();
    let token = format!("STATS_RUNTIME_TEST_TOKEN={}", temp.path().display());
    let stderr = temp.path().join("stderr");
    let child = Command::new(worker("STATS_FLOOD_CHECKER"))
        .args([&configured.binaries, &configured.output])
        .env("OPENPILOT_RUST_ROOT", &configured.root)
        .env("STATS_RUNTIME_TEST_TOKEN", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let id = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut owner = Running {
        child,
        id,
        descendants: vec![],
    };
    let ready = configured.output.join("SIGINT/producer-ready.json");
    let end = Instant::now() + Duration::from_secs(30);
    let prefix = loop {
        if let Ok(data) = fs::read(&ready) {
            if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                let pids = value["pids"].as_array().unwrap();
                assert_eq!(pids.len(), 5);
                for raw in pids {
                    let pid = Pid::from_raw(i32::try_from(raw.as_i64().unwrap()).unwrap()).unwrap();
                    let id = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    // A script producer can cross exec after printing ready.
                    // Hold the pidfd and await its real private environment ACK.
                    let end = Instant::now() + Duration::from_secs(5);
                    loop {
                        let env = fs::read(format!("/proc/{}/environ", pid.as_raw_pid()))
                            .unwrap_or_default();
                        if env.split(|b| *b == 0).any(|v| v == token.as_bytes()) {
                            break;
                        }
                        assert_eq!(
                            poll(
                                &mut [PollFd::new(&id, PollFlags::IN)],
                                Some(&Timespec::default())
                            )
                            .unwrap(),
                            0,
                            "producer exited before environment ACK"
                        );
                        assert!(
                            Instant::now() < end,
                            "owned PID environment was not acknowledged"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    owner.descendants.push(id);
                }
                break value["prefix"].as_str().unwrap().to_owned();
            }
        }
        assert!(
            owner.child.try_wait().unwrap().is_none(),
            "early checker exit: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < end,
            "producer ACK timeout: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        thread::sleep(Duration::from_millis(5));
    };
    let endpoint = Path::new("/tmp/stats");
    assert!(endpoint.exists());
    let foreign = if replace {
        fs::remove_file(endpoint).unwrap();
        Some(UnixListener::bind(endpoint).unwrap())
    } else {
        None
    };
    pidfd_send_signal(&owner.id, Signal::INT).unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        let status = owner.child.try_wait().unwrap();
        let gone = owner.descendants.iter().all(|id| {
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
            Instant::now() < end,
            "cancelled checker/descendants survived"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!configured.output.join("comparison.json").exists());
    assert!(!Path::new("/dev/shm")
        .join(format!("msgq_{prefix}"))
        .exists());
    assert!(!Path::new("/tmp")
        .join(format!("logmessage{prefix}"))
        .exists());
    if foreign.is_some() {
        let survived = endpoint.exists();
        if survived {
            fs::remove_file(endpoint).unwrap();
        }
        assert!(survived, "foreign endpoint removed");
    } else {
        assert!(!endpoint.exists(), "owned endpoint leaked");
    }
}
#[test]
fn owned_cancel_cleans_real_daemon_and_producers_but_preserves_replacement_endpoint() {
    let _guard = ENDPOINT.lock().unwrap();
    cancellation(false);
    cancellation(true);
}
#[test]
fn preexisting_foreign_endpoint_is_unchanged_by_both_controllers() {
    let _guard = ENDPOINT.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let foreign = UnixListener::bind("/tmp/stats").unwrap();
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file("/tmp/stats");
        }
    }
    let _cleanup = Cleanup;
    use std::os::unix::fs::MetadataExt;
    let original = fs::symlink_metadata("/tmp/stats").unwrap();
    for flood in [false, true] {
        let configured = options(&temp.path().join(if flood { "flood" } else { "runtime" }));
        let result = if flood {
            checker::run_flood(&configured, &worker("STATS_FLOOD_CHECKER"))
        } else {
            checker::run_runtime(&configured, &worker("STATS_RUNTIME_CHECKER"))
        };
        assert!(result.is_err());
        let now = fs::symlink_metadata("/tmp/stats").unwrap();
        assert_eq!((original.dev(), original.ino()), (now.dev(), now.ino()));
        assert!(!configured.output.join("comparison.json").exists());
        assert!(!configured.output.join("result.json").exists());
    }
    drop(foreign);
}

#[test]
fn actual_original_cli_arity_and_output_effect_controls() {
    let _guard = ENDPOINT.lock().unwrap();
    let bytes = include_bytes!("fixtures/stats-runtime/original-cli.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "915f1a2bea831da36741303eb0b2756f21a78c918cbc20774477f068725021dd"
    );
    let captured: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(captured["controls"].as_array().unwrap().len(), 16);
    let temp = tempfile::tempdir().unwrap();
    let originals = options(&temp.path().join("unused"));
    for row in captured["controls"].as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let case = row["case"].as_str().unwrap();
        let output = temp.path().join(format!("{kind}-{case}"));
        let binary = if case == "missing-binary" {
            temp.path().join("missing-bin")
        } else if case == "dashed-binary-path" {
            PathBuf::from("--help")
        } else {
            originals.binaries.clone()
        };
        let argv: Vec<std::ffi::OsString> = match case {
            "no-args" => vec![],
            "help" => vec!["--help".into()],
            "one-arg" => vec![binary.as_os_str().into()],
            "extra" => vec![
                binary.as_os_str().into(),
                output.as_os_str().into(),
                "extra".into(),
            ],
            _ => vec![binary.as_os_str().into(), output.as_os_str().into()],
        };
        if case == "existing-output" {
            fs::create_dir(&output).unwrap();
            fs::write(output.join("sentinel"), b"foreign output").unwrap();
        }
        let foreign = if case == "foreign-endpoint" {
            Some(UnixListener::bind("/tmp/stats").unwrap())
        } else {
            None
        };
        struct EndpointCleanup(bool);
        impl Drop for EndpointCleanup {
            fn drop(&mut self) {
                if self.0 {
                    let _ = fs::remove_file("/tmp/stats");
                }
            }
        }
        let _endpoint_cleanup = EndpointCleanup(foreign.is_some());
        use std::os::unix::fs::MetadataExt;
        let identity = foreign
            .as_ref()
            .map(|_| fs::symlink_metadata("/tmp/stats").unwrap());
        let result = Command::new(worker(if kind == "runtime" {
            "STATS_RUNTIME_CHECKER"
        } else {
            "STATS_FLOOD_CHECKER"
        }))
        .args(argv)
        .env("OPENPILOT_RUST_ROOT", &originals.root)
        .current_dir(&originals.root)
        .stdin(Stdio::null())
        .output()
        .unwrap();
        assert_eq!(
            result.status.code().map(i64::from),
            row["exit"].as_i64(),
            "{kind}/{case}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if let Some(identity) = identity {
            let now = fs::symlink_metadata("/tmp/stats").unwrap();
            assert_eq!((identity.dev(), identity.ino()), (now.dev(), now.ino()));
        }
        if case == "existing-output" {
            assert_eq!(
                fs::read(output.join("sentinel")).unwrap(),
                b"foreign output"
            );
        }
        let success = kind == "flood" && case == "existing-output";
        assert_eq!(
            output
                .join(if kind == "runtime" {
                    "result.json"
                } else {
                    "comparison.json"
                })
                .exists(),
            success,
            "{kind}/{case}"
        );
        if matches!(case, "missing-binary" | "dashed-binary-path") {
            let source = if kind == "runtime" {
                output.join("source")
            } else {
                output.join("SIGINT/source")
            };
            assert!(
                source.join("build.json").exists(),
                "spawn failure must retain original source-artifact phase"
            );
        }
    }
}
