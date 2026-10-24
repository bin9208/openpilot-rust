use super::*;
use serde_json::{json, Value};
use std::os::unix::fs::{symlink, PermissionsExt};

struct Fixture {
    _temp: tempfile::TempDir,
    directory: PathBuf,
    output: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let directory = temp.path().join("trace");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let output = temp.path().join("inspection.json");
        Self {
            _temp: temp,
            directory,
            output,
        }
    }
    fn put(&self, pid: &str, bytes: &[u8]) {
        fs::write(self.directory.join(format!("case.{pid}")), bytes).unwrap();
    }
    fn inspect(&self) -> Result<()> {
        inspect(&self.directory, "case", &self.output)
    }
    fn error(&self, expected: &str) {
        assert_eq!(self.inspect().unwrap_err().to_string(), expected);
        assert!(!self.output.exists());
    }
    fn report(&self) -> Value {
        self.inspect().unwrap();
        serde_json::from_slice(&fs::read(&self.output).unwrap()).unwrap()
    }
}
const PLAIN: &[u8] = b"1.000001 execve(\"/bin/tool\", [\"tool\", \"a\\n\\377\"], [\"LANG=C\"]) = 0\n1.000002 +++ exited with 0 +++\n";

#[test]
fn plain_exec_bytes_environment_and_raw_spans_remain_inspection_only() {
    let f = Fixture::new();
    f.put("21", PLAIN);
    let report = f.report();
    let exec = &report["events"][0]["exec"];
    assert_eq!(exec["program"]["bytes"], json!(b"/bin/tool"));
    assert_eq!(exec["argv"][1]["bytes"], json!([97, 10, 255]));
    assert_eq!(exec["environment"]["entries"][0]["bytes"], json!(b"LANG=C"));
    let range = &exec["program"]["source"];
    let body = &PLAIN[b"1.000001 ".len()..];
    assert_eq!(
        &body[range["start"].as_u64().unwrap() as usize..range["end"].as_u64().unwrap() as usize],
        b"\"/bin/tool\""
    );
    assert_eq!(report["raw_immutable"], true);
    for field in [
        "closed_pid",
        "closed_cwd",
        "closed_fd",
        "closed_dso",
        "full_acceptance",
    ] {
        assert_eq!(report[field], false);
    }
}

#[test]
fn hex_exec_failed_attempt_and_execveat_are_data_without_policy_acceptance() {
    let f = Fixture::new();
    f.put("21", b"1.000001 execve(\"\\x2f\\x78\", [\"\\x78\"], [])             = -1 ENOENT (No such file or directory)\n1.000002 execveat(3</tmp/tool>, \"\", [\"x\"], NULL, AT_EMPTY_PATH) = 0\n1.000003 +++ exited with 0 +++\n");
    let report = f.report();
    assert_eq!(report["events"][0]["outcome"], "failed");
    assert_eq!(
        report["events"][0]["exec"]["program"]["bytes"],
        json!(b"/x")
    );
    assert_eq!(report["events"][1]["operation"], "execveat");
    assert_eq!(report["full_acceptance"], false);
}

#[test]
fn memory_network_session_calls_and_opaque_payload_are_lexical_events() {
    let f = Fixture::new();
    f.put("21", b"1.000001 mmap(NULL, 4096, PROT_READ|PROT_WRITE, MAP_PRIVATE, 3</tmp/a>, 0) = 0x7f123000\n1.000002 sendto(4<UNIX:[12->13]>, \"abc\"..., 4096, MSG_DONTWAIT, NULL, 0) = 3\n1.000003 setsid() = 21\n1.000004 prctl(PR_SET_CHILD_SUBREAPER, 1) = 0\n1.000005 clone3({flags=CLONE_VM|CLONE_FILES, exit_signal=SIGCHLD} => {parent_tid=[22]}, 88) = 22\n1.000006 +++ exited with 0 +++\n");
    let report = f.report();
    assert_eq!(report["events"][1]["opaque_payload"], true);
    assert_eq!(report["events"][0]["operation"], "mmap");
    assert_eq!(report["closed_fd"], false);
}

