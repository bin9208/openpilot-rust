use super::*;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::{fcntl_getfl, fcntl_setfl, OFlags},
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags, Signal},
};
use std::{
    io::{ErrorKind, PipeReader, Read, Write},
    os::fd::OwnedFd,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Checker {
    child: Child,
    identity: OwnedFd,
    stdout: PipeReader,
    descendants: Vec<(u32, OwnedFd)>,
}
impl Checker {
    fn drain(&mut self) {
        let mut bytes = [0; 8192];
        loop {
            match self.stdout.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) => panic!("stdout read failed: {error}"),
            }
        }
    }
    fn observe_descendants(&mut self, token: &str) {
        let mut parents = vec![self.child.id()];
        while let Some(parent) = parents.pop() {
            let children = fs::read_to_string(format!("/proc/{parent}/task/{parent}/children"))
                .unwrap_or_default();
            for child in children.split_whitespace() {
                let pid: u32 = child.parse().unwrap();
                parents.push(pid);
                if self.descendants.iter().any(|(known, _)| *known == pid) {
                    continue;
                }
                let Some(pid_value) = Pid::from_raw(pid as i32) else {
                    continue;
                };
                let Ok(identity) = pidfd_open(pid_value, PidfdFlags::empty()) else {
                    continue;
                };
                // Only retain identities acknowledged through this private
                // invocation's inherited environment, never an unowned PID.
                let env = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
                if env
                    .split(|byte| *byte == 0)
                    .any(|value| value == token.as_bytes())
                {
                    self.descendants.push((pid, identity));
                }
            }
        }
    }
}
impl Drop for Checker {
    fn drop(&mut self) {
        let _ = pidfd_send_signal(&self.identity, Signal::INT);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            self.drain();
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let _ = pidfd_send_signal(&self.identity, Signal::KILL);
        for (_, identity) in &self.descendants {
            let _ = pidfd_send_signal(identity, Signal::KILL);
        }
        let _ = self.child.wait();
    }
}

#[test]
fn final_stdout_sigint_after_five_worker_results_cannot_publish_success() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(&temp.path().join("output"));
    let stderr = temp.path().join("stderr");
    let (reader, mut writer) = std::io::pipe().unwrap();
    let flags = fcntl_getfl(&writer).unwrap();
    fcntl_setfl(&writer, flags | OFlags::NONBLOCK).unwrap();
    // A full real stdout pipe selects the final write boundary without a
    // production hook, guessed startup delay, or preexisting root receipt.
    loop {
        match writer.write(&[b'x'; 4096]) {
            Ok(count) => assert!(count > 0),
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => panic!("stdout prefill failed: {error}"),
        }
    }
    fcntl_setfl(&writer, flags).unwrap();
    let flags = fcntl_getfl(&reader).unwrap();
    fcntl_setfl(&reader, flags | OFlags::NONBLOCK).unwrap();
    let value = temp.path().file_name().unwrap().to_str().unwrap();
    let token = format!("STATS_INTERRUPTED_TEST_TOKEN={value}");
    let child = Command::new(worker())
        .args([
            &options.binary,
            &options.binding,
            &options.preload,
            &options.output,
        ])
        .env("OPENPILOT_RUST_ROOT", &options.root)
        .env("STATS_INTERRUPTED_TEST_TOKEN", value)
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let identity = pidfd_open(Pid::from_child(&child), PidfdFlags::empty()).unwrap();
    let mut checker = Checker {
        child,
        identity,
        stdout: reader,
        descendants: vec![],
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        checker.observe_descendants(&token);
        let all_results = rows().iter().all(|original| {
            let path = options
                .output
                .join(original["case"].as_str().unwrap())
                .join("native/result.json");
            fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|result| result == original["result"])
        });
        let state =
            fs::read_to_string(format!("/proc/{}/wchan", checker.child.id())).unwrap_or_default();
        if all_results && state.contains("pipe_write") {
            break;
        }
        assert!(
            checker.child.try_wait().unwrap().is_none(),
            "checker exited before final stdout: {}",
            fs::read_to_string(&stderr).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "all five results/final stdout not acknowledged: {state}; {}",
            fs::read_to_string(&stderr).unwrap()
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !checker.descendants.is_empty(),
        "no owned worker identity was acknowledged"
    );
    pidfd_send_signal(&checker.identity, Signal::INT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        checker.drain();
        if let Some(status) = checker.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "late-cancel checker did not exit"
        );
        thread::sleep(Duration::from_millis(5));
    };
    assert!(!status.success(), "late SIGINT was reported as success");
    assert!(
        !options.output.join("result.json").exists(),
        "late SIGINT left a root success receipt"
    );
    for (_, identity) in &checker.descendants {
        assert_eq!(
            poll(
                &mut [PollFd::new(identity, PollFlags::IN)],
                Some(&Timespec::default())
            )
            .unwrap(),
            1,
            "acknowledged descendant survived checker exit"
        );
    }
}

#[test]
fn original_cli_help_usage_and_before_position_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let options = options(&temp.path().join("unused"));
    let invoke = |args: &[&std::ffi::OsStr]| {
        Command::new(worker())
            .args(args)
            .env("OPENPILOT_RUST_ROOT", &options.root)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let help = invoke(&["--help".as_ref()]);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("--before"));
    let missing = temp.path().join("missing-daemon");
    let arguments = [
        missing.as_os_str(),
        options.binding.as_os_str(),
        options.preload.as_os_str(),
    ];
    let malformed_output = temp.path().join("malformed");
    let mut extra = arguments.to_vec();
    extra.extend([malformed_output.as_os_str(), "extra".as_ref()]);
    for args in [
        vec![],
        arguments.to_vec(),
        vec!["--unknown".as_ref()],
        vec!["--before".as_ref()],
        extra,
    ] {
        let result = invoke(&args);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    for position in [0, 2, 4] {
        let output = temp.path().join(format!("before-{position}"));
        let mut args = arguments.to_vec();
        args.push(output.as_os_str());
        args.insert(position, "--before".as_ref());
        let result = invoke(&args);
        assert_eq!(
            result.status.code(),
            Some(1),
            "valid --before position {position} must reach missing-file admission: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("missing-daemon"),
            "valid arguments must reach the actual missing-candidate open"
        );
        assert!(!output.join("result.json").exists());
    }
}
