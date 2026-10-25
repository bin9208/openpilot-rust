//! Same-checker modem proof in the repository and a private source-absent root.
#![cfg(all(target_os = "linux", feature = "modem-qa-native"))]

use openpilot_dev_tools::{
    modem_capture,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 6] = [
    "openpilot/system/hardware/tici/modem.py",
    "rust/tools/check_modem.py",
    "rust/tools/modem_source.py",
    "rust/tools/modem_fixture.py",
    "rust/tools/modem_fixture_command.py",
    "rust/tools/check_modem_process_boundary.py",
];
const FIXTURE: &str = "rust/crates/dev-tools/tests/fixtures/modem-original";
const ELFS: [(&str, &str); 4] = [
    ("checker", "MODEM_CHECKER_BINARY"),
    ("daemon", "MODEM_DAEMON_BINARY"),
    ("trace", "MODEM_TRACE_BINARY"),
    ("process_launcher", "MODEM_PROCESS_CHILD_BINARY"),
];
const MAX_ELF_BYTES: u64 = 512 * 1024 * 1024;

fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sha_file(file: &File, len: u64) -> String {
    let mut reader = file.try_clone().unwrap();
    reader.seek(SeekFrom::Start(0)).unwrap();
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut bytes = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        total += count as u64;
        assert!(total <= MAX_ELF_BYTES, "ELF exceeds its read bound");
        hash.update(&bytes[..count]);
    }
    assert_eq!(total, len, "ELF length changed while hashing");
    format!("{:x}", hash.finalize())
}

fn identity(metadata: &fs::Metadata) -> (u64, u64, u64, u32, (i64, i64), (i64, i64)) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mode(),
        (metadata.mtime(), metadata.mtime_nsec()),
        (metadata.ctime(), metadata.ctime_nsec()),
    )
}

struct HeldElf {
    name: &'static str,
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, u32, (i64, i64), (i64, i64)),
    sha256: String,
}

impl HeldElf {
    fn open(name: &'static str, path: PathBuf) -> Self {
        let canonical = path.canonicalize().unwrap();
        assert_eq!(canonical, path, "{} path must be canonical", name);
        let file = File::open(&path).unwrap();
        let metadata = file.metadata().unwrap();
        assert!(metadata.is_file() && metadata.len() <= MAX_ELF_BYTES);
        assert_ne!(metadata.mode() & 0o111, 0, "{} is not executable", name);
        let mut reader = file.try_clone().unwrap();
        let mut elf = [0u8; 20];
        reader.read_exact(&mut elf).unwrap();
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            other => panic!("unsupported hosted ELF architecture: {other}"),
        };
        assert!(
            &elf[..4] == b"\x7fELF"
                && elf[4] == 2
                && elf[5] == 1
                && elf[6] == 1
                && u16::from_le_bytes([elf[18], elf[19]]) == machine,
            "{} is not a current-host 64-bit ELF",
            name
        );
        let before = identity(&metadata);
        let sha256 = sha_file(&file, metadata.len());
        assert_eq!(identity(&file.metadata().unwrap()), before);
        Self {
            name,
            path,
            file,
            identity: before,
            sha256,
        }
    }

    fn verify(&self) {
        let held = self.file.metadata().unwrap();
        assert_eq!(
            identity(&held),
            self.identity,
            "{} held ELF changed",
            self.name
        );
        assert_eq!(sha_file(&self.file, self.identity.2), self.sha256);
        let current = fs::symlink_metadata(&self.path).unwrap();
        assert!(current.is_file() && !current.file_type().is_symlink());
        assert_eq!(
            identity(&current),
            self.identity,
            "{} ELF path changed",
            self.name
        );
    }

    fn report(&self) -> Value {
        json!({"name":self.name,"path":self.path.to_string_lossy(),
            "path_bytes_hex":self.path.as_os_str().as_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),
            "bytes":self.identity.2,"sha256":self.sha256,"device":self.identity.0,"inode":self.identity.1})
    }
}

