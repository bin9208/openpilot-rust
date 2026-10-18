//! Polling controllers must never observe the command helper's partial JSON.
use openpilot_dev_tools::{
    qa_process::CommandSpec, timed_fixtures::Fixture, timed_live_child::Child,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
#[test]
fn original_command_record_is_complete_at_first_visible_write() {
    let bytes = include_bytes!("fixtures/timed-tools/command-publication-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bd49f25cff5cf45ade3270f9b20661ecf251ce5cf05225b696bd590d8c8e7270"
    );
    let original: Value = serde_json::from_slice(bytes).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let fixture = Fixture::new(
        &directory.path().join("fixture"),
        "UTC",
        Path::new(env!("CARGO_BIN_EXE_timed-command-fixture")),
    )
    .unwrap();
    let mut args = vec![
        "/usr/bin/strace".into(),
        "-e".into(),
        "trace=write".into(),
        "-e".into(),
        "inject=write:delay_exit=1000000".into(),
        env!("CARGO_BIN_EXE_timed-command-fixture").into(),
    ];
    args.extend(
        original["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap().into()),
    );
    let mut spec = CommandSpec::new_os(args, Duration::from_secs(5));
    fixture.configure(&mut spec);
    let mut child = Child::spawn(&spec, &directory.path().join("trace.log")).unwrap();
    let file = fixture.directory.join("commands.jsonl");
    let end = Instant::now() + Duration::from_secs(5);
    while !file.exists() || fs::metadata(&file).unwrap().len() == 0 {
        assert!(Instant::now() < end, "command not published");
        std::thread::sleep(Duration::from_millis(2));
    }
    let observed = fs::read(&file)
        .unwrap()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(observed, original["first_nonempty_hex"].as_str().unwrap());
    assert_eq!(child.wait(Duration::from_secs(5)).unwrap(), 0);
    assert_eq!(
        fs::read(file)
            .unwrap()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        original["final_hex"].as_str().unwrap()
    );
}
