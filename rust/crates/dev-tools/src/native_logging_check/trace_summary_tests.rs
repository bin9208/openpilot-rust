use super::*;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

fn fixture(raw: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let directory = temp.path().join("raw");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(directory.join("case.21"), raw).unwrap();
    let output = temp.path().join("summary.json");
    (temp, directory, output)
}

const EXEC: &[u8] =
    b"1.000001 execve(\"/bin/tool\", [\"tool\"], []) = 0\n1.000002 +++ exited with 0 +++\n";

#[test]
fn summary_keeps_every_exec_signal_terminal_and_all_event_counts() {
    let raw = b"1.000001 execve(\"/x/python\", [\"python\"], [\"LANG=C\"]) = -1 ENOENT (No such file or directory)\n1.000002 newfstatat(AT_FDCWD, \"/x\", {st_mode=S_IFREG|0600, st_size=0}, 0) = 0\n1.000003 --- SIGUSR1 {si_signo=SIGUSR1, si_code=SI_TKILL, si_pid=21, si_uid=1001} ---\n1.000004 execveat(3, \"\", [\"x\"], [], AT_EMPTY_PATH) = 0\n1.000005 +++ exited with 1 +++\n";
    let (_temp, directory, output) = fixture(raw);
    inspect_mode(&directory, "case", &output, true).unwrap();
    let report: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(report["schema"], 2);
    assert_eq!(report["full_event_index"], false);
    assert_eq!(report["lexically_checked_all_events"], true);
    assert_eq!(report["event_count"], 5);
    assert_eq!(report["physical_lines"], 5);
    assert_eq!(report["events"].as_array().unwrap().len(), 4);
    assert_eq!(
        report["events"][0]["exec"]["program"]["bytes"],
        json!(b"/x/python")
    );
    assert_eq!(report["events"][0]["outcome"], "failed");
    assert_eq!(report["events"][1]["kind"], "signal");
    assert_eq!(report["events"][2]["operation"], "execveat");
    assert_eq!(
        report["events"][3]["result"],
        json!(b"+++ exited with 1 +++")
    );
    assert_eq!(report["event_counts"]["syscall:newfstatat:returned"], 1);
    assert_eq!(
        report["raw_files"][0]["sha256"],
        format!("{:x}", Sha256::digest(raw))
    );
    for flag in [
        "closed_pid",
        "closed_cwd",
        "closed_fd",
        "closed_dso",
        "full_acceptance",
        "retirement_credit",
    ] {
        assert_eq!(report[flag], false);
    }
}

#[test]
fn summary_rejects_malformed_unretained_calls_resumes_and_post_terminal_rows() {
    for (raw, expected) in [
        (b"1.000001 newfstatat(AT_FDCWD, \"\\q\", {}, 0) = 0\n1.000002 +++ exited with 0 +++\n".as_slice(), "trace C-string invalid"),
        (b"1.000001 openat(AT_FDCWD, \"/x\", O_RDONLY <unfinished ...>\n1.000002 <... newfstatat resumed>) = 3\n1.000003 +++ exited with 0 +++\n".as_slice(), "trace resumed syscall differs"),
        (b"1.000001 +++ exited with 0 +++\n1.000002 newfstatat(AT_FDCWD, \"/x\", {}, 0) = 0\n".as_slice(), "trace record after terminal"),
    ] {
        let (_temp, directory, output) = fixture(raw);
        assert_eq!(inspect_mode(&directory, "case", &output, true).unwrap_err().to_string(), expected);
        assert!(!output.exists());
    }
}

#[test]
fn summary_validates_many_metadata_rows_without_exporting_their_details() {
    let mut raw = b"1.000000 execve(\"/bin/tool\", [\"tool\"], []) = 0\n".to_vec();
    for index in 1..=2000 {
        raw.extend_from_slice(format!("1.{index:06} newfstatat(AT_FDCWD, \"/x\", {{st_mode=S_IFREG|0600, st_size=0}}, 0) = 0\n").as_bytes());
    }
    raw.extend_from_slice(b"2.000000 +++ exited with 0 +++\n");
    let (_temp, directory, output) = fixture(&raw);
    inspect_mode(&directory, "case", &output, true).unwrap();
    let bytes = fs::read(output).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["event_count"], 2002);
    assert_eq!(report["physical_lines"], 2002);
    assert_eq!(report["events"].as_array().unwrap().len(), 2);
    assert_eq!(report["event_counts"]["syscall:newfstatat:returned"], 2000);
    assert_eq!(fs::read(directory.join("case.21")).unwrap(), raw);
    assert!(bytes.len() < 16 * 1024);
}

#[test]
fn summary_shares_held_raw_hash_and_identity_rejection() {
    let (_temp, directory, output) = fixture(EXEC);
    let mut input = Inputs::open(&directory, "case").unwrap();
    let mut summary = EventSummary::default();
    let mut lines = 0;
    for file in &mut input.files {
        parse_file_emit(file, &mut lines, &mut |row| summary.observe(row)).unwrap();
    }
    input.verify().unwrap();
    let mut changed = EXEC.to_vec();
    let index = changed.windows(4).position(|v| v == b"tool").unwrap();
    changed[index] = b'T';
    fs::write(directory.join("case.21"), changed).unwrap();
    assert!(input.verify().is_err());
    assert!(!output.exists());
}