#[test]
fn same_pid_resume_preserves_both_physical_fragments() {
    let f = Fixture::new();
    f.put("21", b"1.000001 execve(\"/x\", [\"x\"], [] <unfinished ...>\n1.000002 <... execve resumed>) = 0\n1.000003 +++ exited with 0 +++\n");
    let report = f.report();
    let event = &report["events"][0];
    assert_eq!(event["source"].as_array().unwrap().len(), 2);
    assert_eq!(event["source"][0]["line"], 1);
    assert_eq!(event["source"][1]["line"], 2);
    assert_eq!(event["exec"]["program"]["bytes"], json!(b"/x"));
}

#[test]
fn terminal_signal_keeps_interrupted_call_aborted_and_never_successful() {
    let f = Fixture::new();
    f.put("21", b"1.000001 read(3<pipe:[12]>,  <unfinished ...>\n1.000002 --- stopped by SIGSTOP ---\n1.000003 --- SIGTERM {si_signo=SIGTERM, si_code=SI_USER, si_pid=9, si_uid=1001} ---\n1.000004 +++ killed by SIGTERM +++\n");
    let report = f.report();
    let aborted = report["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "aborted")
        .unwrap();
    assert_eq!(aborted["outcome"], "aborted");
    assert_eq!(aborted["aborted_by"], "SIGTERM");
    assert!(aborted["exec"].is_null());
}

#[test]
fn malformed_timestamp_or_clock_reversal_is_rejected() {
    for raw in [
        b"1.123 execve(\"/x\", [], []) = 0\n".as_slice(),
        b"2.000001 getpid() = 21\n1.000001 +++ exited with 0 +++\n",
    ] {
        let f = Fixture::new();
        f.put("21", raw);
        f.error("trace timestamp invalid");
    }
}

#[test]
fn malformed_cstrings_unknown_escapes_and_exec_ellipsis_are_rejected() {
    for body in [
        "execve(\"/x\\q\", [], []) = 0",
        "execve(\"/x\", [\"x\"...], []) = 0",
        "execve(\"/x\", [], [\"A=B\"...]) = 0",
        "execve(\"/x\", [\"unterminated], []) = 0",
    ] {
        let f = Fixture::new();
        f.put(
            "21",
            format!("1.000001 {body}\n1.000002 +++ exited with 0 +++\n").as_bytes(),
        );
        assert!(f.inspect().is_err(), "{body}");
        assert!(!f.output.exists());
    }
}

#[test]
fn orphan_wrong_operation_and_cross_pid_resumes_are_rejected() {
    for other in ["read", "execve"] {
        let f = Fixture::new();
        f.put(
            "21",
            b"1.000001 execve(\"/x\", [], [] <unfinished ...>\n1.000003 +++ killed by SIGTERM +++\n",
        );
        f.put(
            "22",
            format!("1.000002 <... {other} resumed>) = 0\n1.000003 +++ exited with 0 +++\n")
                .as_bytes(),
        );
        f.error("trace orphan resumed syscall");
    }
    let f = Fixture::new();
    f.put("21",b"1.000001 read(3,  <unfinished ...>\n1.000002 <... write resumed>\"x\", 1) = 1\n1.000003 +++ exited with 0 +++\n");
    f.error("trace resumed syscall differs");
}

#[test]
fn missing_newline_terminal_or_unresolved_pending_is_rejected() {
    for raw in [
        b"1.000001 getpid() = 21".as_slice(),
        b"1.000001 getpid() = 21\n",
        b"1.000001 read(3,  <unfinished ...>\n",
        b"1.000001 read(3,  <unfinished ...>\n1.000002 +++ exited with 0 +++\n",
    ] {
        let f = Fixture::new();
        f.put("21", raw);
        assert!(f.inspect().is_err());
        assert!(!f.output.exists());
    }
}

#[test]
fn malformed_delimiters_nested_pending_and_post_terminal_calls_are_rejected() {
    for raw in [
        b"1.000001 mmap({a=[1)}, 0) = 0\n".as_slice(),
        b"1.000001 read(3,  <unfinished ...>\n1.000002 read(4, <unfinished ...>\n",
        b"1.000001 +++ exited with 0 +++\n1.000002 getpid() = 21\n",
    ] {
        let f = Fixture::new();
        f.put("21", raw);
        assert!(f.inspect().is_err());
        assert!(!f.output.exists());
    }
}

#[test]
fn duplicate_pid_spelling_and_unsafe_prefix_are_rejected() {
    let f = Fixture::new();
    f.put("21", PLAIN);
    f.put("021", PLAIN);
    f.error("duplicate trace PID");
    let f = Fixture::new();
    f.put("21", PLAIN);
    assert_eq!(
        inspect(&f.directory, "../case", &f.output)
            .unwrap_err()
            .to_string(),
        "trace prefix invalid"
    );
}

#[test]
fn symlink_hardlink_fifo_and_public_directory_are_rejected() {
    let f = Fixture::new();
    f.put("21", PLAIN);
    symlink(f.directory.join("case.21"), f.directory.join("case.22")).unwrap();
    f.error("trace roster entry unsafe");
    let f = Fixture::new();
    f.put("21", PLAIN);
    fs::hard_link(f.directory.join("case.21"), f.directory.join("case.22")).unwrap();
    f.error("trace roster entry unsafe");
    let f = Fixture::new();
    rustix::fs::mknodat(
        rustix::fs::CWD,
        f.directory.join("case.21"),
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR,
        0,
    )
    .unwrap();
    f.error("trace roster entry unsafe");
    let f = Fixture::new();
    f.put("21", PLAIN);
    fs::set_permissions(&f.directory, fs::Permissions::from_mode(0o755)).unwrap();
    f.error("trace directory must be owned private");
}

#[test]
fn ancestor_symlink_and_existing_output_cannot_redirect_or_overwrite() {
    let f = Fixture::new();
    f.put("21", PLAIN);
    let alias = f._temp.path().join("alias");
    symlink(&f.directory, &alias).unwrap();
    assert!(inspect(&alias, "case", &f.output).is_err());
    fs::write(&f.output, b"foreign output").unwrap();
    assert!(f.inspect().is_err());
    assert_eq!(fs::read(&f.output).unwrap(), b"foreign output");
}

#[test]
fn held_inputs_detect_same_length_rewrite_replacement_and_added_roster() {
    for action in 0..3 {
        let f = Fixture::new();
        f.put("21", PLAIN);
        let mut input = Inputs::open(&f.directory, "case").unwrap();
        input.events().unwrap();
        match action {
            0 => {
                let mut changed = PLAIN.to_vec();
                let at = changed.windows(4).position(|w| w == b"tool").unwrap();
                changed[at..at + 4].copy_from_slice(b"evil");
                fs::write(f.directory.join("case.21"), changed).unwrap();
            }
            1 => {
                fs::rename(f.directory.join("case.21"), f.directory.join("old")).unwrap();
                f.put("21", PLAIN);
            }
            _ => f.put("22", PLAIN),
        }
        assert!(input.verify().is_err());
    }
}

#[test]
fn line_and_sparse_file_bounds_fail_before_unbounded_reads() {
    let f = Fixture::new();
    let file = File::create(f.directory.join("case.21")).unwrap();
    // 실제 sparse metadata 경계를 작은 한도로 검사한다. production과 CI의512MiB는 유지한다.
    let file_bound = 4096;
    file.set_len(file_bound).unwrap();
    let (parent, _) = directory(&f.directory).unwrap();
    let accepted = roster_with_file_bound(&parent, file_bound).unwrap();
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].identity.size, file_bound);
    file.set_len(file_bound + 1).unwrap();
    assert_eq!(
        roster_with_file_bound(&parent, file_bound)
            .unwrap_err()
            .to_string(),
        "trace roster entry unsafe"
    );
    let f = Fixture::new();
    let mut raw = b"1.000001 read(3, \"".to_vec();
    raw.extend(vec![b'a'; MAX_LINE_BYTES]);
    raw.extend_from_slice(b"\", 1) = 1\n");
    f.put("21", &raw);
    f.error("trace line exceeds bound");
}

