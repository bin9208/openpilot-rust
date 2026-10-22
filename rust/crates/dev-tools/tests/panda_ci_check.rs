//! Source-only contracts for the full original Panda host orchestration roster.
#![cfg(all(target_os = "linux", feature = "panda-ci-native"))]
use openpilot_dev_tools::{
    panda_ci_check::{self, Command, Options},
    python_fs,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
    path::{Path, PathBuf},
};

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
fn assert_stdout_receipts(
    stdout: &[u8],
    passes: &[&str],
    expected_growth_mib: &[(u64, usize)],
    failed_after_guard: bool,
) {
    let mut seen = Vec::new();
    let mut growths = BTreeMap::<u64, usize>::new();
    let mut pending_guards = Vec::new();
    for line in std::str::from_utf8(stdout).unwrap().lines() {
        if let Some(name) = line.strip_prefix("PASS ") {
            let expected: &[u64] = match name {
                "stage-json11" => &[2048, 256, 256],
                "download-spidev" | "build-supervisor-child" => &[64],
                "build-spidev" | "check-usb_raw" => &[64, 512],
                _ => &[512],
            };
            assert_eq!(
                pending_guards.as_slice(),
                expected,
                "{name}: disk reservations differ"
            );
            seen.push(name);
            pending_guards.clear();
            continue;
        }
        let receipt: Value = serde_json::from_str(line).unwrap();
        let object = receipt.as_object().unwrap();
        assert_eq!(object.len(), 3, "unexpected disk receipt fields");
        let free = receipt["free_bytes"].as_u64().unwrap();
        let required = receipt["required_bytes"].as_u64().unwrap();
        let growth = receipt["growth_bytes"].as_u64().unwrap();
        assert_eq!(required, growth);
        assert!(free >= required, "successful phase had insufficient disk");
        assert_eq!(growth % (1024 * 1024), 0);
        *growths.entry(growth / (1024 * 1024)).or_default() += 1;
        pending_guards.push(growth / (1024 * 1024));
    }
    assert_eq!(seen, passes);
    assert_eq!(
        pending_guards,
        if failed_after_guard {
            vec![512]
        } else {
            vec![]
        }
    );
    assert_eq!(
        growths,
        expected_growth_mib
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>()
    );
}
fn assert_only_reservations(stdout: &[u8], expected_growth_mib: &[u64]) {
    let lines = std::str::from_utf8(stdout)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), expected_growth_mib.len());
    for (line, expected) in lines.iter().zip(expected_growth_mib) {
        let receipt: Value = serde_json::from_str(line).unwrap();
        assert_eq!(receipt.as_object().unwrap().len(), 3);
        assert_eq!(receipt["growth_bytes"], expected * 1024 * 1024);
        assert_eq!(receipt["required_bytes"], expected * 1024 * 1024);
        assert!(
            receipt["free_bytes"].as_u64().unwrap() >= receipt["required_bytes"].as_u64().unwrap()
        );
    }
}
fn put(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn tar_bytes(member: &str) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(3);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, member, &b"C++"[..]).unwrap();
    tar.into_inner().unwrap()
}
struct Fixture {
    _temp: tempfile::TempDir,
    options: Options,
    archive: Vec<u8>,
}
impl Fixture {
    fn new(member: &str, correct_hash: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        for name in ["pandad", "panda-usb", "panda-spi", "panda-spi-linux"] {
            fs::create_dir_all(root.join("rust/crates").join(name)).unwrap();
        }
        fs::create_dir_all(root.join("rust/tools")).unwrap();
        fs::create_dir_all(root.join("rust/crates/dev-tools/src/bin")).unwrap();
        let binaries = temp.path().join("bins");
        for name in [
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
            "check-pandad-device",
            "check-pandad-protocol",
            "check-pandad-can-io",
        ] {
            put(&binaries.join(name), b"sealed fixture executable");
        }
        for name in [
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
        ] {
            put(
                &binaries.join("examples").join(name),
                b"sealed example executable",
            );
        }
        let archive = tar_bytes(member);
        let digest = format!("{:x}", Sha256::digest(&archive));
        let digest = if correct_hash { digest } else { "0".repeat(64) };
        put(
            &root.join("uv.lock"),
            format!(
                "[[package]]\nname = 'spidev'\nversion = '3.6'\n[package.sdist]\nurl = 'https://example.invalid/spidev.tar'\nhash = 'sha256:{digest}'\n"
            ),
        );
        let zmq = temp.path().join("zmq");
        put(&zmq.join("source/include/zmq.h"), b"header");
        put(&zmq.join("lib/libzmq.a"), b"archive");
        let messages = temp.path().join("messages.jsonl");
        put(
            &messages,
            format!(
                "{{\"reason\":\"build-script-executed\",\"package_id\":\"zmq-sys 0.10\",\"out_dir\":\"{}\"}}\n",
                zmq.display()
            ),
        );
        let options = Options {
            root,
            binaries,
            build_messages: messages,
            output: temp.path().join("proof"),
            python: "python3".into(),
        };
        Self {
            _temp: temp,
            options,
            archive,
        }
    }
}
struct Fake {
    names: Vec<String>,
    receipt_output: Option<PathBuf>,
    fail_at: Option<String>,
    archive: Vec<u8>,
    stage_stdout: Option<Vec<u8>>,
    invalid_stream: Option<&'static str>,
    overflow: bool,
    metadata: Option<Vec<u8>>,
}
impl Fake {
    fn new(archive: Vec<u8>) -> Self {
        Self {
            names: Vec::new(),
            receipt_output: None,
            fail_at: None,
            archive,
            stage_stdout: None,
            invalid_stream: None,
            overflow: false,
            metadata: None,
        }
    }
}
impl Runner for Fake {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let argv = spec.effective_argv();
        if argv
            .iter()
            .any(|s| s.to_string_lossy().contains("sysconfig,pathlib,usb1"))
        {
            return Ok(Captured { code: 0, stdout: self.metadata.clone().unwrap_or_else(|| br#"{"include":"/usr/include/python3.12","suffix":".so","usb1":"/usr/lib/python3.12/site-packages/usb1.py"}"#.to_vec()), stderr: vec![] });
        }
        let cwd = spec.cwd.as_ref().unwrap();
        let output = self
            .receipt_output
            .clone()
            .unwrap_or_else(|| cwd.parent().unwrap().join("proof"));
        let files = output.join("commands");
        let name = fs::read_dir(&files)?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_name()?.to_str()?;
                name.strip_suffix(".command.json").map(str::to_owned)
            })
            .find(|name| !self.names.contains(name))
            .ok_or("missing command receipt")?;
        self.names.push(name.clone());
        if name == "download-spidev" {
            let pos = argv
                .iter()
                .position(|arg| arg == "-o")
                .ok_or("missing curl output")?;
            put(Path::new(&argv[pos + 1]), &self.archive);
        }
        let mut stdout = if name == "stage-json11" {
            self.stage_stdout.clone().unwrap_or_else(|| {
                format!(
                    "{{\"prefix\":\"{}\",\"dependency\":{{\"sha256\":\"fixture\"}}}}",
                    output.join("json11-dependency/prefix").display()
                )
                .into_bytes()
            })
        } else {
            vec![]
        };
        if name == "check-safety_padding" {
            if self.invalid_stream == Some("stdout") {
                stdout = vec![0xff];
            }
            if self.overflow {
                stdout = vec![b'x'; spec.stdout_limit + 1];
            }
        }
        Ok(Captured {
            code: i32::from(self.fail_at.as_deref() == Some(&name)),
            stdout,
            stderr: if name == "check-safety_padding" && self.invalid_stream == Some("stderr") {
                vec![0xff]
            } else if self.fail_at.as_deref() == Some(&name) {
                b"original child error\n".to_vec()
            } else {
                vec![]
            },
        })
    }
}
#[test]
fn original_roster_receipts_and_final_publication_are_complete() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let mut runner = Fake::new(fixture.archive.clone());
    let mut cli_stdout = Vec::new();
    let mut cli_stderr = Vec::new();
    let result = panda_ci_check::run_with_output(
        &fixture.options,
        &mut runner,
        &mut cli_stdout,
        &mut cli_stderr,
    )
    .unwrap();
    assert_eq!(result["status"], "PASS");
    assert_eq!(
        runner.names.iter().map(String::as_str).collect::<Vec<_>>(),
        ROSTER
    );
    let proof = &fixture.options.output;
    assert_eq!(
        fs::read_dir(proof.join("commands")).unwrap().count(),
        38 * 4
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(proof.join("result.json")).unwrap()).unwrap(),
        result
    );
    assert!(proof.join("invocation.json").is_file());
    assert!(proof.join("python-dependency.json").is_file());
    assert!(proof.join("spidev-dependency.json").is_file());
    assert_stdout_receipts(
        &cli_stdout,
        &ROSTER,
        &[(2048, 1), (256, 2), (512, 35), (64, 4)],
        false,
    );
    assert!(cli_stderr.is_empty());
}
#[test]
fn first_failed_phase_stops_and_cannot_publish_pass() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let mut runner = Fake::new(fixture.archive.clone());
    runner.fail_at = Some("check-safety_padding".into());
    let mut cli_stdout = Vec::new();
    let mut cli_stderr = Vec::new();
    assert!(panda_ci_check::run_with_output(
        &fixture.options,
        &mut runner,
        &mut cli_stdout,
        &mut cli_stderr
    )
    .is_err());
    assert_eq!(
        runner.names.iter().map(String::as_str).collect::<Vec<_>>(),
        ROSTER[..6]
    );
    assert!(!fixture.options.output.join("result.json").exists());
    let exit: Value = serde_json::from_slice(
        &fs::read(
            fixture
                .options
                .output
                .join("commands/check-safety_padding.exit.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(exit["returncode"], 1);
    assert_stdout_receipts(
        &cli_stdout,
        &ROSTER[..5],
        &[(2048, 1), (256, 2), (512, 5)],
        true,
    );
    assert_eq!(cli_stderr, b"original child error\n");
}
#[test]
fn text_capture_rejects_invalid_stdout_or_stderr_before_success_receipt() {
    for stream in ["stdout", "stderr"] {
        let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
        let mut runner = Fake::new(fixture.archive.clone());
        runner.invalid_stream = Some(stream);
        let mut cli_stdout = Vec::new();
        let mut cli_stderr = Vec::new();
        assert!(panda_ci_check::run_with_output(
            &fixture.options,
            &mut runner,
            &mut cli_stdout,
            &mut cli_stderr
        )
        .is_err());
        let commands = fixture.options.output.join("commands");
        assert!(commands
            .join(format!("check-safety_padding.{stream}"))
            .is_file());
        assert!(commands.join("check-safety_padding.failure.json").is_file());
        assert!(!commands.join("check-safety_padding.exit.json").exists());
        assert!(!fixture.options.output.join("result.json").exists());
        assert_stdout_receipts(
            &cli_stdout,
            &ROSTER[..5],
            &[(2048, 1), (256, 2), (512, 5)],
            true,
        );
    }
}
#[test]
fn noisy_child_hits_active_stream_limit_without_pass() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let mut runner = Fake::new(fixture.archive.clone());
    runner.overflow = true;
    let mut cli_stdout = Vec::new();
    let mut cli_stderr = Vec::new();
    let error = panda_ci_check::run_with_output(
        &fixture.options,
        &mut runner,
        &mut cli_stdout,
        &mut cli_stderr,
    )
    .unwrap_err();
    assert!(error.to_string().contains("stdout"));
    let commands = fixture.options.output.join("commands");
    assert!(commands.join("check-safety_padding.command.json").is_file());
    assert!(commands.join("check-safety_padding.failure.json").is_file());
    assert!(commands.join("check-safety_padding.stdout").is_file());
    assert!(!commands.join("check-safety_padding.exit.json").exists());
    assert!(!fixture.options.output.join("result.json").exists());
    assert_stdout_receipts(
        &cli_stdout,
        &ROSTER[..5],
        &[(2048, 1), (256, 2), (512, 5)],
        true,
    );
}
#[test]
fn source_symlink_to_regular_file_is_hashed_under_original_glob_name() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let tools = fixture.options.root.join("rust/tools");
    put(&tools.join("pandad_source.py"), b"original bytes\n");
    put(
        &fixture
            .options
            .root
            .join("rust/crates/dev-tools/src/python_fs.rs"),
        b"shared filesystem codec\n",
    );
    symlink("pandad_source.py", tools.join("check_pandad_link.py")).unwrap();
    let mut runner = Fake::new(fixture.archive.clone());
    panda_ci_check::run_with_output(
        &fixture.options,
        &mut runner,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    let invocation: Value =
        serde_json::from_slice(&fs::read(fixture.options.output.join("invocation.json")).unwrap())
            .unwrap();
    let expected = format!("{:x}", Sha256::digest(b"original bytes\n"));
    assert_eq!(
        invocation["source_sha256"]["rust/tools/check_pandad_link.py"],
        expected
    );
    assert_eq!(
        invocation["source_sha256"]["rust/crates/dev-tools/src/python_fs.rs"],
        format!("{:x}", Sha256::digest(b"shared filesystem codec\n"))
    );
}
#[test]
fn malformed_json11_stage_receipt_stops_before_build() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let mut runner = Fake::new(fixture.archive.clone());
    runner.stage_stdout = Some(b"not JSON".to_vec());
    assert!(panda_ci_check::run_with(&fixture.options, &mut runner).is_err());
    assert_eq!(
        runner.names.iter().map(String::as_str).collect::<Vec<_>>(),
        ["stage-json11"]
    );
    assert!(!fixture.options.output.join("result.json").exists());
}
#[test]
fn raw_json11_prefix_reaches_next_child_without_replacement() {
    let mut fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    fixture.options.output = fixture
        ._temp
        .path()
        .join(OsString::from_vec(b"proof-\xff".to_vec()));
    let prefix = fixture.options.output.join("json11-dependency/prefix");
    let mut runner = Fake::new(fixture.archive.clone());
    runner.receipt_output = Some(fixture.options.output.clone());
    runner.stage_stdout = Some(
        format!(
            "{{\"prefix\":{},\"dependency\":{{\"sha256\":\"fixture\"}}}}",
            python_fs::json_string(prefix.as_os_str()).unwrap()
        )
        .into_bytes(),
    );
    runner.fail_at = Some("build-protocol".into());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let error =
        panda_ci_check::run_with_output(&fixture.options, &mut runner, &mut stdout, &mut stderr)
            .unwrap_err();
    assert!(error.to_string().contains("build-protocol exited 1"));
    assert_eq!(runner.names, ["stage-json11", "build-protocol"]);
    let commands = fixture.options.output.join("commands");
    let stage_exit: Value =
        serde_json::from_slice(&fs::read(commands.join("stage-json11.exit.json")).unwrap())
            .unwrap();
    assert_eq!(stage_exit["returncode"], 0);
    let next = fs::read_to_string(commands.join("build-protocol.command.json")).unwrap();
    assert!(next.contains(r"proof-\udcff/json11-dependency/prefix"));
    assert!(openpilot_log_format::JsonValue::parse(&next).is_ok());
    assert!(!fixture.options.output.join("result.json").exists());
}
#[test]
fn spidev_hash_and_missing_original_module_fail_before_build() {
    for (member, correct_hash) in [
        ("spidev-3.6/spidev_module.c", false),
        ("spidev-3.6/other.c", true),
    ] {
        let fixture = Fixture::new(member, correct_hash);
        let mut runner = Fake::new(fixture.archive.clone());
        assert!(panda_ci_check::run_with(&fixture.options, &mut runner).is_err());
        assert_eq!(
            runner.names.last().map(String::as_str),
            Some("download-spidev")
        );
        assert!(!fixture.options.output.join("result.json").exists());
        assert!(!fixture.options.output.join("spidev-src").exists());
    }
}
#[test]
fn zmq_must_have_one_complete_source_output() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let messages = &fixture.options.build_messages;
    put(
        messages,
        b"{\"reason\":\"build-script-executed\",\"package_id\":\"other\",\"out_dir\":\"/tmp\"}\n",
    );
    let mut runner = Fake::new(fixture.archive.clone());
    assert!(panda_ci_check::run_with(&fixture.options, &mut runner).is_err());
    assert!(runner.names.is_empty());
    assert!(!fixture.options.output.join("result.json").exists());
    let zmq = fixture._temp.path().join("another-zmq");
    put(&zmq.join("source/include/zmq.h"), b"header");
    put(&zmq.join("lib/libzmq.a"), b"archive");
    let original = fixture._temp.path().join("zmq");
    put(
        messages,
        format!(
            "{{\"reason\":\"build-script-executed\",\"package_id\":\"zmq-sys 0.10\",\"out_dir\":\"{}\"}}\n{{\"reason\":\"build-script-executed\",\"package_id\":\"zmq-sys 0.10\",\"out_dir\":\"{}\"}}\n",
            original.display(),
            zmq.display()
        ),
    );
    assert!(panda_ci_check::zmq_output(messages).is_err());
}
#[test]
fn zmq_uses_only_executed_build_output_not_compiler_artifact_or_stale_cache() {
    let temp = tempfile::tempdir().unwrap();
    let selected = temp.path().join("executed output");
    let stale = temp.path().join("stale output");
    for root in [&selected, &stale] {
        put(&root.join("source/include/zmq.h"), b"fixture header");
        put(&root.join("lib/libzmq.a"), b"fixture archive");
    }
    let messages = temp.path().join("build.jsonl");
    put(
        &messages,
        format!(
            "{{\"reason\":\"build-script-executed\",\"package_id\":\"registry+example#zmq-sys@0.12.0\",\"out_dir\":\"{}\"}}\n{{\"reason\":\"compiler-artifact\",\"package_id\":\"registry+example#zmq-sys@0.12.0\",\"out_dir\":\"{}\"}}\n{{\"reason\":\"build-finished\",\"success\":true}}\n",
            selected.display(),
            stale.display()
        ),
    );
    assert_eq!(
        panda_ci_check::zmq_output(&messages).unwrap(),
        selected.canonicalize().unwrap()
    );
    fs::remove_file(selected.join("lib/libzmq.a")).unwrap();
    assert!(panda_ci_check::zmq_output(&messages).is_err());
}
fn command_argv(proof: &Path, name: &str) -> Vec<String> {
    let receipt: Value = serde_json::from_slice(
        &fs::read(proof.join("commands").join(format!("{name}.command.json"))).unwrap(),
    )
    .unwrap();
    serde_json::from_value(receipt["argv"].clone()).unwrap()
}
#[test]
fn spidev_comparison_uses_fresh_output_active_abi_and_native_json11_stage() {
    let fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    let mut runner = Fake::new(fixture.archive.clone());
    runner.metadata = Some(
        format!(
            "{{\"include\":\"{}\",\"suffix\":\".active-python-abi.so\",\"usb1\":\"{}\"}}",
            fixture._temp.path().join("active python headers").display(),
            fixture._temp.path().join("usb1.py").display()
        )
        .into_bytes(),
    );
    panda_ci_check::run_with_output(
        &fixture.options,
        &mut runner,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    let proof = &fixture.options.output;
    let stage = command_argv(proof, "stage-json11");
    assert_eq!(
        stage,
        [
            fixture
                .options
                .binaries
                .join("stage-json11")
                .to_string_lossy()
                .into_owned(),
            "--root".into(),
            fixture.options.root.to_string_lossy().into_owned(),
            "--output".into(),
            proof
                .join("json11-dependency")
                .to_string_lossy()
                .into_owned()
        ]
    );
    let staged: Value =
        serde_json::from_slice(&fs::read(proof.join("json11-dependency.json")).unwrap()).unwrap();
    assert_eq!(staged, serde_json::json!({"sha256":"fixture"}));
    let padding = command_argv(proof, "check-safety_padding");
    assert_eq!(
        padding,
        [
            fixture
                .options
                .binaries
                .join("check-pandad-safety-padding")
                .to_string_lossy()
                .into_owned(),
            "--source".into(),
            proof
                .join("safety-src/pandad-protocol-source")
                .to_string_lossy()
                .into_owned(),
            "--binary".into(),
            fixture
                .options
                .binaries
                .join("examples/panda_safety")
                .to_string_lossy()
                .into_owned(),
            "--output".into(),
            proof.join("safety_padding").to_string_lossy().into_owned()
        ]
    );
    let build = command_argv(proof, "build-spidev");
    let comparison = command_argv(proof, "check-spidev");
    let value = |argv: &[String], flag: &str| -> String {
        let i = argv.iter().position(|arg| arg == flag).unwrap();
        argv[i + 1].clone()
    };
    assert_eq!(
        value(&build, "--output"),
        proof.join("spidev-src").to_string_lossy().into_owned()
    );
    assert_eq!(
        value(&build, "--python-include"),
        fixture
            ._temp
            .path()
            .join("active python headers")
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(value(&build, "--extension-suffix"), ".active-python-abi.so");
    assert_eq!(
        value(&build, "--root"),
        fixture.options.root.to_string_lossy().into_owned()
    );
    assert_eq!(
        value(&build, "--json11-prefix"),
        proof
            .join("json11-dependency/prefix")
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(
        value(&build, "--spidev-source"),
        proof
            .join("spidev-source/spidev-3.6")
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(
        value(&comparison, "--output"),
        proof.join("spidev").to_string_lossy().into_owned()
    );
    assert_eq!(
        Path::new(&value(&comparison, "--library"))
            .parent()
            .unwrap(),
        proof.join("spidev-src")
    );
    assert_ne!(value(&build, "--output"), value(&comparison, "--output"));
    assert_eq!(comparison[0], "python3");
    assert_eq!(
        comparison[1],
        fixture
            .options
            .root
            .join("rust/tools/check_pandad_spidev.py")
            .to_string_lossy()
            .into_owned()
    );
    for (phase, binary, flag, example) in [
        ("device", "check-pandad-device", "--native", "device"),
        (
            "protocol",
            "check-pandad-protocol",
            "--binary",
            "panda_protocol",
        ),
        ("can_io", "check-pandad-can-io", "--native", "can_io"),
    ] {
        let command_name = format!("check-{phase}");
        let argv = command_argv(proof, &command_name);
        assert_eq!(Path::new(&argv[0]), fixture.options.binaries.join(binary));
        assert_eq!(
            Path::new(&value(&argv, flag)),
            fixture.options.binaries.join("examples").join(example)
        );
        assert_eq!(Path::new(&value(&argv, "--output")), proof.join(phase));
        let receipt: Value = serde_json::from_slice(
            &fs::read(
                proof
                    .join("commands")
                    .join(format!("{command_name}.command.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            receipt["native_source_root"],
            fixture.options.root.to_string_lossy().as_ref()
        );
    }
    let native_builds = runner
        .names
        .iter()
        .filter_map(|name| {
            if name.starts_with("build-")
                || name == "stage-json11"
                || name == "check-safety_padding"
            {
                Some(command_argv(proof, name)[0].clone())
            } else {
                None
            }
        })
        .take(11)
        .collect::<Vec<_>>();
    let expected = [
        "stage-json11",
        "build-pandad-protocol-source",
        "build-pandad-protocol-source",
        "check-pandad-safety-padding",
        "build-pandad-protocol-source",
        "build-pandad-protocol-source",
        "build-pandad-protocol-source",
        "build-pandad-peripheral-source",
        "build-pandad-spi-source",
        "build-pandad-usb-source",
        "build-pandad-firmware-spi-fixture",
    ]
    .map(|name| {
        fixture
            .options
            .binaries
            .join(name)
            .to_string_lossy()
            .into_owned()
    });
    assert_eq!(native_builds, expected.to_vec());
}
#[test]
fn cli_keeps_three_required_inputs_and_explicit_python_provider() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        panda_ci_check::parse(&[OsString::from("--help")], root.path()).unwrap(),
        Command::Help
    ));
    assert!(panda_ci_check::parse(
        &[OsString::from("--binaries"), OsString::from("x")],
        root.path()
    )
    .is_err());
    let args = [
        "--binaries",
        "x",
        "--build-messages",
        "m",
        "--output",
        "o",
        "--python",
        "/usr/bin/python3",
    ];
    let Command::Run(parsed) =
        panda_ci_check::parse(&args.map(OsString::from), root.path()).unwrap()
    else {
        panic!("not run")
    };
    assert_eq!(parsed.python, OsString::from("/usr/bin/python3"));
    assert_eq!(parsed.output, std::env::current_dir().unwrap().join("o"));
    let raw = OsString::from_vec(vec![b'x', 0xff]);
    assert!(matches!(
        panda_ci_check::parse(
            &[
                OsString::from("--output"),
                raw.clone(),
                OsString::from("--help")
            ],
            root.path()
        )
        .unwrap(),
        Command::Help
    ));
    let invalid = [
        OsString::from("--binaries"),
        raw,
        OsString::from("--build-messages"),
        OsString::from("m"),
        OsString::from("--output"),
        OsString::from("o"),
    ];
    let Command::Run(parsed) = panda_ci_check::parse(&invalid, root.path()).unwrap() else {
        panic!("raw argument was treated as help")
    };
    assert!(parsed.binaries.as_os_str().as_bytes().ends_with(b"/x\xff"));
}

#[test]
fn raw_output_path_reaches_first_child_with_surrogate_receipts_and_no_success() {
    struct FirstChild;
    impl Runner for FirstChild {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            let argv = spec.effective_argv();
            assert_eq!(argv.len(), 5);
            assert_eq!(argv[3], "--output");
            assert!(argv[4]
                .as_bytes()
                .ends_with(b"proof-\xff/json11-dependency"));
            Ok(Captured {
                code: 101,
                stdout: Vec::new(),
                stderr: b"original stage-json11 failed\n".to_vec(),
            })
        }
    }
    let mut fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    fixture.options.output = fixture
        ._temp
        .path()
        .join(OsString::from_vec(b"proof-\xff".to_vec()));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let error = panda_ci_check::run_with_output(
        &fixture.options,
        &mut FirstChild,
        &mut stdout,
        &mut stderr,
    )
    .unwrap_err();
    assert!(error.to_string().contains("stage-json11 exited 101"));
    assert_eq!(stderr, b"original stage-json11 failed\n");
    assert_only_reservations(&stdout, &[2048, 256, 256]);
    let proof = &fixture.options.output;
    assert!(proof.is_dir());
    assert!(!proof.join("result.json").exists());
    let command = fs::read_to_string(proof.join("commands/stage-json11.command.json")).unwrap();
    assert!(command.contains(r"proof-\udcff/json11-dependency"));
    assert!(openpilot_log_format::JsonValue::parse(&command).is_ok());
    let exit: Value =
        serde_json::from_slice(&fs::read(proof.join("commands/stage-json11.exit.json")).unwrap())
            .unwrap();
    assert_eq!(exit["returncode"], 101);
}

#[test]
fn raw_output_with_missing_messages_still_publishes_initial_guard_and_directory() {
    let mut fixture = Fixture::new("spidev-3.6/spidev_module.c", true);
    fixture.options.output = fixture
        ._temp
        .path()
        .join(OsString::from_vec(b"proof-\xff".to_vec()));
    fs::remove_file(&fixture.options.build_messages).unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    assert!(panda_ci_check::run_with_output(
        &fixture.options,
        &mut Fake::new(fixture.archive.clone()),
        &mut stdout,
        &mut stderr,
    )
    .is_err());
    assert_only_reservations(&stdout, &[2048]);
    assert!(fixture.options.output.is_dir());
    assert!(!fixture.options.output.join("invocation.json").exists());
    assert!(!fixture.options.output.join("result.json").exists());
}
