use super::*;
use std::{
    ffi::OsString,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
    path::PathBuf,
    process::Command,
};
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn root_copy(include_checker: bool) -> tempfile::TempDir {
    let copy = tempfile::tempdir().unwrap();
    for name in [
        "openpilot/selfdrive/carrot/bluetooth/daemon.py",
        "rust/tools/check_bluetooth_input_decode.py",
    ] {
        if !include_checker && name.starts_with("rust/tools/") {
            continue;
        }
        let target = copy.path().join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        match fs::copy(repository().join(name), target) {
            Ok(_) => (),
            Err(error)
                if name.starts_with("rust/tools/")
                    && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("{name}: {error}"),
        }
    }
    copy
}
struct MutatingPeer {
    peer: Peer,
    action: Box<dyn FnMut()>,
}
impl Runner for MutatingPeer {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        let result = self.peer.run(command)?;
        (self.action)();
        Ok(result)
    }
}
fn peer(action: impl FnMut() + 'static) -> MutatingPeer {
    MutatingPeer {
        peer: Peer {
            mode: "exact".into(),
            lines: original_lines(),
            calls: 0,
        },
        action: Box::new(action),
    }
}
#[test]
fn executable_argument_retarget_cannot_publish_mixed_identity_success() {
    let root = root_copy(true);
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let argument = dir.path().join("candidate");
    fs::write(&first, b"first executable identity\n").unwrap();
    fs::write(&second, b"other executable identity\n").unwrap();
    symlink(&first, &argument).unwrap();
    let change = argument.clone();
    let mut peer = peer(move || {
        fs::remove_file(&change).unwrap();
        symlink(&second, &change).unwrap();
    });
    let output = dir.path().join("output");
    let result = openpilot_dev_tools::bluetooth_input_check::check(
        root.path(),
        &argument,
        &output,
        &mut peer,
        &mut vec![],
    );
    assert!(result.is_err(), "changed executable argument produced PASS");
    assert_eq!(peer.peer.calls, 1);
    assert!(!output.join("result.json").exists());
}
#[test]
fn owned_file_content_replacement_and_source_roster_drift_are_rejected() {
    for mode in [
        "binary-content",
        "binary-replace",
        "source-content",
        "source-replace",
        "helper-appears",
    ] {
        let root = root_copy(mode != "helper-appears");
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        fs::write(&binary, b"fixture executable identity\n").unwrap();
        let source = root
            .path()
            .join("openpilot/selfdrive/carrot/bluetooth/daemon.py");
        let helper = root
            .path()
            .join("rust/tools/check_bluetooth_input_decode.py");
        let target = if mode.starts_with("binary") {
            binary.clone()
        } else {
            source
        };
        let replacement = dir.path().join("replacement");
        fs::write(&replacement, b"different identity\n").unwrap();
        let mut peer = peer(move || match mode {
            "helper-appears" => {
                fs::create_dir_all(helper.parent().unwrap()).unwrap();
                // Reintroduction with unaudited bytes must fail even when the
                // originally verified helper is no longer in the checkout.
                fs::write(&helper, b"reintroduced unaudited helper\n").unwrap();
            }
            "binary-replace" | "source-replace" => fs::rename(&replacement, &target).unwrap(),
            _ => fs::write(&target, b"changed owned input\n").unwrap(),
        });
        let output = dir.path().join("output");
        assert!(
            openpilot_dev_tools::bluetooth_input_check::check(
                root.path(),
                &binary,
                &output,
                &mut peer,
                &mut vec![]
            )
            .is_err(),
            "{mode}"
        );
        assert_eq!(peer.peer.calls, 1);
        assert!(!output.join("result.json").exists(), "{mode}");
    }
}
#[test]
fn retired_helper_absence_is_explicit_but_present_bad_inputs_fail_before_child() {
    for mode in [
        "absent-helper",
        "changed-helper",
        "dangling-helper",
        "directory-helper",
        "missing-live",
    ] {
        let root = root_copy(false);
        let helper = root
            .path()
            .join("rust/tools/check_bluetooth_input_decode.py");
        fs::create_dir_all(helper.parent().unwrap()).unwrap();
        match mode {
            "changed-helper" => fs::write(&helper, b"changed").unwrap(),
            "dangling-helper" => symlink("missing-original", &helper).unwrap(),
            "directory-helper" => fs::create_dir(&helper).unwrap(),
            "missing-live" => fs::remove_file(
                root.path()
                    .join("openpilot/selfdrive/carrot/bluetooth/daemon.py"),
            )
            .unwrap(),
            _ => (),
        }
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("candidate");
        fs::write(&binary, b"fixture executable identity\n").unwrap();
        let output = dir.path().join("output");
        let mut peer = peer(|| {});
        let result = openpilot_dev_tools::bluetooth_input_check::check(
            root.path(),
            &binary,
            &output,
            &mut peer,
            &mut vec![],
        );
        assert_eq!(
            result.is_ok(),
            mode == "absent-helper",
            "{mode}: {result:?}"
        );
        assert_eq!(peer.peer.calls, usize::from(mode == "absent-helper"));
        assert_eq!(output.exists(), mode == "absent-helper");
    }
}
#[test]
fn actual_cli_keeps_raw_paths_help_and_argument_failure_boundaries() {
    let checker = env!("CARGO_BIN_EXE_check-bluetooth-input-decode");
    for args in [
        vec![],
        vec!["--binary", "missing"],
        vec!["--unknown"],
        vec!["--output", "x", "--binary"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let result = Command::new(checker)
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    assert!(Command::new(checker)
        .arg("--help")
        .output()
        .unwrap()
        .status
        .success());
    let fixture = fs::canonicalize(
        std::env::var_os("BLUETOOTH_INPUT_FIXTURE").expect("mandatory freshly built input_fixture"),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join(OsString::from_vec(b"binary-\xff".to_vec()));
    let output = dir.path().join(OsString::from_vec(b"output-\xfe".to_vec()));
    symlink(&fixture, &binary).unwrap();
    let result = Command::new(checker)
        .arg("--binary")
        .arg(&binary)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", repository())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout: Value = serde_json::from_slice(&result.stdout).unwrap();
    let saved: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(stdout, saved);
    assert_eq!(saved["cases"], 10_049);
}

#[cfg(target_os = "linux")]
#[test]
fn actual_cli_leader_cancellation_cleans_acknowledged_candidate_and_descendant() {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
    };
    use std::{
        os::fd::OwnedFd,
        process::{Child, Stdio},
        thread,
        time::Instant,
    };
    struct Checker {
        child: Child,
        identity: OwnedFd,
    }
    impl Drop for Checker {
        fn drop(&mut self) {
            let _ = pidfd_send_signal(&self.identity, Signal::INT);
            let end = Instant::now() + Duration::from_secs(2);
            while Instant::now() < end {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = pidfd_send_signal(&self.identity, Signal::KILL);
            let _ = self.child.wait();
        }
    }
    struct Children(Vec<OwnedFd>);
    impl Drop for Children {
        fn drop(&mut self) {
            // Failure cleanup only. The success assertions below run first and
            // signal only the checker leader, never the nested process group.
            for identity in &self.0 {
                let _ = pidfd_send_signal(identity, Signal::KILL);
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("candidate");
    let marker = dir.path().join("ack");
    let token = dir.path().file_name().unwrap().to_str().unwrap();
    fs::write(&script, b"#!/bin/sh\n/bin/sleep 20 &\nprintf '%s %s %s\\n' \"$BT_TEST_TOKEN\" \"$$\" \"$!\" > \"$BT_TEST_MARKER\"\nwait\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let output = dir.path().join("output");
    let child = Command::new(env!("CARGO_BIN_EXE_check-bluetooth-input-decode"))
        .arg("--binary")
        .arg(&script)
        .arg("--output")
        .arg(&output)
        .env("OPENPILOT_RUST_ROOT", repository())
        .env("BT_TEST_TOKEN", token)
        .env("BT_TEST_MARKER", &marker)
        .stdin(Stdio::null())
        .stdout(fs::File::create(dir.path().join("stdout")).unwrap())
        .stderr(fs::File::create(dir.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker { child, identity };
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut children = Children(vec![]);
    loop {
        if let Ok(text) = fs::read_to_string(&marker) {
            let fields = text.split_whitespace().collect::<Vec<_>>();
            if fields.len() == 3 && fields[0] == token {
                for field in &fields[1..] {
                    let pid = Pid::from_raw(field.parse().unwrap()).unwrap();
                    let identity = pidfd_open(pid, PidfdFlags::empty()).unwrap();
                    let environment =
                        fs::read(format!("/proc/{}/environ", pid.as_raw_pid())).unwrap();
                    assert!(environment
                        .split(|b| *b == 0)
                        .any(|v| v == format!("BT_TEST_TOKEN={token}").as_bytes()));
                    children.0.push(identity);
                }
                break;
            }
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before child acknowledgement"
        );
        assert!(
            Instant::now() < deadline,
            "owned candidate did not acknowledge startup"
        );
        thread::sleep(Duration::from_millis(5));
    }
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let exited = checker.child.try_wait().unwrap();
        let gone = children.0.iter().all(|id| {
            poll(
                &mut [PollFd::new(id, PollFlags::IN)],
                Some(&Timespec::default()),
            )
            .unwrap()
                == 1
        });
        if let Some(status) = exited {
            assert!(!status.success());
            if gone {
                break;
            }
        }
        assert!(
            Instant::now() < end,
            "leader cancellation left an acknowledged descendant alive"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(!output.join("result.json").exists());
}
