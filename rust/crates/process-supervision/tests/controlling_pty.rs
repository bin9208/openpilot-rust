use openpilot_process_supervision::{pty::Pair, CapturedCommand};
use std::{io::Read, path::PathBuf};
fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--fixture")) {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let stderr = std::io::stderr();
        println!(
            "{}",
            serde_json::json!({
                "pid":std::process::id(),
                "sid":rustix::process::getsid(None).unwrap().as_raw_nonzero().get(),
                "pgid":rustix::process::getpgrp().as_raw_nonzero().get(),
                "foreground":rustix::termios::tcgetpgrp(&stdin).ok().map(|pid|pid.as_raw_nonzero().get()),
                "stdio":[rustix::termios::isatty(&stdin),rustix::termios::isatty(&stdout),rustix::termios::isatty(&stderr)]
            })
        );
        return;
    }
    controlling_terminal_is_additive_and_web_session_remains_uncontrolled();
}

fn controlling_terminal_is_additive_and_web_session_remains_uncontrolled() {
    let root = tempfile::tempdir().unwrap();
    let mut observed = Vec::new();
    for controlled in [false, true] {
        let Pair { mut master, slave } = Pair::open(24, 80).unwrap();
        let request = CapturedCommand {
            launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
            cwd: root.path().into(),
            argv: vec![
                std::env::current_exe().unwrap().into_os_string(),
                "--fixture".into(),
            ],
        };
        let mut child = if controlled {
            request.spawn_controlling_pty(slave)
        } else {
            request.spawn_session_pty_with_env(slave, &[])
        }
        .unwrap();
        let mut output = Vec::new();
        loop {
            let mut packet = [0; 4096];
            match master.read(&mut packet) {
                Ok(0) => break,
                Ok(length) => output.extend(&packet[..length]),
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::IO.raw_os_error()) =>
                {
                    break
                }
                Err(error) => panic!("{error}"),
            }
        }
        assert!(child.process.wait().unwrap().success());
        let row: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(row["sid"], row["pid"]);
        assert_eq!(row["pgid"], row["pid"]);
        assert_eq!(row["stdio"], serde_json::json!([true, true, true]));
        assert_eq!(
            row["foreground"],
            if controlled {
                row["pid"].clone()
            } else {
                serde_json::Value::Null
            }
        );
        observed.push(serde_json::json!({"controlling":controlled,"child":row,"reaped":true}));
    }
    let evidence = std::env::var_os("CI_JOB_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.path().to_path_buf())
        .join("test-evidence/269-runtime-review-fixes/controlling-pty.json");
    std::fs::create_dir_all(evidence.parent().unwrap()).unwrap();
    std::fs::write(evidence, serde_json::to_vec_pretty(&observed).unwrap()).unwrap();
    println!("{}", serde_json::json!(observed));
}