#[test]
fn signal_cpu_comments_and_no_return_exit_are_not_successful_syscalls() {
    let f = Fixture::new();
    f.put("21",b"1.000001 --- SIGCHLD {si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=22, si_uid=1001, si_status=0, si_utime=1 /* 0.01 s */, si_stime=0} ---\n1.000002 exit_group(0) = ?\n1.000003 +++ exited with 0 +++\n");
    let report = f.report();
    assert_eq!(report["events"][1]["outcome"], "no_return");
}

#[test]
fn model_and_report_byte_limits_reject_further_data() {
    let mut events = Vec::new();
    let mut budget = MAX_MODEL_BYTES;
    assert_eq!(
        push(&mut events, &mut budget, event(21, "signal", Vec::new()))
            .unwrap_err()
            .to_string(),
        "trace model exceeds bound"
    );
    assert!(events.is_empty());
    let mut writer = ReportBytes(vec![0; MAX_MODEL_BYTES]);
    assert!(writer.write_all(b"x").is_err());
    assert_eq!(writer.0.len(), MAX_MODEL_BYTES);
}

#[test]
fn dispatch_and_help_require_only_the_first_literal_and_no_files() {
    assert!(selected(&[OsString::from("inspect-trace")]));
    assert!(!selected(&[
        OsString::from("retained"),
        OsString::from("inspect-trace")
    ]));
    cli(&[OsString::from("--help")]).unwrap();
}

