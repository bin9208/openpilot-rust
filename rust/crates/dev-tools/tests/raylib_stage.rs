use std::process::Command;

#[test]
fn help_succeeds_without_required_paths_or_external_programs() {
    for flag in ["--help", "-h"] {
        let result = Command::new(env!("CARGO_BIN_EXE_stage-startup-ui-raylib"))
            .arg(flag)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        for option in [
            "--arch",
            "--output",
            "--shared",
            "--cxx",
            "--wheel-file",
            "--library-directory",
        ] {
            assert!(text.contains(option), "missing {option}");
        }
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn nonregular_lock_nodes_fail_promptly_without_output_or_temporary_trees() {
    use std::{
        fs,
        io::Read,
        os::unix::fs::{symlink, OpenOptionsExt},
        process::{Child, Stdio},
        time::{Duration, Instant},
    };
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for kind in ["fifo", "fifo-reader", "directory", "symlink"] {
        let temp = tempfile::tempdir().unwrap();
        let lock = temp.path().join(".startup-ui-raylib.lock");
        match kind {
            "fifo" | "fifo-reader" => rustix::fs::mkfifoat(
                rustix::fs::CWD,
                &lock,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )
            .unwrap(),
            "directory" => fs::create_dir(&lock).unwrap(),
            _ => {
                fs::write(temp.path().join("kept"), b"untouched").unwrap();
                symlink("kept", &lock).unwrap();
            }
        }
        let _reader = if kind == "fifo-reader" {
            Some(
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(i32::try_from(rustix::fs::OFlags::NONBLOCK.bits()).unwrap())
                    .open(&lock)
                    .unwrap(),
            )
        } else {
            None
        };
        let output = temp.path().join("output");
        let mut child = Owned(
            Command::new(env!("CARGO_BIN_EXE_stage-startup-ui-raylib"))
                .args(["--arch", "x86_64", "--output"])
                .arg(&output)
                .arg("--wheel-file")
                .arg(temp.path().join("missing.whl"))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "{kind}: lock admission blocked");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(!status.success(), "{kind}");
        let mut error = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut error)
            .unwrap();
        if kind == "fifo-reader" {
            assert!(error.contains("regular file"), "{error}");
        }
        assert!(!output.exists());
        assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".raylib-")));
        if kind == "symlink" {
            assert_eq!(fs::read(temp.path().join("kept")).unwrap(), b"untouched");
        }
    }
}