#[test]
fn summary_preserves_event_retained_model_and_key_bounds() {
    let mut summary = EventSummary {
        event_count: MAX_EVENTS,
        ..Default::default()
    };
    assert_eq!(
        summary
            .observe(event(21, "terminal_exit", Vec::new()))
            .unwrap_err()
            .to_string(),
        "trace event count exceeds bound"
    );
    let mut summary = EventSummary {
        budget: MAX_MODEL_BYTES,
        ..Default::default()
    };
    assert_eq!(
        summary
            .observe(event(21, "terminal_exit", Vec::new()))
            .unwrap_err()
            .to_string(),
        "trace model exceeds bound"
    );
    let mut summary = EventSummary::default();
    for index in 0..1024 {
        let mut row = event(21, "syscall", Vec::new());
        row.operation = Some(format!("op_{index}"));
        summary.observe(row).unwrap();
    }
    let mut row = event(21, "syscall", Vec::new());
    row.operation = Some("another".into());
    assert_eq!(
        summary.observe(row).unwrap_err().to_string(),
        "trace summary key count exceeds bound"
    );
}

#[test]
fn summary_cli_is_explicit_and_full_report_contract_remains_schema_one() {
    let (_temp, directory, output) = fixture(EXEC);
    inspect(&directory, "case", &output).unwrap();
    let full: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(full["schema"], 1);
    assert!(full.get("full_event_index").is_none());
    let (_temp, directory, output) = fixture(EXEC);
    let args = [
        OsString::from("inspect-trace-summary"),
        OsString::from("--directory"),
        directory.into_os_string(),
        OsString::from("--prefix"),
        OsString::from("case"),
        OsString::from("--output"),
        output.clone().into_os_string(),
    ];
    assert!(selected(&args));
    cli_selected(&args).unwrap();
    let summary: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(summary["schema"], 2);
    assert!(!selected(&[OsString::from("inspect-trace-summary-extra")]));
}

#[test]
fn summary_keeps_successful_and_failed_process_session_details_with_exact_counts() {
    let calls = [
        ("fork", "fork() = 22"),
        ("vfork", "vfork() = 23"),
        ("clone", "clone(child_stack=NULL, flags=SIGCHLD) = 24"),
        (
            "clone3",
            "clone3({flags=CLONE_VM|CLONE_VFORK, exit_signal=SIGCHLD}, 88) = 25",
        ),
        ("setpgid", "setpgid(0, 0) = 0"),
        ("setsid", "setsid() = 21"),
        ("unshare", "unshare(CLONE_NEWNS) = 0"),
        ("setns", "setns(3, CLONE_NEWNS) = 0"),
    ];
    let mut raw = b"1.000000 execve(\"/bin/tool\", [\"tool\"], []) = 0\n".to_vec();
    for (index, (_, call)) in calls.iter().enumerate() {
        raw.extend_from_slice(format!("1.{:06} {call}\n", index * 2 + 1).as_bytes());
        let (args, _) = call.rsplit_once(" = ").unwrap();
        raw.extend_from_slice(
            format!(
                "1.{:06} {args} = -1 EPERM (Operation not permitted)\n",
                index * 2 + 2
            )
            .as_bytes(),
        );
    }
    raw.extend_from_slice(b"2.000001 read(3, \"abc\", 3) = 3\n2.000002 wait4(22, NULL, WNOHANG, NULL) = 0\n2.000003 +++ exited with 0 +++\n");
    let (_temp, directory, output) = fixture(&raw);
    inspect_mode(&directory, "case", &output, true).unwrap();
    let report: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(
        report["detailed_operations"],
        json!(SUMMARY_DETAIL_OPERATIONS)
    );
    assert_eq!(report["event_count"], 20);
    assert_eq!(report["physical_lines"], 20);
    assert_eq!(report["events"].as_array().unwrap().len(), 18);
    for (operation, call) in calls {
        let rows = report["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["operation"] == operation)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["outcome"], "returned");
        assert_eq!(rows[1]["outcome"], "failed");
        assert_eq!(
            rows[0]["result"],
            json!(call.rsplit_once(" = ").unwrap().1.as_bytes())
        );
        assert_eq!(
            rows[1]["result"],
            json!(b"-1 EPERM (Operation not permitted)".as_slice())
        );
        for outcome in ["returned", "failed"] {
            assert_eq!(
                report["event_counts"][format!("syscall:{operation}:{outcome}")],
                1
            );
        }
        assert!(rows
            .iter()
            .all(|row| row["source"].as_array().unwrap().len() == 1));
    }
    assert_eq!(report["event_counts"]["syscall:read:returned"], 1);
    assert_eq!(report["event_counts"]["syscall:wait4:returned"], 1);
    assert_eq!(report["full_event_index"], false);
    assert_eq!(report["lexically_checked_all_events"], true);
    for flag in [
        "closed_pid",
        "closed_cwd",
        "closed_fd",
        "closed_dso",
        "full_acceptance",
        "retirement_credit",
    ] {
        assert_eq!(report[flag], false);
    }
    assert_eq!(fs::read(directory.join("case.21")).unwrap(), raw);
}
