//! Pre-deletion, cloud-only original/present/absent getFloat evidence.
#![cfg(all(unix, feature = "calibration-float-native"))]
use openpilot_dev_tools::calibration_float_check as check;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const ORIGINAL_REPORT_SHA: &str =
    "c196cadfd49fa7ce417b4987ec2c03ce1d048d3d9c686fca1ecc0aebef24e6e2";
const ORIGINAL_STDOUT_SHA: &str =
    "28b4df9d49445c09ba53e17252ffa5ff4c7d300f0bde50e792d228a27d824b4e";
const ORIGINAL_CPP_SHA: &str = "71e73caa64a4dcc851edc57dc4167b38b01758ba9e16159e5a6a24fe430ce803";
// Historical original-only observation: source 60e3711e09d6b45b66a3f5ac30d79d6521f86e0c.

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("missing {name}")))
}
fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn json(path: &Path) -> Value {
    serde_json::from_slice(&bytes(path)).unwrap()
}
fn absent(path: &Path) {
    assert!(
        matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "must be genuinely absent: {}",
        path.display()
    );
}

#[test]
fn sealed_original_and_same_elf_present_absent_keep_all_25_rows_and_artifacts() {
    let checkout = required("CALIBRATION_RETIREMENT_CHECKOUT_ROOT");
    let absent_root = required("CALIBRATION_RETIREMENT_ABSENT_ROOT");
    let original = required("CALIBRATION_RETIREMENT_ORIGINAL_OUTPUT");
    let present = required("CALIBRATION_RETIREMENT_PRESENT_OUTPUT");
    let absent_output = required("CALIBRATION_RETIREMENT_ABSENT_OUTPUT");
    assert_eq!(
        fs::read_to_string(required("CALIBRATION_RETIREMENT_SOURCE_COMMIT"))
            .unwrap()
            .trim(),
        std::env::var("CALIBRATION_RETIREMENT_EXPECTED_SOURCE").unwrap()
    );
    assert!(
        checkout.join(check::CHECKER).is_file(),
        "live original must remain"
    );
    absent(&absent_root.join(check::CHECKER));
    let live = check::sources(&checkout).unwrap();
    let missing = check::sources(&absent_root).unwrap();
    assert_eq!(live["source_sha256"][check::CHECKER], check::CHECKER_SHA256);
    assert_eq!(
        missing["historical_source_sha256"][check::CHECKER],
        check::CHECKER_SHA256
    );
    assert!(missing["source_sha256"].get(check::CHECKER).is_none());
    for proof in [&live, &missing] {
        assert_eq!(proof["source_sha256"][check::HEADER], check::HEADER_SHA256);
    }

    let original_report_bytes = bytes(&original.join("report.json"));
    assert_eq!(
        sha(&original_report_bytes),
        ORIGINAL_REPORT_SHA,
        "full original-only report drift"
    );
    let expected: Value = serde_json::from_slice(&original_report_bytes).unwrap();
    assert_eq!(expected["result"], "pass");
    let rows = expected["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 25);
    for (row, input) in rows.iter().zip(check::inputs()) {
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["input_hex"], hex(input));
    }
    let original_stdout = bytes(&required("CALIBRATION_RETIREMENT_ORIGINAL_STDOUT"));
    assert_eq!(sha(&original_stdout), ORIGINAL_STDOUT_SHA);
    assert_eq!(original_stdout, check::SUCCESS.as_bytes());
    assert!(bytes(&required("CALIBRATION_RETIREMENT_ORIGINAL_STDERR")).is_empty());
    let cpp = bytes(&original.join("source_get_float.cc"));
    assert_eq!(sha(&cpp), ORIGINAL_CPP_SHA);
    let original_reference = bytes(&original.join("source-get-float"));
    assert!(!original_reference.is_empty());
    for (name, output, proof, stdout, stderr) in [
        (
            "present",
            &present,
            &live,
            "CALIBRATION_RETIREMENT_PRESENT_STDOUT",
            "CALIBRATION_RETIREMENT_PRESENT_STDERR",
        ),
        (
            "absent",
            &absent_output,
            &missing,
            "CALIBRATION_RETIREMENT_ABSENT_STDOUT",
            "CALIBRATION_RETIREMENT_ABSENT_STDERR",
        ),
    ] {
        assert_eq!(
            json(&output.join("report.json")),
            expected,
            "{name} full report"
        );
        assert_eq!(
            bytes(&output.join("source_get_float.cc")),
            cpp,
            "{name} generated C++"
        );
        assert_eq!(
            bytes(&output.join("source-get-float")),
            original_reference,
            "{name} reference ELF"
        );
        assert_eq!(
            bytes(&required(stdout)),
            original_stdout,
            "{name} CLI stdout"
        );
        assert!(bytes(&required(stderr)).is_empty(), "{name} CLI stderr");
        let provenance = json(&output.join("provenance.json"));
        assert_eq!(
            provenance["sources"], *proof,
            "{name} source classification"
        );
        assert_eq!(provenance["generated_source_sha256"], ORIGINAL_CPP_SHA);
        assert_eq!(
            provenance["binary_sha256"],
            sha(&bytes(&required("CALIBRATION_RETIREMENT_CANDIDATE")))
        );
        assert_eq!(provenance["reference_sha256"], sha(&original_reference));
        assert_eq!(
            provenance["compiler_sha256"],
            sha(&bytes(&required("CALIBRATION_RETIREMENT_COMPILER")))
        );
        assert_eq!(
            provenance["executing_checker_sha256"],
            sha(&bytes(&required("CALIBRATION_RETIREMENT_CHECKER")))
        );
        assert_eq!(
            provenance["compile_argv_bytes"][0],
            serde_json::json!([103, 43, 43])
        ); // g++ argv[0]
        for index in 0..25 {
            for stream in [
                "source.stdout",
                "source.stderr",
                "native.stdout",
                "native.stderr",
            ] {
                let file = format!("{index:02}-{stream}");
                assert_eq!(
                    bytes(&present.join(&file)),
                    bytes(&absent_output.join(&file)),
                    "{file} differs across roots"
                );
            }
        }
    }
    absent(&absent_root.join(check::CHECKER));
    assert_eq!(check::sources(&checkout).unwrap(), live);
    assert_eq!(check::sources(&absent_root).unwrap(), missing);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn seal_map(path: &Path) -> BTreeMap<String, String> {
    serde_json::from_slice(&bytes(path)).unwrap()
}
fn audit_exec(
    trace: &Path,
    output: &Path,
    checker: &Path,
    candidate: &Path,
    compiler: &Path,
) -> BTreeMap<String, String> {
    let raw = fs::read_to_string(trace).unwrap();
    let opening = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    let invocation =
        regex::Regex::new(r#"\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut helpers = BTreeMap::new();
    let mut failed_helper_lookups = 0;
    let checker = checker.to_str().unwrap();
    let candidate = candidate.to_str().unwrap();
    let compiler = compiler.to_str().unwrap();
    let reference = output.join("source-get-float");
    let reference = reference.to_str().unwrap();
    for line in raw.lines().filter(|line| opening.is_match(line)) {
        assert!(!line.contains("execveat("), "opaque execveat");
        if !line.ends_with("= 0") {
            assert!(line.contains("ENOENT"), "non-ENOENT exec failure: {line}");
            let path = line
                .split_once("execve(\"")
                .unwrap()
                .1
                .split_once('"')
                .unwrap()
                .0;
            let name = Path::new(path).file_name().unwrap().to_str().unwrap();
            assert!(
                ["cc1plus", "as", "collect2", "ld"].contains(&name),
                "non-helper failed exec: {line}"
            );
            assert!(
                path.starts_with("/usr/bin/")
                    || path.starts_with("/bin/")
                    || path.starts_with("/usr/lib/gcc/"),
                "helper search outside toolchain: {line}"
            );
            failed_helper_lookups += 1;
            assert!(
                failed_helper_lookups <= 16,
                "unbounded compiler helper search"
            );
            continue;
        }
        let capture = invocation
            .captures(line)
            .expect("opaque successful executable attempt");
        let program: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        let role = if program == checker {
            assert_eq!(
                argv,
                vec![
                    checker.to_owned(),
                    "--binary".into(),
                    candidate.to_owned(),
                    "--output".into(),
                    output.to_str().unwrap().into()
                ]
            );
            "checker"
        } else if program == compiler {
            assert_eq!(argv.len(), 6);
            assert_eq!(argv[0], "g++");
            assert_eq!(argv[1], "-std=c++17");
            assert_eq!(argv[2], "-O2");
            assert_eq!(
                argv[3],
                output.join("source_get_float.cc").to_str().unwrap()
            );
            assert_eq!(argv[4], "-o");
            assert_eq!(argv[5], reference);
            "compiler"
        } else if program == reference || program == candidate {
            assert_eq!(argv, vec![program.clone()]);
            if program == reference {
                "reference"
            } else {
                "candidate"
            }
        } else {
            let name = Path::new(&program).file_name().unwrap().to_str().unwrap();
            assert!(
                ["cc1plus", "as", "collect2", "ld"].contains(&name),
                "unrecognized compiler child {program}"
            );
            assert!(
                program.starts_with("/usr/bin/") || program.starts_with("/usr/lib/gcc/"),
                "compiler child outside toolchain: {program}"
            );
            assert!(Path::new(&program).is_file());
            helpers.insert(program.clone(), sha(&bytes(Path::new(&program))));
            name
        };
        *counts.entry(role.to_owned()).or_default() += 1;
    }
    assert_eq!(counts.get("checker"), Some(&1));
    assert_eq!(counts.get("compiler"), Some(&1));
    assert_eq!(counts.get("reference"), Some(&25));
    assert_eq!(counts.get("candidate"), Some(&25));
    for name in ["cc1plus", "as", "collect2", "ld"] {
        assert_eq!(counts.get(name), Some(&1), "compiler child {name}");
    }
    assert_eq!(counts.values().sum::<usize>(), 55);
    helpers
}

#[test]
fn present_and_absent_lanes_execute_only_bound_checker_compiler_helpers_and_25_pairs() {
    let before = seal_map(&required("CALIBRATION_RETIREMENT_ARTIFACTS_BEFORE"));
    let after = seal_map(&required("CALIBRATION_RETIREMENT_ARTIFACTS_AFTER"));
    assert_eq!(
        before, after,
        "checker, compiler, candidate or source changed"
    );
    let checker = required("CALIBRATION_RETIREMENT_CHECKER");
    let candidate = required("CALIBRATION_RETIREMENT_CANDIDATE");
    let compiler = required("CALIBRATION_RETIREMENT_COMPILER");
    let checkout = required("CALIBRATION_RETIREMENT_CHECKOUT_ROOT");
    let header = checkout.join(check::HEADER);
    let original_checker = checkout.join(check::CHECKER);
    for path in [&checker, &candidate, &compiler, &header, &original_checker] {
        assert_eq!(
            before[path.to_str().unwrap()],
            sha(&bytes(path)),
            "unsealed input {}",
            path.display()
        );
    }
    assert_eq!(before.len(), 5);
    let present_helpers = audit_exec(
        &required("CALIBRATION_RETIREMENT_PRESENT_EXEC"),
        &required("CALIBRATION_RETIREMENT_PRESENT_OUTPUT"),
        &checker,
        &candidate,
        &compiler,
    );
    let absent_helpers = audit_exec(
        &required("CALIBRATION_RETIREMENT_ABSENT_EXEC"),
        &required("CALIBRATION_RETIREMENT_ABSENT_OUTPUT"),
        &checker,
        &candidate,
        &compiler,
    );
    assert_eq!(
        present_helpers, absent_helpers,
        "toolchain child identity changed between lanes"
    );
    assert_eq!(present_helpers.len(), 4);
}

#[test]
fn retained_original_and_native_cli_admission_has_explicit_raw_outcomes() {
    let root = required("CALIBRATION_RETIREMENT_CLI_CAPTURE");
    for (name, expected) in [
        ("help", 0),
        ("hfoo", 0),
        ("hh", 0),
        ("hhh", 0),
        ("h-h", 2),
        ("hh-eq", 2),
        ("h-eq", 2),
        ("long-help-eq", 2),
        ("missing-binary", 2),
        ("unknown", 2),
    ] {
        let case = root.join(name);
        for lane in ["original", "native"] {
            let code: i32 = fs::read_to_string(case.join(format!("{lane}.code")))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(code, expected, "{name}/{lane}");
            let stdout = bytes(&case.join(format!("{lane}.stdout")));
            let stderr = bytes(&case.join(format!("{lane}.stderr")));
            if expected == 0 {
                assert!(!stdout.is_empty(), "{name}/{lane} help empty");
                assert!(stderr.is_empty(), "{name}/{lane} help stderr");
            } else {
                assert!(stdout.is_empty(), "{name}/{lane} invalid stdout");
                assert!(!stderr.is_empty(), "{name}/{lane} invalid stderr");
            }
        }
    }
    for (name, required_code) in [("path-unset", None), ("path-empty", Some(1))] {
        let case = root.join(name);
        let code = |lane: &str| -> i32 {
            fs::read_to_string(case.join(format!("{lane}.code")))
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        let original_code = code("original");
        assert!(matches!(original_code, 0 | 1));
        if let Some(expected) = required_code {
            assert_eq!(original_code, expected);
        }
        assert_eq!(code("native"), original_code, "{name} status parity");
        assert_eq!(
            bytes(&case.join("original.stdout")),
            bytes(&case.join("native.stdout")),
            "{name} stdout parity"
        );
        let original_report = case.join("original-output/report.json");
        let native_report = case.join("native-output/report.json");
        if original_code == 0 {
            assert_eq!(json(&original_report), json(&native_report));
        } else {
            absent(&original_report);
            absent(&native_report);
        }
        assert!(case.join("original.stderr").is_file());
        assert!(case.join("native.stderr").is_file());
    }
}
