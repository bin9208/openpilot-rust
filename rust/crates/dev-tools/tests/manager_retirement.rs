//! Hosted same-ELF manager QA proof in the checkout and a private root without
//! the three historical QA controllers. Original observations are fixed data.
#![cfg(all(target_os = "linux", feature = "manager-qa-native"))]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{Value, json};
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

const RETIRED: [&str; 3] = [
    "rust/tools/check_manager.py",
    "rust/tools/check_manager_logging.py",
    "rust/tools/manager_reference.py",
];
const REQUIRED: [&str; 5] = [
    "rust/tools/generate_manager_cars.py",
    "rust/tools/original_params_binding.py",
    "openpilot/system/manager/manager.py",
    "openpilot/common/params.py",
    "openpilot/common/swaglog.py",
];
const CASES: [&str; 7] = [
    "normal",
    "registration_failure",
    "poll_failure",
    "prepare_only",
    "interrupted",
    "default_edges",
    "reset_defaults",
];
const MAX_ELF: u64 = 512 * 1024 * 1024;

type Identity = (u64, u64, u64, u32, (i64, i64), (i64, i64));

fn identity(meta: &fs::Metadata) -> Identity {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mode(),
        (meta.mtime(), meta.mtime_nsec()),
        (meta.ctime(), meta.ctime_nsec()),
    )
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct HeldElf {
    name: &'static str,
    path: PathBuf,
    file: File,
    identity: Identity,
    hash: String,
}

impl HeldElf {
    fn open(name: &'static str, path: PathBuf) -> Self {
        let path = path.canonicalize().unwrap();
        let before = fs::symlink_metadata(&path).unwrap();
        assert!(before.is_file() && !before.file_type().is_symlink());
        assert!(before.len() <= MAX_ELF);
        assert!(name == "binding" || before.mode() & 0o111 != 0);
        let file = File::from(
            rustix::fs::open(
                &path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        let meta = file.metadata().unwrap();
        assert_eq!(identity(&before), identity(&meta));
        let mut header = [0; 20];
        file.try_clone().unwrap().read_exact(&mut header).unwrap();
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            other => panic!("unsupported manager QA host {other}"),
        };
        assert!(
            &header[..4] == b"\x7fELF"
                && header[4] == 2
                && header[5] == 1
                && u16::from_le_bytes([header[18], header[19]]) == machine,
            "{name} is not a current-host ELF"
        );
        let mut bytes = Vec::new();
        let mut reader = file.try_clone().unwrap();
        reader.seek(SeekFrom::Start(0)).unwrap();
        reader.take(MAX_ELF + 1).read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes.len() as u64, meta.len());
        let held = Self {
            name,
            path,
            file,
            identity: identity(&meta),
            hash: sha(&bytes),
        };
        held.verify();
        held
    }

    fn verify(&self) {
        for meta in [
            self.file.metadata().unwrap(),
            fs::symlink_metadata(&self.path).unwrap(),
        ] {
            assert_eq!(
                identity(&meta),
                self.identity,
                "{} identity drift",
                self.name
            );
        }
        let mut reader = self.file.try_clone().unwrap();
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        reader.take(MAX_ELF + 1).read_to_end(&mut bytes).unwrap();
        assert_eq!(sha(&bytes), self.hash, "{} content drift", self.name);
    }

    fn receipt(&self) -> Value {
        json!({"name":self.name,"path":self.path.as_os_str().as_bytes(),
            "sha256":self.hash,"bytes":self.identity.2,"device":self.identity.0,
            "inode":self.identity.1})
    }
}

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn copy_tree(from: &Path, into: &Path) {
    fs::create_dir_all(into).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = into.join(entry.file_name());
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            assert!(kind.is_file(), "unexpected fixture node");
            let source = fs::read(entry.path()).unwrap();
            fs::write(&target, &source).unwrap();
            assert_eq!(fs::read(target).unwrap(), source);
        }
    }
}

