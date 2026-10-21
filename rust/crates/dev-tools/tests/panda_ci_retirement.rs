//! Same-ELF Panda controller proof in the checkout and a separately prepared
//! full source root with only the historical orchestrator and its tests absent.
//! All still-live Python phase checkers remain present and are explicitly run.
#![cfg(all(target_os = "linux", feature = "panda-ci-native"))]
use base64::{engine::general_purpose::STANDARD, Engine as _};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    os::unix::ffi::OsStringExt,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 2] = [
    "rust/tools/check_pandad_ci.py",
    "rust/tools/tests/test_pandad_ci.py",
];
const HISTORICAL: [&str; 2] = [
    "21dd66f08fb279f0d8bdee35b97fd22027e82275486c5a8510704cf3847676c9",
    "46575991a4a668d12828da5ee02832db015e5aaa159e3c617d1926b09329cb22",
];
const ROSTER: [&str; 38] = [
    "stage-json11",
    "build-protocol",
    "check-protocol",
    "build-safety",
    "check-safety",
    "check-safety_padding",
    "build-device",
    "check-device",
    "build-state",
    "check-state",
    "build-can-io",
    "check-can_io",
    "build-peripheral",
    "check-peripheral",
    "build-spi",
    "check-spi",
    "build-usb",
    "check-usb",
    "check-firmware",
    "check-client",
    "check-firmware_spi",
    "check-supervisor",
    "check-usb_policy",
    "check-usb_raw",
    "download-spidev",
    "build-spidev",
    "check-spidev",
    "build-runtime-fixture",
    "build-runtime-source",
    "check-runtime-source",
    "check-runtime-native",
    "compare-runtime",
    "build-supervisor-child",
    "check-native_supervisor",
    "check-supervisor_signals",
    "check-dfu_recovery",
    "check-composed-explicit",
    "check-composed-default",
];
const BINARIES: [&str; 13] = [
    "openpilot-pandad",
    "openpilot-pandad-supervisor",
    "openpilot-process-child",
    "openpilot-logmessaged",
    "stage-json11",
    "build-pandad-protocol-source",
    "build-pandad-peripheral-source",
    "build-pandad-spi-source",
    "build-pandad-usb-source",
    "build-pandad-runtime-fixture",
    "build-pandad-runtime-source",
    "build-pandad-firmware-spi-fixture",
    "check-pandad-safety-padding",
];
const EXAMPLES: [&str; 17] = [
    "panda_protocol",
    "panda_safety",
    "device",
    "state",
    "can_io",
    "peripheral",
    "firmware",
    "firmware_client",
    "firmware_spi",
    "supervisor",
    "supervisor_native",
    "dfu_recovery",
    "usb_policy",
    "spi_policy",
    "usb_fixture",
    "raw_fixture",
    "spi_fixture",
];

fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("missing mandatory {name}")))
}
fn sha(path: &Path) -> String {
    held_sha(&mut File::open(path).unwrap())
}
fn held_sha(file: &mut File) -> String {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = file.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    format!("{:x}", digest.finalize())
}
struct HeldElf {
    file: File,
    identity: (u64, u64),
    digest: String,
}
impl HeldElf {
    fn open(path: &Path) -> Self {
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_file());
        let mut file = File::from(
            rustix::fs::open(
                path,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .unwrap(),
        );
        let metadata = file.metadata().unwrap();
        assert!(metadata.is_file());
        let named = fs::symlink_metadata(path).unwrap();
        assert!(named.file_type().is_file());
        assert_eq!((named.dev(), named.ino()), (metadata.dev(), metadata.ino()));
        let mut magic = [0u8; 6];
        file.read_exact(&mut magic).unwrap();
        assert_eq!(&magic, b"\x7fELF\x02\x01", "non-ELF: {}", path.display());
        let identity = (metadata.dev(), metadata.ino());
        let digest = held_sha(&mut file);
        Self {
            file,
            identity,
            digest,
        }
    }
    fn verify(&mut self, path: &Path) {
        let named = fs::symlink_metadata(path).unwrap();
        let held = self.file.metadata().unwrap();
        assert!(named.file_type().is_file() && held.is_file());
        assert_eq!(
            (named.dev(), named.ino()),
            self.identity,
            "ELF path replaced: {}",
            path.display()
        );
        assert_eq!((held.dev(), held.ino()), self.identity);
        assert_eq!(
            held_sha(&mut self.file),
            self.digest,
            "held ELF changed: {}",
            path.display()
        );
    }
}
fn absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("retired Python source reappeared: {}", path.display()),
        Err(error) => panic!("cannot establish source absence: {error}"),
    }
}
fn source_state(root: &Path, expect_absent: bool) {
    for (name, historical) in RETIRED.into_iter().zip(HISTORICAL) {
        let path = root.join(name);
        if expect_absent {
            absent(&path);
        } else {
            assert!(fs::symlink_metadata(&path).unwrap().file_type().is_file());
            assert_eq!(sha(&path), historical, "historical source changed: {name}");
        }
    }
}
fn elf_seals(checker: &Path, binaries: &Path) -> BTreeMap<PathBuf, HeldElf> {
    let paths = std::iter::once(checker.to_path_buf())
        .chain(BINARIES.into_iter().map(|name| binaries.join(name)))
        .chain(
            EXAMPLES
                .into_iter()
                .map(|name| binaries.join("examples").join(name)),
        );
    let mut seals = BTreeMap::new();
    for path in paths {
        seals.insert(path.clone(), HeldElf::open(&path));
    }
    assert_eq!(seals.len(), 31);
    seals
}
fn normalize(value: &str, root: &Path, checkout_root: &Path, output: &Path) -> String {
    value
        .replace(output.to_str().unwrap(), "<OUTPUT>")
        .replace(root.to_str().unwrap(), "<ROOT>")
        .replace(checkout_root.to_str().unwrap(), "<ROOT>")
}
#[derive(Clone, Copy)]
struct LaneInputs<'a> {
    root: &'a Path,
    checkout_root: &'a Path,
    checker: &'a Path,
    binaries: &'a Path,
    messages: &'a Path,
    python: &'a Path,
    evidence: &'a Path,
}
fn run_lane(
    name: &str,
    LaneInputs {
        root,
        checkout_root,
        checker,
        binaries,
        messages,
        python,
        evidence,
    }: LaneInputs<'_>,
) -> (Value, Vec<Value>, Value) {
    let output = evidence.join(format!("{name}-output"));
    let argv = vec![
        checker.to_str().unwrap().to_owned(),
        "--python".into(),
        python.to_str().unwrap().to_owned(),
        "--binaries".into(),
        binaries.to_str().unwrap().to_owned(),
        "--build-messages".into(),
        messages.to_str().unwrap().to_owned(),
        "--output".into(),
        output.to_str().unwrap().to_owned(),
    ];
    let mut spec = CommandSpec::new(argv, Duration::from_secs(5400));
    spec.cwd = Some(root.to_path_buf());
    spec.env.insert(
        "OPENPILOT_RUST_ROOT".into(),
        root.to_str().unwrap().to_owned(),
    );
    let captured = NativeRunner.run(&spec).unwrap();
    fs::write(evidence.join(format!("{name}.stdout")), &captured.stdout).unwrap();
    fs::write(evidence.join(format!("{name}.stderr")), &captured.stderr).unwrap();
    fs::write(
        evidence.join(format!("{name}.code")),
        captured.code.to_string(),
    )
    .unwrap();
    assert_eq!(
        captured.code,
        0,
        "{name}: {}",
        String::from_utf8_lossy(&captured.stderr)
    );
    assert!(
        captured.stderr.is_empty(),
        "{name}: unexpected controller stderr"
    );
    let mut passes = Vec::new();
    let mut guards = 0;
    let mut pending_growth = Vec::new();
    for line in std::str::from_utf8(&captured.stdout).unwrap().lines() {
        if let Some(phase) = line.strip_prefix("PASS ") {
            let expected: &[u64] = match phase {
                "stage-json11" => &[2048, 256, 256],
                "download-spidev" | "build-supervisor-child" => &[64],
                "build-spidev" | "check-usb_raw" => &[64, 512],
                _ => &[512],
            };
            assert_eq!(
                pending_growth, expected,
                "{name}: {phase} reservation order"
            );
            pending_growth.clear();
            passes.push(phase);
        } else {
            let row: Value = serde_json::from_str(line).unwrap();
            assert_eq!(row.as_object().unwrap().len(), 3);
            let growth = row["growth_bytes"].as_u64().unwrap();
            assert_eq!(
                row["required_bytes"].as_u64().unwrap(),
                25 * 1024_u64.pow(3) + growth
            );
            assert!(row["free_bytes"].as_u64().unwrap() >= row["required_bytes"].as_u64().unwrap());
            assert_eq!(growth % (1024 * 1024), 0);
            pending_growth.push(growth / (1024 * 1024));
            guards += 1;
        }
    }
    assert!(pending_growth.is_empty());
    assert_eq!(passes, ROSTER);
    assert_eq!(guards, 42);
    let result: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(result["status"], "PASS");
    let invocation: Value =
        serde_json::from_slice(&fs::read(output.join("invocation.json")).unwrap()).unwrap();
    assert_eq!(invocation["build_messages_sha256"], sha(messages));
    assert_eq!(invocation["binary_sha256"].as_object().unwrap().len(), 30);
    for (path, digest) in invocation["source_sha256"].as_object().unwrap() {
        let source = root.join(path);
        assert!(source.is_file(), "{name}: source disappeared: {path}");
        assert_eq!(
            sha(&source),
            digest.as_str().unwrap(),
            "{name}: source changed: {path}"
        );
    }
    for (path, digest) in invocation["binary_sha256"].as_object().unwrap() {
        assert_eq!(
            sha(Path::new(path)),
            digest.as_str().unwrap(),
            "{name}: executable changed: {path}"
        );
    }
    let mut commands = Vec::new();
    for phase in ROSTER {
        let prefix = output.join("commands").join(phase);
        let command: Value =
            serde_json::from_slice(&fs::read(prefix.with_extension("command.json")).unwrap())
                .unwrap();
        let exit: Value =
            serde_json::from_slice(&fs::read(prefix.with_extension("exit.json")).unwrap()).unwrap();
        assert_eq!(exit["returncode"], 0, "{name}: {phase}");
        assert_eq!(command["cwd"], root.to_str().unwrap());
        let args = command["argv"].as_array().unwrap();
        assert!(!args.is_empty());
        assert!(args.iter().all(|arg| arg.as_str().is_some()));
        assert!(!args
            .iter()
            .any(|arg| arg.as_str().unwrap().ends_with("/check_pandad_ci.py")));
        for suffix in ["stdout", "stderr"] {
            assert!(
                prefix.with_extension(suffix).is_file(),
                "{name}: {phase}.{suffix}"
            );
        }
        commands.push(Value::Array(
            args.iter()
                .map(|arg| {
                    Value::String(normalize(
                        arg.as_str().unwrap(),
                        root,
                        checkout_root,
                        &output,
                    ))
                })
                .collect(),
        ));
    }
    assert_eq!(
        fs::read_dir(output.join("commands")).unwrap().count(),
        38 * 4
    );
    (invocation, commands, result)
}