fn held_elfs() -> Vec<HeldElf> {
    ELFS.iter()
        .map(|(name, env)| HeldElf::open(name, required(env).canonicalize().unwrap()))
        .collect()
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = destination.join(entry.file_name());
        let metadata = entry.file_type().unwrap();
        if metadata.is_dir() {
            copy_tree(&path, &target);
        } else {
            assert!(
                metadata.is_file(),
                "unexpected fixture entry: {}",
                path.display()
            );
            let input = fs::read(&path).unwrap();
            fs::write(&target, &input).unwrap();
            assert_eq!(fs::read(&target).unwrap(), input);
        }
    }
}

fn absent(root: &Path) {
    for name in RETIRED {
        assert!(
            fs::symlink_metadata(root.join(name))
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            "retired modem source reappeared: {name}"
        );
    }
}

fn invoke(
    checker: &Path,
    args: &[OsString],
    cwd: &Path,
    evidence: &Path,
    expected_status: i32,
) -> Value {
    fs::create_dir(evidence).unwrap();
    let argv = std::iter::once(checker.as_os_str().to_owned())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let mut command = CommandSpec::new_os(argv.clone(), Duration::from_secs(300));
    command.cwd = Some(cwd.to_path_buf());
    command.env_remove = vec![
        OsString::from("PYTHONPATH"),
        OsString::from("PYTHONHOME"),
        OsString::from("MODEM_FIXTURE_ROOT"),
    ];
    command.stdout_limit = 8 * 1024 * 1024;
    command.stderr_limit = 8 * 1024 * 1024;
    let result = NativeRunner.run_to_files(
        &command,
        &evidence.join("stdout.log"),
        &evidence.join("stderr.log"),
    );
    let stdout = fs::read(evidence.join("stdout.log")).unwrap();
    let stderr = fs::read(evidence.join("stderr.log")).unwrap();
    let code = result.as_ref().ok().map(|captured| captured.code);
    fs::write(
        evidence.join("invocation.json"),
        serde_json::to_vec_pretty(&json!({
            "argv_bytes":argv.iter().map(|arg|arg.as_bytes().to_vec()).collect::<Vec<_>>(),
            "cwd_bytes":cwd.as_os_str().as_bytes(),
            "timeout_seconds":command.timeout.as_secs(),
            "stdout_bytes":stdout.len(),"stdout_sha256":sha(&stdout),
            "stderr_bytes":stderr.len(),"stderr_sha256":sha(&stderr),
            "exit_code":code,"runner_error":result.as_ref().err().map(ToString::to_string)
        }))
        .unwrap(),
    )
    .unwrap();
    let captured = result.unwrap_or_else(|error| panic!("checker runner failed: {error}"));
    assert_eq!(
        captured.code,
        expected_status,
        "checker exit status: {}",
        String::from_utf8_lossy(&stderr)
    );
    if expected_status == 0 {
        serde_json::from_slice(&stdout)
            .unwrap_or_else(|_| json!({"stdout":String::from_utf8_lossy(&stdout)}))
    } else {
        json!({"exit_code":captured.code,"stdout_sha256":sha(&stdout),"stderr_sha256":sha(&stderr)})
    }
}

fn checker_args(
    source: &Path,
    evidence: &Path,
    source_state: modem_capture::SourceState,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--binary"),
        required("MODEM_DAEMON_BINARY")
            .canonicalize()
            .unwrap()
            .into_os_string(),
        OsString::from("--trace"),
        required("MODEM_TRACE_BINARY")
            .canonicalize()
            .unwrap()
            .into_os_string(),
        OsString::from("--launcher"),
        required("MODEM_PROCESS_CHILD_BINARY")
            .canonicalize()
            .unwrap()
            .into_os_string(),
        OsString::from("--evidence"),
        evidence.as_os_str().to_owned(),
        OsString::from("--source-root"),
        source.as_os_str().to_owned(),
    ];
    match source_state {
        modem_capture::SourceState::OriginalPresent => {}
        modem_capture::SourceState::ToolsRetiredRuntimePinned => {
            args.push(OsString::from("--require-tools-retired"));
        }
        modem_capture::SourceState::AllRetired => {
            args.push(OsString::from("--require-retired"));
        }
    }
    args
}