fn private_root(checkout: &Path, root: &Path) {
    for name in REQUIRED {
        let source = checkout.join(name);
        let target = root.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let bytes = fs::read(&source).unwrap();
        fs::write(&target, &bytes).unwrap();
        assert_eq!(sha(&fs::read(target).unwrap()), sha(&bytes));
    }
    copy_tree(
        &checkout.join("rust/crates/dev-tools/tests/fixtures/manager-qa-original"),
        &root.join("rust/crates/dev-tools/tests/fixtures/manager-qa-original"),
    );
    for name in RETIRED {
        assert!(
            fs::symlink_metadata(root.join(name))
                .is_err_and(|error| { error.kind() == std::io::ErrorKind::NotFound })
        );
    }
}

fn raw_path(encoded: &str) -> Vec<u8> {
    let bytes = encoded.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] == b'\\' {
            assert!(pos + 3 < bytes.len() && bytes[pos + 1] == b'x');
            let value = u8::from_str_radix(&encoded[pos + 2..pos + 4], 16).unwrap();
            result.push(value);
            pos += 4;
        } else {
            assert!(bytes[pos].is_ascii() && bytes[pos] != b'"');
            result.push(bytes[pos]);
            pos += 1;
        }
    }
    result
}

fn source_negative_controls(
    checkout: &Path,
    private: &Path,
    evidence: &Path,
    binaries: &[HeldElf; 7],
) -> usize {
    let mut controls = RETIRED
        .iter()
        .enumerate()
        .map(|(index, path)| (format!("reappearance-{index}"), private.join(*path)))
        .collect::<Vec<_>>();
    controls.push(("retained-drift".to_owned(), private.join(REQUIRED[0])));
    controls.push(("missing-parent".to_owned(), private.join("rust/tools")));
    let count = controls.len();
    for (control, path) in controls {
        let probe = evidence.join(format!("negative-output-{control}"));
        if control == "missing-parent" {
            fs::rename(&path, private.join("rust/old-tools")).unwrap();
        } else {
            fs::write(&path, b"changed").unwrap();
        }
        let observed = traced(
            private,
            evidence,
            &format!("negative-{control}"),
            vec![
                binaries[0].path.as_os_str().into(),
                "--binary".into(),
                binaries[3].path.as_os_str().into(),
                "--output".into(),
                probe.as_os_str().into(),
                "--source-root".into(),
                private.as_os_str().into(),
                "--require-retired".into(),
            ],
            1,
            &[(&binaries[0].path, 1)],
        );
        assert!(!probe.exists(), "failed source admission created output");
        assert_eq!(observed["status"], 1);
        let stderr = observed["stderr"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| byte.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let stderr = String::from_utf8(stderr).unwrap();
        let expected_diagnostic = if control.starts_with("reappearance-") {
            "manager QA historical source roster is mixed"
        } else if control == "retained-drift" {
            "manager QA original source changed: rust/tools/generate_manager_cars.py"
        } else {
            "No such file or directory"
        };
        assert!(
            stderr.contains(expected_diagnostic),
            "{control} failed for the wrong reason: {stderr}"
        );
        if control.starts_with("reappearance-") {
            fs::remove_file(&path).unwrap();
        }
        if control == "retained-drift" {
            fs::copy(checkout.join(REQUIRED[0]), &path).unwrap();
        }
        if control == "missing-parent" {
            fs::rename(private.join("rust/old-tools"), &path).unwrap();
        }
    }
    count
}

fn audit_exec(trace: &Path, expected: &[(&Path, usize)]) -> Value {
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let success = regex::Regex::new(r#"\bexecve\("([^"]*)", \[.*\], .*\) = 0$"#).unwrap();
    let mut observed = BTreeMap::<Vec<u8>, usize>::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(trace).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let bytes = fs::read(entry.path()).unwrap();
        assert!(bytes.len() <= 16 * 1024 * 1024 && bytes.ends_with(b"\n"));
        let text = std::str::from_utf8(&bytes).unwrap();
        for line in text.lines() {
            if !line.contains("execve") {
                continue;
            }
            assert!(opening.is_match(line), "opaque execution attempt: {line}");
            assert!(!line.contains("...") && !line.contains("execveat("));
            let parsed = success
                .captures(line)
                .unwrap_or_else(|| panic!("opaque or failed exec: {line}"));
            *observed.entry(raw_path(&parsed[1])).or_default() += 1;
        }
        files.push(
            json!({"name":entry.file_name().as_bytes(),"bytes":bytes.len(),"sha256":sha(&bytes)}),
        );
    }
    assert!(!files.is_empty(), "missing manager QA strace evidence");
    let target = expected
        .iter()
        .map(|(path, count)| (path.as_os_str().as_bytes().to_vec(), *count))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(observed, target, "manager QA exec roster changed");
    json!({"programs":observed.iter().map(|(p,n)|json!({"path_bytes":p,"count":n})).collect::<Vec<_>>(),"files":files})
}

fn traced(
    root: &Path,
    evidence: &Path,
    label: &str,
    argv: Vec<OsString>,
    expected_status: i32,
    programs: &[(&Path, usize)],
) -> Value {
    let lane = evidence.join(label);
    fs::create_dir(&lane).unwrap();
    let trace = lane.join("exec");
    fs::create_dir(&trace).unwrap();
    let mut command = CommandSpec::new_os(
        [
            vec![
                OsString::from("/usr/bin/strace"),
                OsString::from("-f"),
                OsString::from("-ff"),
                OsString::from("-qq"),
                OsString::from("-xx"),
                OsString::from("-s32768"),
                OsString::from("-e"),
                OsString::from("trace=execve,execveat"),
                OsString::from("-o"),
                trace.join("trace").into_os_string(),
                OsString::from("--"),
            ],
            argv.clone(),
        ]
        .concat(),
        Duration::from_secs(300),
    );
    command.cwd = Some(root.to_path_buf());
    command.env_remove = vec!["PYTHONPATH".into(), "PYTHONHOME".into()];
    command.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root.as_os_str().to_os_string(),
    );
    command
        .env
        .insert("PATH".into(), OsString::from("/nonexistent"));
    let captured =
        NativeRunner.run_to_files(&command, &lane.join("stdout.log"), &lane.join("stderr.log"));
    let stdout = fs::read(lane.join("stdout.log")).unwrap();
    let stderr = fs::read(lane.join("stderr.log")).unwrap();
    fs::write(
        lane.join("invocation.json"),
        serde_json::to_vec_pretty(
            &json!({"argv":argv.iter().map(|v|v.as_bytes()).collect::<Vec<_>>(),
            "cwd":root.as_os_str().as_bytes(),"status":captured.as_ref().ok().map(|x|x.code),
            "runner_error":captured.as_ref().err().map(ToString::to_string),
            "stdout_sha256":sha(&stdout),"stderr_sha256":sha(&stderr)}),
        )
        .unwrap(),
    )
    .unwrap();
    let result = captured.unwrap_or_else(|error| panic!("{label} runner failed: {error}"));
    assert_eq!(
        result.code,
        expected_status,
        "{label} stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    let attempts = audit_exec(&trace, programs);
    json!({"stdout":stdout,"stderr":stderr,"status":result.code,"exec":attempts})
}

#[test]
#[ignore = "hosted same-ELF manager QA original/private source-absence and closed exec proof"]
fn manager_original_observations_survive_three_controller_retirement() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let evidence = required("MANAGER_RETIREMENT_EVIDENCE");
    assert!(!evidence.exists());
    fs::create_dir(&evidence).unwrap();
    let expect_checkout_absent = match std::env::var("MANAGER_RETIREMENT_REQUIRE_ABSENT")
        .expect("MANAGER_RETIREMENT_REQUIRE_ABSENT required")
        .as_str()
    {
        "0" => false,
        "1" => true,
        value => panic!("invalid MANAGER_RETIREMENT_REQUIRE_ABSENT {value}"),
    };
    let binaries = [
        HeldElf::open("unified", required("MANAGER_RETIREMENT_CHECKER")),
        HeldElf::open("manager", required("MANAGER_RETIREMENT_MANAGER")),
        HeldElf::open("logging", required("MANAGER_RETIREMENT_LOGGING")),
        HeldElf::open("trace", required("MANAGER_RETIREMENT_TRACE")),
        HeldElf::open("binding", required("MANAGER_RETIREMENT_BINDING")),
        HeldElf::open("strace", PathBuf::from("/usr/bin/strace")),
        HeldElf::open("env", PathBuf::from("/usr/bin/env")),
    ];
    let held_before = binaries.iter().map(HeldElf::receipt).collect::<Vec<_>>();
    fs::write(
        evidence.join("held-before.json"),
        serde_json::to_vec_pretty(&held_before).unwrap(),
    )
    .unwrap();
    let private = evidence.join("private-root");
    fs::create_dir(&private).unwrap();
    private_root(&checkout, &private);
    let fixture =
        checkout.join("rust/crates/dev-tools/tests/fixtures/manager-qa-original/retirement");
    let original_admissions = fs::read_to_string(fixture.join("cli-status.txt")).unwrap();
    let cases = CASES
        .iter()
        .map(|case| {
            let path = fixture
                .join("manager")
                .join(format!("{case}-source/result.json"));
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap()
        })
        .collect::<Vec<_>>();
    let negative_count = source_negative_controls(&checkout, &private, &evidence, &binaries);
    let mut reports = Vec::new();
    for (root, name, absent) in [
        (&checkout, "checkout", expect_checkout_absent),
        (&private, "private", true),
    ] {
        for historical in RETIRED {
            let path = root.join(historical);
            assert_eq!(
                fs::symlink_metadata(path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                absent,
                "wrong historical source state at {name}: {historical}"
            );
        }
        let lane = evidence.join(name);
        fs::create_dir(&lane).unwrap();
        let native = lane.join("native");
        let mut checker_argv = vec![
            binaries[0].path.as_os_str().into(),
            "--binary".into(),
            binaries[3].path.as_os_str().into(),
            "--output".into(),
            native.as_os_str().into(),
            "--source-root".into(),
            root.as_os_str().into(),
        ];
        if absent {
            checker_argv.push("--require-retired".into());
        }
        let checker = traced(
            root,
            &lane,
            "unified",
            checker_argv,
            0,
            &[
                (&binaries[0].path, 1),
                (&binaries[3].path, 8),
                (&binaries[6].path, 1),
            ],
        );
        assert_eq!(checker["status"], 0);
        let result: Value =
            serde_json::from_slice(&fs::read(native.join("result.json")).unwrap()).unwrap();
        assert_eq!(result["cases"].as_object().unwrap().len(), CASES.len());
        assert_eq!(
            result["logging"]["full_packets"].as_array().unwrap().len(),
            4
        );
        assert_eq!(result["historical_sources_absent"], absent);
        for (case, original) in CASES.iter().zip(&cases) {
            let actual: Value =
                serde_json::from_slice(&fs::read(native.join(case).join("result.json")).unwrap())
                    .unwrap();
            assert_eq!(&actual, original, "case {case} in {name}");
        }
        let selected = lane.join("selected");
        let manager = traced(
            root,
            &lane,
            "manager",
            vec![
                binaries[1].path.as_os_str().into(),
                binaries[3].path.as_os_str().into(),
                binaries[4].path.as_os_str().into(),
                selected.as_os_str().into(),
                "--scenario".into(),
                "normal".into(),
                "--scenario".into(),
                "normal".into(),
                "--scenario".into(),
                "custom".into(),
            ],
            0,
            &[(&binaries[1].path, 1), (&binaries[3].path, 3)],
        );
        assert_eq!(
            manager["stdout"],
            json!(fs::read(fixture.join("manager-selected.stdout")).unwrap())
        );
        let expected: Value = serde_json::from_slice(
            &fs::read(fixture.join("manager-selected/summary.json")).unwrap(),
        )
        .unwrap();
        let actual: Value =
            serde_json::from_slice(&fs::read(selected.join("summary.json")).unwrap()).unwrap();
        assert_eq!(actual, expected);
        for (native_name, source_name) in [
            ("normal-native", "normal-source"),
            ("custom-native", "custom-source"),
        ] {
            let actual: Value = serde_json::from_slice(
                &fs::read(selected.join(native_name).join("result.json")).unwrap(),
            )
            .unwrap();
            let expected: Value = serde_json::from_slice(
                &fs::read(
                    fixture
                        .join("manager-selected")
                        .join(source_name)
                        .join("result.json"),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(actual, expected, "positional scenario {source_name}");
        }
        let logs = lane.join("logging");
        let logging = traced(
            root,
            &lane,
            "logging",
            vec![
                binaries[2].path.as_os_str().into(),
                binaries[3].path.as_os_str().into(),
                binaries[4].path.as_os_str().into(),
                logs.as_os_str().into(),
            ],
            0,
            &[
                (&binaries[2].path, 1),
                (&binaries[3].path, 1),
                (&binaries[6].path, 1),
            ],
        );
        assert_eq!(
            logging["stdout"],
            json!(fs::read(fixture.join("manager-logging.stdout")).unwrap())
        );
        let actual: Value =
            serde_json::from_slice(&fs::read(logs.join("native/result.json")).unwrap()).unwrap();
        let original: Value = serde_json::from_slice(
            &fs::read(fixture.join("manager-logging/source/result.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, original);
        let packets: Value =
            serde_json::from_slice(&fs::read(logs.join("native/native-records.json")).unwrap())
                .unwrap();
        assert_eq!(packets.as_array().unwrap().len(), 4);
        for (tool, elf) in [
            ("check_manager.py", &binaries[1]),
            ("check_manager_logging.py", &binaries[2]),
        ] {
            for (case, args, code) in [
                ("help", vec![OsString::from("--help")], 0),
                ("missing", Vec::new(), 2),
                ("unknown", vec![OsString::from("--unknown")], 2),
            ] {
                let mut argv = vec![elf.path.as_os_str().to_os_string()];
                argv.extend(args);
                let observed = traced(
                    root,
                    &lane,
                    &format!("{tool}-{case}"),
                    argv,
                    code,
                    &[(&elf.path, 1)],
                );
                let expected_admission = format!("{tool} {case} {code}");
                assert_eq!(
                    original_admissions
                        .lines()
                        .filter(|row| *row == expected_admission.as_str())
                        .count(),
                    1,
                    "original admission status missing or duplicated"
                );
                let recorded = fs::read(fixture.join(format!("cli-{tool}-{case}.stdout"))).unwrap();
                assert_eq!(observed["stdout"], json!(recorded));
                let original_stderr =
                    fs::read(fixture.join(format!("cli-{tool}-{case}.stderr"))).unwrap();
                assert_eq!(observed["stderr"], json!(original_stderr));
            }
        }
        let broad_controls = [
            (
                "manager-missing-output",
                &binaries[1],
                vec![
                    binaries[3].path.as_os_str().to_os_string(),
                    binaries[4].path.as_os_str().to_os_string(),
                ],
                b"usage: check_manager.py [-h] [--scenario SCENARIO] binary binding output\ncheck_manager.py: error: the following arguments are required: output\n".to_vec(),
            ),
            (
                "logging-missing-output",
                &binaries[2],
                vec![
                    binaries[3].path.as_os_str().to_os_string(),
                    binaries[4].path.as_os_str().to_os_string(),
                ],
                b"usage: check_manager_logging.py [-h] binary binding output\ncheck_manager_logging.py: error: the following arguments are required: output\n".to_vec(),
            ),
            (
                "manager-unknown-with-positionals",
                &binaries[1],
                vec![
                    binaries[3].path.as_os_str().to_os_string(),
                    binaries[4].path.as_os_str().to_os_string(),
                    lane.join("manager-unknown-output").into_os_string(),
                    OsString::from("--unknown"),
                ],
                b"usage: check_manager.py [-h] [--scenario SCENARIO] binary binding output\ncheck_manager.py: error: unrecognized arguments: --unknown\n".to_vec(),
            ),
            (
                "logging-unknown-with-positionals",
                &binaries[2],
                vec![
                    binaries[3].path.as_os_str().to_os_string(),
                    binaries[4].path.as_os_str().to_os_string(),
                    lane.join("logging-unknown-output").into_os_string(),
                    OsString::from("--unknown"),
                ],
                b"usage: check_manager_logging.py [-h] binary binding output\ncheck_manager_logging.py: error: unrecognized arguments: --unknown\n".to_vec(),
            ),
            (
                "manager-scenario-missing-value",
                &binaries[1],
                vec![
                    binaries[3].path.as_os_str().to_os_string(),
                    binaries[4].path.as_os_str().to_os_string(),
                    lane.join("manager-scenario-output").into_os_string(),
                    OsString::from("--scenario"),
                ],
                b"usage: check_manager.py [-h] [--scenario SCENARIO] binary binding output\ncheck_manager.py: error: argument --scenario: expected one argument\n".to_vec(),
            ),
        ];
        for (label, elf, args, expected_stderr) in broad_controls {
            let mut argv = vec![elf.path.as_os_str().to_os_string()];
            argv.extend(args);
            let observed = traced(root, &lane, label, argv, 2, &[(elf.path.as_path(), 1)]);
            assert_eq!(observed["stdout"], json!([]), "{label}");
            assert_eq!(observed["stderr"], json!(expected_stderr), "{label}");
        }
        for elf in &binaries {
            elf.verify();
        }
        reports.push(
            json!({"root":root.as_os_str().as_bytes(),"historical_absent":absent,
            "cases":7,"logs":4,"cli_admissions":6,"positive_traffic_exec_attempts":17,
            "total_exec_attempts":28,
            "elfs":binaries.iter().map(HeldElf::receipt).collect::<Vec<_>>() }),
        );
    }
    for elf in &binaries {
        elf.verify();
    }
    let held_after = binaries.iter().map(HeldElf::receipt).collect::<Vec<_>>();
    assert_eq!(held_before, held_after);
    fs::write(
        evidence.join("held-after.json"),
        serde_json::to_vec_pretty(&held_after).unwrap(),
    )
    .unwrap();
    for historical in RETIRED {
        assert!(
            fs::symlink_metadata(private.join(historical))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        );
    }
    for name in REQUIRED {
        assert_eq!(
            sha(&fs::read(private.join(name)).unwrap()),
            sha(&fs::read(checkout.join(name)).unwrap()),
            "retained manager QA source drift after controls: {name}"
        );
    }
    fs::write(
        evidence.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "passed":true,"roots":reports,"historical_source_count":RETIRED.len(),
            "negative_controls":negative_count,"original_case_count":7,"original_log_records":4,
            "captured_cli_admissions_per_root":6,"broader_cli_diagnostic_controls_per_root":5,
            "total_cli_controls_per_root":11,"positive_traffic_exec_attempts_per_root":17,
            "closed_native_exec_attempts_per_positive_root":28,
            "source_admission_negative_exec_attempts":negative_count,
            "total_closed_native_exec_attempts":61,
            "source_capture_run":39728775823u64,"device_tested":false
        }))
        .unwrap(),
    )
    .unwrap();
}
