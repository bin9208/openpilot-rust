use openpilot_process_supervision::{pty::Pair, CapturedCommand};
use std::{io::Read, path::Path};
#[test]
fn controlling_terminal_is_additive_and_web_session_remains_uncontrolled() {
    let root = tempfile::tempdir().unwrap();
    let mut observed = Vec::new();
    for controlled in [false, true] {
        let Pair { mut master, slave } = Pair::open(24, 80).unwrap();
        let request = CapturedCommand {
            launcher: env!("CARGO_BIN_EXE_openpilot-process-child").into(),
            cwd: root.path().into(),
            argv: ["/usr/bin/python3", "-c", "import json,os; p=os.getpid(); c=None\ntry: c=os.tcgetpgrp(0)\nexcept OSError: pass\nprint(json.dumps(dict(pid=p,sid=os.getsid(0),pgid=os.getpgrp(),foreground=c,stdio=[os.isatty(i) for i in range(3)])))"].iter().map(Into::into).collect(),
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
    let evidence = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.omo/evidence/269-runtime-review-fixes/controlling-pty.json");
    std::fs::create_dir_all(evidence.parent().unwrap()).unwrap();
    std::fs::write(evidence, serde_json::to_vec_pretty(&observed).unwrap()).unwrap();
    println!("{}", serde_json::json!(observed));
}