fn check_case_files(evidence: &Path) -> ((usize, usize, usize), BTreeMap<String, Value>) {
    let report: Value =
        serde_json::from_slice(&fs::read(evidence.join("results.json")).unwrap()).unwrap();
    assert_eq!(report["passed"], 17);
    assert_eq!(report["request_plan_validation"]["source_cases"], 13);
    assert_eq!(
        report["request_plan_validation"]["paired_request_plans"],
        26
    );
    assert_eq!(
        report["request_plan_validation"]["original_source_operations"],
        40
    );
    assert_eq!(
        report["request_plan_validation"]["scenario_operations_match"],
        true
    );
    assert_eq!(
        report["request_plan_validation"]["response_overrides_match"],
        true
    );
    assert_eq!(
        report["request_plan_validation"]["default_responses_match"],
        true
    );
    assert_eq!(
        report["request_plan_validation"]["scalar_configuration_match"],
        true
    );
    assert_eq!(
        report["source_capture"]["helper_original_reference"]["sha256"],
        "7e99f8fa3501a481e83a72a00b965ec9c3009404071de32adacbbf51c57327d2"
    );
    let scenarios = report["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 13);
    let mut rows = 0;
    let mut commands = 0;
    let mut calls = 0;
    let mut native_by_case = BTreeMap::new();
    for scenario in scenarios {
        assert_eq!(scenario["pass"], true);
        let name = scenario["scenario"].as_str().unwrap();
        let case: Value =
            serde_json::from_slice(&fs::read(evidence.join(format!("{name}.json"))).unwrap())
                .unwrap();
        assert_eq!(case["pass"], true);
        for side in ["original_source", "native"] {
            rows += case[side]["rows"].as_array().unwrap().len();
            commands += case[side]["commands"].as_array().unwrap().len();
            calls += case[side]["calls"].as_array().unwrap().len();
        }
        assert_eq!(
            case["original_source"], case["native"],
            "full case differs: {name}"
        );
        native_by_case.insert(name.to_owned(), case["native"].clone());
    }
    assert!(evidence.join("lifecycle.json").is_file());
    let lifecycle: Value =
        serde_json::from_slice(&fs::read(evidence.join("lifecycle.json")).unwrap()).unwrap();
    assert_eq!(lifecycle["exit_code"], 0);
    assert_eq!(lifecycle["state_removed"], true);
    assert_eq!(lifecycle["child_reaped"], true);
    assert_eq!(lifecycle["snapshots"].as_array().unwrap().len(), 5);
    let lifecycle_calls = lifecycle["calls"].as_array().unwrap();
    assert!(lifecycle_calls.contains(&json!(["systemctl", "mask", "--runtime", "ModemManager"])));
    assert!(lifecycle_calls.contains(&json!(["systemctl", "start", "ModemManager"])));
    assert!(lifecycle_calls.contains(&json!(["resolvectl", "dns", "ppp0", "1.1.1.1", "8.8.8.8"])));
    assert_eq!(report["process_boundary"].as_array().unwrap().len(), 3);
    let descriptor: Value =
        serde_json::from_slice(&fs::read(evidence.join("descriptor-boundary.json")).unwrap())
            .unwrap();
    let descriptor_rows = descriptor["descriptor_closure"].as_array().unwrap();
    assert!(descriptor_rows.len() >= 8);
    assert!(descriptor_rows.iter().all(|row| row["leaked"] == false));
    assert!(descriptor_rows.iter().any(|row| row["argv"][0] == "pppd"));
    assert!(descriptor_rows.iter().any(|row| row["argv"][0] == "-4"));
    let shell: Value =
        serde_json::from_slice(&fs::read(evidence.join("shell-fallback.json")).unwrap()).unwrap();
    assert_ne!(shell["exit_code"], 0);
    assert_eq!(shell["shell_fallback_marker"], false);
    let startup: Value =
        serde_json::from_slice(&fs::read(evidence.join("startup-order.json")).unwrap()).unwrap();
    assert_ne!(startup["native_exit_code"], 0);
    assert_eq!(startup["source_external_commands"], json!([]));
    assert_eq!(startup["native_external_commands"], json!([]));
    ((rows, commands, calls), native_by_case)
}

#[test]
#[ignore = "hosted same-ELF modem source/private-root differential, PTY lifecycle and CLI gate"]
fn same_held_native_elfs_replay_modem_originals_in_checkout_and_private_absent_root() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let output = required("MODEM_RETIREMENT_OUTPUT");
    fs::create_dir(&output).unwrap();
    let checker = required("MODEM_CHECKER_BINARY").canonicalize().unwrap();
    let require_all_retired =
        std::env::var_os("MODEM_REQUIRE_RETIRED").is_some_and(|value| value == "1");
    let require_tools_retired =
        std::env::var_os("MODEM_REQUIRE_TOOLS_RETIRED").is_some_and(|value| value == "1");
    assert!(!(require_all_retired && require_tools_retired));
    let actual_state = if require_all_retired {
        modem_capture::SourceState::AllRetired
    } else if require_tools_retired {
        modem_capture::SourceState::ToolsRetiredRuntimePinned
    } else {
        modem_capture::SourceState::OriginalPresent
    };
    let held = held_elfs();
    assert_eq!(
        held.iter().find(|elf| elf.name == "checker").unwrap().path,
        checker
    );
    let source_receipts = [modem_capture::validate_with_state(&root, actual_state).unwrap()];
    let private = output.join("private-source-root");
    fs::create_dir(&private).unwrap();
    // Retired project files must be absent beneath real source-parent directories.
    fs::create_dir_all(private.join("rust/tools")).unwrap();
    fs::create_dir_all(private.join("openpilot/system/hardware/tici")).unwrap();
    let fixture_source = root.join(FIXTURE);
    let fixture_target = private.join(FIXTURE);
    copy_tree(&fixture_source, &fixture_target);
    let helper_reference =
        root.join("rust/crates/modem/tests/fixtures/original-source-reference.json");
    let private_helper_reference =
        private.join("rust/crates/modem/tests/fixtures/original-source-reference.json");
    fs::create_dir_all(private_helper_reference.parent().unwrap()).unwrap();
    fs::copy(&helper_reference, &private_helper_reference).unwrap();
    assert_eq!(
        fs::read(&helper_reference).unwrap(),
        fs::read(&private_helper_reference).unwrap()
    );
    for name in RETIRED {
        assert!(fs::symlink_metadata(private.join(name)).is_err());
    }
    let empty_cwd = output.join("empty-cwd");
    fs::create_dir(&empty_cwd).unwrap();
    let private_receipt = modem_capture::validate(&private, true).unwrap();
    let input_receipts = json!({
            "actual":{"root_bytes":root.as_os_str().as_bytes(),"source_state":format!("{actual_state:?}"),
            "capture":source_receipts[0].report()},
            "private":{"root_bytes":private.as_os_str().as_bytes(),"source_state":"AllRetired",
            "capture":private_receipt.report(),"retired_absent":RETIRED},
        "empty_cwd_bytes":empty_cwd.as_os_str().as_bytes()
    });
    fs::write(
        output.join("input-receipts-before.json"),
        serde_json::to_vec_pretty(&input_receipts).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("elf-before.json"),
        serde_json::to_vec_pretty(&json!(held.iter().map(HeldElf::report).collect::<Vec<_>>()))
            .unwrap(),
    )
    .unwrap();

    let mut totals = Vec::new();
    let mut full_cases_by_root = Vec::new();
    for (label, source, cwd, source_state) in [
        ("actual", root.as_path(), root.as_path(), actual_state),
        (
            "private",
            private.as_path(),
            empty_cwd.as_path(),
            modem_capture::SourceState::AllRetired,
        ),
    ] {
        let evidence = output.join(format!("{label}-run"));
        let args = checker_args(source, &evidence, source_state);
        let summary = invoke(
            &checker,
            &args,
            cwd,
            &output.join(format!("{label}-invocation")),
            0,
        );
        assert_eq!(summary, json!({"passed":14,"evidence":evidence}));
        let report: Value =
            serde_json::from_slice(&fs::read(evidence.join("results.json")).unwrap()).unwrap();
        let (counts, full_cases) = check_case_files(&evidence);
        full_cases_by_root.push(full_cases);
        totals.push(json!({"root":label,"cases":13,"full_rows":counts.0,
            "commands":counts.1,"calls":counts.2,"process_boundary":3,"lifecycle":true}));
        source_receipts[0].verify().unwrap();
        private_receipt.verify().unwrap();
        for elf in &held {
            elf.verify();
        }
        let report_elfs = report["executables"].as_array().unwrap();
        assert_eq!(report_elfs.len(), 4);
        for held_elf in &held {
            let expected = held_elf.report();
            assert_eq!(
                report_elfs
                    .iter()
                    .filter(|entry| {
                        ["path_bytes_hex", "sha256", "bytes", "device", "inode"]
                            .iter()
                            .all(|key| entry[*key] == expected[*key])
                    })
                    .count(),
                1,
                "{} held ELF is not uniquely bound in the checker report",
                held_elf.name
            );
        }
    }
    assert_eq!(
        full_cases_by_root[0], full_cases_by_root[1],
        "same checker/capture differs across roots"
    );

    // Six controls cover help and the original parser's missing/unknown argument exits.
    let controls = [
        ("help", vec![OsString::from("--help")], 0),
        ("missing", vec![], 2),
        ("unknown", vec![OsString::from("--unknown")], 2),
        ("missing-binary-value", vec![OsString::from("--binary")], 2),
        ("missing-trace-value", vec![OsString::from("--trace")], 2),
        (
            "missing-evidence-value",
            vec![OsString::from("--evidence")],
            2,
        ),
    ];
    for (name, args, status) in controls {
        for cwd in [&root, &empty_cwd] {
            invoke(
                &checker,
                &args,
                cwd,
                &output.join(format!(
                    "cli-{name}-{}",
                    if cwd == &root { "actual" } else { "empty" }
                )),
                status,
            );
        }
    }

    let mut reappearance = Vec::new();
    for name in RETIRED {
        let path = private.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"reappeared original Python source").unwrap();
        let evidence = output.join(format!("reappearance-{}", name.replace('/', "-")));
        let absent_output = output.join(format!("reappearance-output-{}", name.replace('/', "-")));
        let status = invoke(
            &checker,
            &checker_args(
                &private,
                &absent_output,
                modem_capture::SourceState::AllRetired,
            ),
            &empty_cwd,
            &evidence,
            1,
        );
        assert!(
            !absent_output.exists(),
            "failed admission produced a report"
        );
        fs::remove_file(&path).unwrap();
        private_receipt.verify().unwrap();
        reappearance
            .push(json!({"path":name,"exit_code":status["exit_code"],"report_absent":true}));
    }
    absent(&private);
    source_receipts[0].verify().unwrap();
    private_receipt.verify().unwrap();
    fs::write(
        output.join("input-receipts-after.json"),
        serde_json::to_vec_pretty(&input_receipts).unwrap(),
    )
    .unwrap();
    for elf in &held {
        elf.verify();
    }
    fs::write(
        output.join("elf-after.json"),
        serde_json::to_vec_pretty(&json!(held.iter().map(HeldElf::report).collect::<Vec<_>>()))
            .unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "result":"PASS","roots":2,"case_counts":totals,"cli_controls":12,
            "source_reappearance_controls":reappearance,"private_absent_source_count":RETIRED.len(),
            "accepted_source_removals":0,"retirement_acceptance":false,
            "same_held_elfs":true,"device_validation":false
        }))
        .unwrap(),
    )
    .unwrap();
}
