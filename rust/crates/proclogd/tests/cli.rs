use openpilot_cereal::log_capnp::event;
use std::{
    fs,
    io::Read,
    process::{Command, Stdio},
};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_openpilot-proclogd"))
}

#[test]
fn help_invalid_arguments_and_production_namespace_refusal() {
    let help = command().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("isolated"));
    for args in [
        vec![],
        vec!["--unknown"],
        vec!["--stdout", "--frames", "0"],
        vec!["--stdout", "--frames", "301"],
        vec!["--stdout", "--interval-ms", "-1"],
        vec!["--publish"],
    ] {
        assert!(!command()
            .args(args)
            .env_remove("OPENPILOT_PREFIX")
            .output()
            .unwrap()
            .status
            .success());
    }
}

#[test]
fn bounded_file_output_contains_canonical_messages_and_never_overwrites() {
    let output = tempfile::tempdir().unwrap();
    let result = command()
        .args(["--frames", "2", "--interval-ms", "0", "--output-dir"])
        .arg(output.path())
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    let files: Vec<_> = fs::read_dir(output.path()).unwrap().collect();
    assert_eq!(files.len(), 2);
    for index in 0..2 {
        let bytes = fs::read(output.path().join(format!("procLog-{index:04}.capnp"))).unwrap();
        let reader = capnp::serialize::read_message(bytes.as_slice(), Default::default()).unwrap();
        let message = reader.get_root::<event::Reader>().unwrap();
        assert!(message.get_log_mono_time() > 0);
        assert!(message.get_valid());
        let event::ProcLog(log) = message.which().unwrap() else {
            panic!("wrong event");
        };
        assert!(!log.unwrap().get_procs().unwrap().is_empty());
    }
    assert!(!command()
        .args(["--frames", "1", "--output-dir"])
        .arg(output.path())
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn closed_pipe_exits_cleanly() {
    let mut child = command()
        .args(["--stdout", "--frames", "10", "--interval-ms", "10"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut first = [0; 8];
    output.read_exact(&mut first).unwrap();
    drop(output);
    assert!(child.wait().unwrap().success());
}

#[test]
fn isolated_self_test_exercises_ipc_and_temporary_params() {
    let directory = tempfile::Builder::new()
        .prefix("msgq_rust-probe-")
        .tempdir_in("/dev/shm")
        .unwrap();
    let prefix = directory
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("msgq_")
        .unwrap();
    let output = command()
        .arg("--self-test")
        .env("OPENPILOT_PREFIX", prefix)
        .env_remove("CEREAL_FAKE")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS"));
}
