//! One-time genuine-checker-absence gate against sealed original-only cloud observations.
#![cfg(all(target_os = "linux", feature = "calibration-float-native"))]
use openpilot_dev_tools::{
    calibration_float_check as check,
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

const REPORT: &[u8] = include_bytes!("fixtures/calibration-float-original-7b5/report.json");
const CPP: &[u8] = include_bytes!("fixtures/calibration-float-original-7b5/source_get_float.cc");
const STDOUT: &[u8] = include_bytes!("fixtures/calibration-float-original-7b5/original.stdout");
const CLI: &[u8] = include_bytes!("fixtures/calibration-float-original-7b5/cli-original.json");
const STREAMS: &[u8] =
    include_bytes!("fixtures/calibration-float-original-7b5/predelete-native-streams.json");
const REPORT_SHA: &str = "c196cadfd49fa7ce417b4987ec2c03ce1d048d3d9c686fca1ecc0aebef24e6e2";
const CPP_SHA: &str = "71e73caa64a4dcc851edc57dc4167b38b01758ba9e16159e5a6a24fe430ce803";
const STDOUT_SHA: &str = "28b4df9d49445c09ba53e17252ffa5ff4c7d300f0bde50e792d228a27d824b4e";
const CLI_SHA: &str = "48504f07bbae0dc0ae768b0d6d132a42bf148808a93d92c901f73b5f7aadb84f";
const STREAMS_SHA: &str = "c906afadb238f6839a0f486c43617b7bbbf79080b5a54d02fe6d013b40990e87";
const REFERENCE_ELF_SHA: &str = "6501105092a79ed7d452e70d5323c98014c9cd3bbce1dc0b92693c0464015fff";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_sha(path: &Path) -> String {
    sha(&fs::read(path).unwrap())
}
fn required(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_else(|| panic!("missing {name}")))
}
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .canonicalize()
        .unwrap()
}
fn absent(path: &Path) {
    assert!(
        matches!(fs::symlink_metadata(path),Err(error) if error.kind()==io::ErrorKind::NotFound),
        "checker must be genuinely absent: {}",
        path.display()
    );
}
fn decode(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn sealed() -> Value {
    for (bytes, expected) in [
        (REPORT, REPORT_SHA),
        (CPP, CPP_SHA),
        (STDOUT, STDOUT_SHA),
        (CLI, CLI_SHA),
        (STREAMS, STREAMS_SHA),
    ] {
        assert_eq!(sha(bytes), expected);
    }
    let receipt: Value = serde_json::from_slice(CLI).unwrap();
    assert_eq!(
        receipt["source_commit"],
        "7b5ebbb7243a0f74cb3b075067a5dd50506b4927"
    );
    assert_eq!(receipt["job_id"], 119003782752_i64);
    assert_eq!(
        receipt["artifact_zip_sha256"],
        "c169d7ef9864c0c3ccc3d1a23e91cf56d02962d3153b5cf185734a79052871d2"
    );
    assert_eq!(receipt["historical_checker_sha256"], check::CHECKER_SHA256);
    assert_eq!(receipt["original_reference_elf_sha256"], REFERENCE_ELF_SHA);
    receipt
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
    let reference = output.join("source-get-float");
    for line in raw.lines().filter(|line| opening.is_match(line)) {
        assert!(!line.contains("execveat("), "opaque execveat: {line}");
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
                    || path.starts_with("/usr/lib/gcc/")
                    || path.starts_with("/usr/libexec/gcc/"),
                "helper lookup outside toolchain: {line}"
            );
            failed_helper_lookups += 1;
            assert!(failed_helper_lookups <= 16, "unbounded helper lookup");
            continue;
        }
        let capture = invocation
            .captures(line)
            .expect("opaque successful executable attempt");
        let program: String = serde_json::from_str(&capture[1]).unwrap();
        let argv: Vec<String> = serde_json::from_str(&capture[2]).unwrap();
        let role = if program == checker.to_str().unwrap() {
            assert_eq!(
                argv,
                vec![
                    program.clone(),
                    "--binary".into(),
                    candidate.to_str().unwrap().into(),
                    "--output".into(),
                    output.to_str().unwrap().into()
                ]
            );
            "checker"
        } else if program == compiler.to_str().unwrap() {
            assert_eq!(
                argv,
                vec![
                    "g++",
                    "-std=c++17",
                    "-O2",
                    output.join("source_get_float.cc").to_str().unwrap(),
                    "-o",
                    reference.to_str().unwrap()
                ]
            );
            "compiler"
        } else if program == reference.to_str().unwrap() || program == candidate.to_str().unwrap() {
            assert_eq!(argv, vec![program.clone()]);
            if program == reference.to_str().unwrap() {
                "reference"
            } else {
                "candidate"
            }
        } else {
            let name = Path::new(&program).file_name().unwrap().to_str().unwrap();
            assert!(
                ["cc1plus", "as", "collect2", "ld"].contains(&name),
                "unrecognized child {program}"
            );
            assert!(
                program.starts_with("/usr/bin/")
                    || program.starts_with("/usr/lib/gcc/")
                    || program.starts_with("/usr/libexec/gcc/"),
                "child outside toolchain {program}"
            );
            helpers.insert(program.clone(), file_sha(Path::new(&program)));
            name
        };
        *counts.entry(role.to_owned()).or_default() += 1;
    }
    for (role, count) in [
        ("checker", 1),
        ("compiler", 1),
        ("reference", 25),
        ("candidate", 25),
        ("cc1plus", 1),
        ("as", 1),
        ("collect2", 1),
        ("ld", 1),
    ] {
        assert_eq!(counts.get(role), Some(&count), "exec count for {role}");
    }
    assert_eq!(counts.values().sum::<usize>(), 56);
    assert_eq!(helpers.len(), 4);
    helpers
}

