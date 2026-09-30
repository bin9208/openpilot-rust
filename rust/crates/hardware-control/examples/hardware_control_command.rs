use openpilot_hardware_control::{Command, Commands, ProcessCommands};
use serde_json::json;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let launcher = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected process-child launcher")?,
    );
    let mut commands = ProcessCommands { launcher };
    let output = commands.run(&Command::Output(vec![
        "/bin/sh".into(),
        "-c".into(),
        "printf inherited-stderr >&2; head -c 131072 /dev/zero; exit 7".into(),
    ]))?;
    let call = commands.run(&Command::Call(vec![
        "/bin/sh".into(),
        "-c".into(),
        "exit 3".into(),
    ]))?;
    let shell = commands.run(&Command::Shell("exit 5".into()))?;
    let missing = commands.run(&Command::Output(vec![
        "/fixture/nonexistent-hardware-command".into(),
    ]));
    println!(
        "{}",
        json!({"status":output.status, "stdout_bytes":output.stdout.len(), "stdout_zero":output.stdout.iter().all(|byte| *byte == 0), "call_status":call.status, "shell_status":shell.status, "missing_is_io":matches!(missing, Err(openpilot_hardware_control::Error::Io { .. }))})
    );
    Ok(())
}