#[test]
fn raw_read_hex_arguments_and_returns_keep_source_ranges_and_opaque_body() {
    let f = Fixture::new();
    f.put("21",b"1.000001 read(0x3, 0x7f123000, 0x1000) = 0x40\n1.000002 pread64(0, 0x7f123000, 0x1000, 0) = -1 EAGAIN (Resource temporarily unavailable)\n1.000003 +++ exited with 0 +++\n");
    let report = f.report();
    let rows = report["events"].as_array().unwrap();
    assert_eq!(rows[0]["numeric_read_arguments"][0]["bytes"], json!(b"0x3"));
    assert_eq!(rows[0]["numeric_read_result"]["bytes"], json!(b"0x40"));
    assert_eq!(rows[1]["outcome"], "failed");
    assert!(rows[1]["numeric_read_result"].is_null());
    assert_eq!(rows[0]["opaque_payload"], true);
    assert_eq!(report["closed_fd"], false);
}

#[test]
fn output_seal_rejects_same_inode_rewrite_replacement_and_added_alias() {
    for action in 0..3 {
        let f = Fixture::new();
        fs::write(&f.output, b"original").unwrap();
        let held = File::open(&f.output).unwrap();
        let saved = identity(&held.metadata().unwrap());
        let expected = format!("{:x}", Sha256::digest(b"original"));
        let (parent, _) = directory(f.output.parent().unwrap()).unwrap();
        verify_output_seal(&held, &parent, "inspection.json", &saved, &expected).unwrap();
        match action {
            0 => {
                fs::write(&f.output, b"changed!").unwrap();
                let current = identity(&held.metadata().unwrap());
                assert_eq!(
                    (current.dev, current.ino, current.size),
                    (saved.dev, saved.ino, saved.size)
                );
                // Even a refreshed metadata baseline cannot bypass the independent expected bytes.
                assert_eq!(
                    verify_output_seal(&held, &parent, "inspection.json", &current, &expected)
                        .unwrap_err()
                        .to_string(),
                    "trace output changed"
                );
            }
            1 => {
                fs::rename(&f.output, f.output.with_extension("saved")).unwrap();
                fs::write(&f.output, b"original").unwrap();
                assert_ne!(fs::metadata(&f.output).unwrap().ino(), saved.ino);
            }
            _ => {
                fs::hard_link(&f.output, f.output.with_extension("alias")).unwrap();
                assert_eq!(held.metadata().unwrap().nlink(), 2);
            }
        }
        assert_eq!(
            verify_output_seal(&held, &parent, "inspection.json", &saved, &expected)
                .unwrap_err()
                .to_string(),
            "trace output changed"
        );
    }
}