#[test]
fn absent_python_checker_still_runs_all_25_cold_original_cpp_and_native_cases() {
    let _receipt = sealed();
    let repo = repo();
    absent(&repo.join(check::CHECKER));
    assert_eq!(file_sha(&repo.join(check::HEADER)), check::HEADER_SHA256);
    let proof = check::sources(&repo).unwrap();
    assert!(proof["source_sha256"].get(check::CHECKER).is_none());
    assert_eq!(
        proof["historical_source_sha256"][check::CHECKER],
        check::CHECKER_SHA256
    );
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-calibration-float"));
    let candidate = required("CALIBRATION_FLOAT_BINARY");
    let compiler = PathBuf::from("/usr/bin/g++");
    let strace = PathBuf::from("/usr/bin/strace");
    assert!(strace.is_file(), "cloud must install strace");
    let evidence = required("CALIBRATION_POSTDELETE_EVIDENCE");
    assert!(!evidence.exists(), "fresh evidence directory required");
    fs::create_dir_all(&evidence).unwrap();
    let private = evidence.join("private-root");
    let private_header = private.join(check::HEADER);
    fs::create_dir_all(private_header.parent().unwrap()).unwrap();
    fs::copy(repo.join(check::HEADER), &private_header).unwrap();
    fs::create_dir_all(private.join(check::CHECKER).parent().unwrap()).unwrap();
    absent(&private.join(check::CHECKER));
    assert_eq!(check::sources(&private).unwrap(), proof);
    let bound = [
        &checker,
        &candidate,
        &compiler,
        &strace,
        &repo.join(check::HEADER),
        &private_header,
    ]
    .into_iter()
    .map(|path| (path.display().to_string(), file_sha(path)))
    .collect::<BTreeMap<_, _>>();
    fs::write(
        evidence.join("artifacts-before.json"),
        serde_json::to_vec_pretty(&bound).unwrap(),
    )
    .unwrap();
    let streams: Value = serde_json::from_slice(STREAMS).unwrap();
    let expected_streams = streams["streams"].as_object().unwrap();
    assert_eq!(expected_streams.len(), 100);
    let original: Value = serde_json::from_slice(REPORT).unwrap();
    assert_eq!(original["cases"].as_array().unwrap().len(), 25);
    for (row, input) in original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(check::inputs())
    {
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(
            row["input_hex"],
            input
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    let mut helpers = None;
    let mut reference_sha = None;
    for (name, root) in [("checkout", &repo), ("private", &private)] {
        let output = evidence.join(format!("{name}-output"));
        let trace = evidence.join(format!("{name}.exec"));
        let mut command = CommandSpec::new_os(
            vec![
                strace.as_os_str().to_owned(),
                "-f".into(),
                "-qq".into(),
                "-s65535".into(),
                "-e".into(),
                "trace=execve,execveat".into(),
                "-o".into(),
                trace.as_os_str().to_owned(),
                checker.as_os_str().to_owned(),
                "--binary".into(),
                candidate.as_os_str().to_owned(),
                "--output".into(),
                output.as_os_str().to_owned(),
            ],
            Duration::from_secs(180),
        );
        command
            .env
            .insert("OPENPILOT_RUST_ROOT".into(), root.to_str().unwrap().into());
        // Preserve the admitted pre-deletion compiler/helper search environment.
        // Unset and empty PATH are separately exercised by the CLI regressions.
        command.env.insert("PATH".into(), "/usr/bin:/bin".into());
        let result = NativeRunner.run(&command).unwrap();
        fs::write(
            evidence.join(format!("{name}.code")),
            result.code.to_string(),
        )
        .unwrap();
        fs::write(evidence.join(format!("{name}.stdout")), &result.stdout).unwrap();
        fs::write(evidence.join(format!("{name}.stderr")), &result.stderr).unwrap();
        assert_eq!(
            result.code,
            0,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, STDOUT);
        assert!(result.stderr.is_empty());
        let actual: Value =
            serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap();
        assert_eq!(actual, original, "{name} full 25-row original observation");
        assert_eq!(fs::read(output.join("source_get_float.cc")).unwrap(), CPP);
        let current_reference_sha = file_sha(&output.join("source-get-float"));
        if let Some(previous) = &reference_sha {
            assert_eq!(&current_reference_sha, previous);
        }
        reference_sha = Some(current_reference_sha.clone());
        let provenance: Value =
            serde_json::from_slice(&fs::read(output.join("provenance.json")).unwrap()).unwrap();
        assert_eq!(provenance["sources"], proof);
        assert_eq!(provenance["generated_source_sha256"], CPP_SHA);
        assert_eq!(provenance["reference_sha256"], current_reference_sha);
        assert_eq!(
            provenance["compiler_sha256"],
            bound[&compiler.display().to_string()]
        );
        assert_eq!(
            provenance["executing_checker_sha256"],
            bound[&checker.display().to_string()]
        );
        assert_eq!(
            provenance["binary_sha256"],
            bound[&candidate.display().to_string()]
        );
        assert_eq!(
            provenance["compile_argv_bytes"][0],
            serde_json::json!([103, 43, 43])
        );
        for (stream, expected) in expected_streams {
            assert_eq!(
                fs::read(output.join(stream)).unwrap(),
                decode(expected.as_str().unwrap()),
                "{name}/{stream}"
            );
        }
        let lane_helpers = audit_exec(&trace, &output, &checker, &candidate, &compiler);
        if let Some(previous) = &helpers {
            assert_eq!(&lane_helpers, previous);
        }
        helpers = Some(lane_helpers);
        absent(&root.join(check::CHECKER));
        assert_eq!(check::sources(root).unwrap(), proof);
        for (path, expected) in &bound {
            assert_eq!(
                file_sha(Path::new(path)),
                *expected,
                "{name} changed sealed artifact: {path}"
            );
        }
    }
    let helpers = helpers.unwrap();
    fs::write(
        evidence.join("helpers.json"),
        serde_json::to_vec_pretty(&helpers).unwrap(),
    )
    .unwrap();
    for (path, hash) in &helpers {
        assert_eq!(
            file_sha(Path::new(path)),
            *hash,
            "compiler helper changed after audit: {path}"
        );
    }
    let after = [
        &checker,
        &candidate,
        &compiler,
        &strace,
        &repo.join(check::HEADER),
        &private_header,
    ]
    .into_iter()
    .map(|path| (path.display().to_string(), file_sha(path)))
    .collect::<BTreeMap<_, _>>();
    fs::write(
        evidence.join("artifacts-after.json"),
        serde_json::to_vec_pretty(&after).unwrap(),
    )
    .unwrap();
    assert_eq!(bound, after, "checker/candidate/compiler/headers drifted");
    absent(&repo.join(check::CHECKER));
    absent(&private.join(check::CHECKER));
}

#[test]
fn sealed_twelve_original_cli_phases_remain_admitted_after_checker_deletion() {
    let receipt = sealed();
    let repo = repo();
    absent(&repo.join(check::CHECKER));
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-calibration-float"));
    let candidate = required("CALIBRATION_FLOAT_BINARY");
    let checker_sha = file_sha(&checker);
    let candidate_sha = file_sha(&candidate);
    let header_sha = file_sha(&repo.join(check::HEADER));
    let directory = tempfile::tempdir().unwrap();
    let private = directory.path().join("private");
    fs::create_dir(&private).unwrap();
    let rows = receipt["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let args: Vec<&str> = match name {
            "help" => vec!["--help"],
            "hfoo" => vec!["-hfoo"],
            "hh" => vec!["-hh"],
            "hhh" => vec!["-hhh"],
            "h-h" => vec!["-h-h"],
            "hh-eq" => vec!["-hh=foo"],
            "h-eq" => vec!["-h=foo"],
            "long-help-eq" => vec!["--help=foo"],
            "missing-binary" => vec!["--binary"],
            "unknown" => vec!["--unknown"],
            "path-unset" | "path-empty" => vec![],
            _ => panic!("unknown historical CLI phase {name}"),
        };
        let mut argv = vec![checker.as_os_str().to_owned()];
        argv.extend(args.iter().map(OsString::from));
        let output = directory.path().join(format!("{name}-output"));
        if name.starts_with("path-") {
            argv.extend([
                "--binary".into(),
                candidate.as_os_str().to_owned(),
                "--output".into(),
                output.as_os_str().to_owned(),
            ]);
        }
        let mut command = CommandSpec::new_os(argv, Duration::from_secs(60));
        command
            .env
            .insert("OPENPILOT_RUST_ROOT".into(), repo.to_str().unwrap().into());
        if name.starts_with("path-") {
            command.cwd = Some(private.clone());
        }
        if name == "path-unset" {
            command.env_remove.push("PATH".into());
        }
        if name == "path-empty" {
            command.env.insert("PATH".into(), String::new());
        }
        let result = NativeRunner.run(&command).unwrap();
        assert_eq!(
            result.code,
            row["code"].as_i64().unwrap() as i32,
            "{name} status"
        );
        let historical_stdout = decode(row["stdout_hex"].as_str().unwrap());
        let historical_stderr = decode(row["stderr_hex"].as_str().unwrap());
        if name.starts_with("path-") {
            assert_eq!(result.stdout, historical_stdout, "{name} stdout");
            assert!(
                !historical_stderr.is_empty() && !result.stderr.is_empty(),
                "{name} failure diagnostics"
            );
            assert_eq!(fs::read(output.join("source_get_float.cc")).unwrap(), CPP);
            assert_eq!(row["generated_cpp_sha256"], CPP_SHA);
            assert!(!row["report_present"].as_bool().unwrap());
            absent(&output.join("report.json"));
            absent(&output.join("provenance.json"));
        } else if result.code == 0 {
            assert!(
                !historical_stdout.is_empty() && !result.stdout.is_empty(),
                "{name} help"
            );
            assert!(
                historical_stderr.is_empty() && result.stderr.is_empty(),
                "{name} help stderr"
            );
        } else {
            assert!(
                historical_stdout.is_empty() && result.stdout.is_empty(),
                "{name} invalid stdout"
            );
            assert!(
                !historical_stderr.is_empty() && !result.stderr.is_empty(),
                "{name} invalid stderr"
            );
        }
    }
    absent(&repo.join(check::CHECKER));
    assert_eq!(file_sha(&checker), checker_sha);
    assert_eq!(file_sha(&candidate), candidate_sha);
    assert_eq!(file_sha(&repo.join(check::HEADER)), header_sha);
}