#[test]
fn same_elf_checkout_and_private_checker_absent_roots_complete_all_original_phases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let private = required("PANDA_CI_PRIVATE_ROOT").canonicalize().unwrap();
    assert_ne!(private, repo);
    let checker = required("PANDA_CI_CHECKER_BINARY").canonicalize().unwrap();
    let binaries = required("PANDA_CI_BINARIES").canonicalize().unwrap();
    let messages = required("PANDA_CI_BUILD_MESSAGES").canonicalize().unwrap();
    let python = required("PANDA_CI_PYTHON").canonicalize().unwrap();
    let evidence = required("PANDA_CI_RETIREMENT_EVIDENCE");
    fs::create_dir(&evidence).unwrap();
    let expect_absent = match env::var("PANDA_CI_EXPECT_ABSENT").unwrap().as_str() {
        "0" => false,
        "1" => true,
        other => panic!("invalid source presence contract: {other}"),
    };
    source_state(&repo, expect_absent);
    source_state(&private, true);
    let entry = Path::new("rust/crates/dev-tools/src/bin/check_pandad_ci.rs");
    assert!(repo.join(entry).is_file() && private.join(entry).is_file());
    let entry_sha = sha(&repo.join(entry));
    assert_eq!(sha(&private.join(entry)), entry_sha);
    let mut seals = elf_seals(&checker, &binaries);
    let shared = LaneInputs {
        root: &repo,
        checkout_root: &repo,
        checker: &checker,
        binaries: &binaries,
        messages: &messages,
        python: &python,
        evidence: &evidence,
    };
    let (checkout, checkout_commands, checkout_result) = run_lane("checkout", shared);
    source_state(&repo, expect_absent);
    source_state(&private, true);
    let (absent_lane, absent_commands, private_result) = run_lane(
        "private",
        LaneInputs {
            root: &private,
            ..shared
        },
    );
    source_state(&repo, expect_absent);
    source_state(&private, true);
    assert_eq!(
        checkout_commands, absent_commands,
        "same-ELF phase argv changed"
    );
    assert_eq!(
        checkout_result, private_result,
        "same-ELF final receipt changed"
    );
    let current_sources = checkout["source_sha256"].as_object().unwrap();
    let absent_sources = absent_lane["source_sha256"].as_object().unwrap();
    let shared = "rust/crates/dev-tools/src/python_fs.rs";
    let shared_sha = sha(&repo.join(shared));
    assert_eq!(current_sources[shared].as_str(), Some(shared_sha.as_str()));
    assert_eq!(absent_sources[shared].as_str(), Some(shared_sha.as_str()));
    assert_eq!(
        current_sources.len(),
        absent_sources.len() + usize::from(!expect_absent)
    );
    for (name, hash) in absent_sources {
        assert_eq!(
            current_sources.get(name),
            Some(hash),
            "source changed: {name}"
        );
    }
    let historical = "rust/tools/check_pandad_ci.py";
    assert!(!absent_sources.contains_key(historical));
    if expect_absent {
        assert!(!current_sources.contains_key(historical));
    } else {
        assert_eq!(current_sources[historical], HISTORICAL[0]);
    }
    assert_eq!(sha(&repo.join(entry)), entry_sha);
    assert_eq!(sha(&private.join(entry)), entry_sha);
    assert_eq!(checkout["binary_sha256"], absent_lane["binary_sha256"]);
    assert_eq!(
        checkout["build_messages_sha256"],
        absent_lane["build_messages_sha256"]
    );
    for (path, seal) in &mut seals {
        seal.verify(path);
    }
    let digests = seals
        .into_iter()
        .map(|(path, seal)| (path.display().to_string(), seal.digest))
        .collect::<BTreeMap<_, _>>();
    fs::write(
        evidence.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed": true, "phases_per_lane": 38, "disk_receipts_per_lane": 42,
            "checker_absent_in_checkout": expect_absent, "current_elf_sha256": digests,
            "historical_checker_sha256": HISTORICAL[0],
            "historical_test_sha256": HISTORICAL[1],
        "native_entry_source_sha256": entry_sha,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn current_native_cli_preserves_sealed_original_admissions_and_side_effects() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let checker = required("PANDA_CI_CHECKER_BINARY").canonicalize().unwrap();
    let binaries = required("PANDA_CI_BINARIES").canonicalize().unwrap();
    let original = required("PANDA_CI_ORIGINAL_CLI_RECEIPT");
    assert_eq!(
        sha(&original),
        env::var("PANDA_CI_ORIGINAL_CLI_SHA256").unwrap()
    );
    let capture: Value = serde_json::from_slice(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(capture["source_sha256"], HISTORICAL[0]);
    assert_eq!(capture["test_sha256"], HISTORICAL[1]);
    let cases = capture["cases"].as_object().unwrap();
    let expected = BTreeMap::from([
        ("help", 0),
        ("short-help", 0),
        ("attached-short-help", 0),
        ("repeated-short-help", 0),
        ("help-with-value", 2),
        ("unknown", 2),
        ("missing-required", 2),
        ("raw-unused-before-help", 0),
        ("raw-missing-messages", 1),
        ("abbreviated-binaries-missing-messages", 1),
        ("repeated-output-missing-messages", 1),
    ]);
    assert_eq!(cases.len(), expected.len(), "original CLI corpus changed");
    let evidence = required("PANDA_CI_CLI_EVIDENCE");
    fs::create_dir(&evidence).unwrap();
    for (name, code) in expected {
        let row = &cases[name];
        assert_eq!(row["exit"], code, "original CLI admission changed: {name}");
        let encoded = row["argv_b64"].as_array().unwrap();
        assert!(encoded.len() >= 2);
        let mut arguments = encoded
            .iter()
            .map(|value| {
                std::ffi::OsString::from_vec(STANDARD.decode(value.as_str().unwrap()).unwrap())
            })
            .collect::<Vec<_>>();
        assert!(arguments[1]
            .to_string_lossy()
            .ends_with("/check_pandad_ci.py"));
        let mut native = vec![checker.as_os_str().to_owned()];
        native.extend(arguments.drain(2..));
        let execution = name.ends_with("missing-messages");
        let mut final_output = evidence.join(format!("{name}-output"));
        if name == "raw-missing-messages" {
            let mut bytes = final_output.into_os_string().into_vec();
            bytes.push(0xff);
            final_output = PathBuf::from(std::ffi::OsString::from_vec(bytes));
        }
        let first_output = evidence.join("repeated-first-output");
        if execution {
            let mut outputs = 0;
            for at in 1..native.len() - 1 {
                match native[at].to_str().unwrap_or("") {
                    "--bin" | "--binaries" => native[at + 1] = binaries.as_os_str().to_owned(),
                    "--build-messages" => {
                        native[at + 1] = evidence.join("missing-build.jsonl").into_os_string()
                    }
                    "--output" => {
                        outputs += 1;
                        native[at + 1] =
                            if name == "repeated-output-missing-messages" && outputs == 1 {
                                first_output.as_os_str().to_owned()
                            } else {
                                final_output.as_os_str().to_owned()
                            };
                    }
                    _ => {}
                }
            }
            assert_eq!(
                outputs,
                if name.starts_with("repeated-output") {
                    2
                } else {
                    1
                }
            );
        }
        let mut spec = CommandSpec::new_os(native, Duration::from_secs(60));
        spec.cwd = Some(repo.clone());
        spec.env.insert(
            "OPENPILOT_RUST_ROOT".into(),
            repo.to_str().unwrap().to_owned(),
        );
        let observed = NativeRunner.run(&spec).unwrap();
        fs::write(evidence.join(format!("{name}.stdout")), &observed.stdout).unwrap();
        fs::write(evidence.join(format!("{name}.stderr")), &observed.stderr).unwrap();
        fs::write(
            evidence.join(format!("{name}.code")),
            observed.code.to_string(),
        )
        .unwrap();
        assert_eq!(observed.code, code, "native CLI exit differs: {name}");
        let source_stdout = STANDARD
            .decode(row["stdout_b64"].as_str().unwrap())
            .unwrap();
        let source_stderr = STANDARD
            .decode(row["stderr_b64"].as_str().unwrap())
            .unwrap();
        if code == 0 {
            assert!(!source_stdout.is_empty() && !observed.stdout.is_empty());
            assert!(source_stderr.is_empty() && observed.stderr.is_empty());
            assert!(!final_output.exists(), "help created output: {name}");
        } else if code == 2 {
            assert!(source_stdout.is_empty() && observed.stdout.is_empty());
            assert!(!source_stderr.is_empty() && !observed.stderr.is_empty());
            assert!(
                !final_output.exists(),
                "argument error created output: {name}"
            );
        } else {
            assert_eq!(code, 1);
            assert_eq!(row["output_exists"], true);
            assert_eq!(row["result_exists"], false);
            assert!(final_output.is_dir(), "selected output not created: {name}");
            assert!(!final_output.join("result.json").exists());
            assert!(!final_output.join("invocation.json").exists());
            assert!(!observed.stderr.is_empty() && !source_stderr.is_empty());
            let source_line = std::str::from_utf8(&source_stdout).unwrap();
            let native_line = std::str::from_utf8(&observed.stdout).unwrap();
            let source_reservation: Value = serde_json::from_str(source_line).unwrap();
            let native_reservation: Value = serde_json::from_str(native_line).unwrap();
            assert_eq!(source_reservation["growth_bytes"], 2048 * 1024 * 1024_u64);
            assert_eq!(
                native_reservation["growth_bytes"],
                source_reservation["growth_bytes"]
            );
            assert_eq!(
                native_reservation["required_bytes"],
                source_reservation["required_bytes"]
            );
            assert!(!first_output.exists(), "earlier repeated output was used");
        }
    }
}
