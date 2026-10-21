//! Mandatory five-case native comparison to sealed original-only observations.
#![cfg(target_os = "linux")]
use openpilot_dev_tools::{
    qa_process::{CommandSpec, NativeRunner, Runner},
    updated_check::{assert_equivalent, cases, parse, run},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    env,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

fn required(name: &str) -> PathBuf {
    PathBuf::from(
        env::var_os(name).unwrap_or_else(|| panic!("{name} is mandatory for updated comparison")),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn exec_attempts(trace: &str) -> Vec<(String, Vec<String>)> {
    let start = regex::Regex::new(r#"^(\d+)\s+execve\("((?:[^"\\]|\\.)*)", (\[.*\]), "#).unwrap();
    let resumed = regex::Regex::new(r"^(\d+)\s+<\.\.\. execve resumed>\)\s+= 0$").unwrap();
    let mut pending = HashMap::<String, (String, Vec<String>)>::new();
    let mut result = Vec::new();
    for line in trace.lines().filter(|line| line.contains("execve")) {
        assert!(!line.contains("execveat("), "opaque execution: {line}");
        if let Some(fields) = start.captures(line) {
            let pid = fields[1].to_owned();
            let program: String = serde_json::from_str(&format!("\"{}\"", &fields[2])).unwrap();
            let argv: Vec<String> = serde_json::from_str(&fields[3]).unwrap();
            assert!(!argv.is_empty(), "empty executable argv: {line}");
            if line.ends_with("<unfinished ...>") {
                assert!(pending.insert(pid, (program, argv)).is_none());
            } else {
                assert!(line.ends_with(") = 0"), "failed executable attempt: {line}");
                result.push((program, argv));
            }
        } else if let Some(fields) = resumed.captures(line) {
            result.push(pending.remove(&fields[1]).expect("orphan resumed execve"));
        } else {
            panic!("unclassified executable attempt: {line}");
        }
    }
    assert!(pending.is_empty(), "unfinished executable attempts");
    result
}

#[test]
fn exec_trace_parser_requires_complete_successful_calls() {
    let direct = "7 execve(\"/usr/bin/git\", [\"/usr/bin/git\", \"status\"], 0x1 /* 1 vars */) = 0";
    let split = "8  execve(\"/usr/bin/basename\", [\"basename\", \"--\", \"/usr/lib/git-core/git-submodule\"], 0x2 /* 1 vars */ <unfinished ...>\n8 <... execve resumed>) = 0";
    assert_eq!(
        exec_attempts(&format!("{direct}\n{split}")),
        vec![
            (
                "/usr/bin/git".into(),
                vec!["/usr/bin/git".into(), "status".into()]
            ),
            (
                "/usr/bin/basename".into(),
                vec![
                    "basename".into(),
                    "--".into(),
                    "/usr/lib/git-core/git-submodule".into()
                ]
            ),
        ]
    );
    for malformed in [
        "8 <... execve resumed>) = 0",
        "8 execve(\"/usr/bin/git\", [\"git\"], 0x1 /* 1 vars */ <unfinished ...>\n8 <... execve resumed>) = -1 ENOENT (No such file or directory)",
        "8 execve(\"/usr/bin/git\", [\"git\"], 0x1 /* 1 vars */ <unfinished ...>",
        "8 execve(\"/usr/bin/git\", [\"git\"], 0x1 /* 1 vars */ <unfinished ...>\n9 <... execve resumed>) = 0",
        "8 execveat(AT_FDCWD, \"/usr/bin/git\", [\"git\"], 0x1, 0) = 0",
        "8 execve(\"/usr/bin/git\", [\"git\"], 0x1 /* 1 vars */) = -1 ENOENT (No such file or directory)",
    ] {
        assert!(
            std::panic::catch_unwind(|| exec_attempts(malformed)).is_err(),
            "malformed trace accepted: {malformed}"
        );
    }
}

fn exec_kind(
    program: &str,
    argv: &[String],
    checker: &Path,
    binary: &Path,
    launcher: &Path,
) -> &'static str {
    let first = argv.first().map(String::as_str);
    let args = argv.iter().map(String::as_str).collect::<Vec<_>>();
    if program == checker.to_str().unwrap() {
        assert_eq!(first, Some(program));
        return "checker";
    }
    if program == binary.to_str().unwrap() {
        assert_eq!(argv.len(), 1);
        assert_eq!(first, Some(program));
        return "candidate";
    }
    if program == launcher.to_str().unwrap() {
        assert_eq!(first, Some(program));
        assert_eq!(argv.len(), 2);
        assert!(argv[1].starts_with("/tmp/op-exec-"));
        return "launcher";
    }
    if program.starts_with("/tmp/updated-source-") {
        let (case, tool) = [
            "check-fetch-metered",
            "missing-branch-failure",
            "tizi-branch-map-agnos-same-version",
            "agnos-model-manifest-boundary",
            "connectivity-thresholds",
        ]
        .into_iter()
        .find_map(|name| {
            ["git", "sudo", "bash"].into_iter().find_map(|tool| {
                program
                    .ends_with(&format!("/{name}-native/bin/{tool}"))
                    .then_some((name, tool))
            })
        })
        .unwrap_or_else(|| panic!("unrecognized private fixture executable: {program}"));
        assert!(!case.is_empty());
        assert_eq!(first, Some(tool));
        match tool {
            "git" => assert!(matches!(
                argv.get(1).map(String::as_str),
                Some(
                    "rev-parse"
                        | "show"
                        | "diff"
                        | "config"
                        | "ls-remote"
                        | "reset"
                        | "submodule"
                        | "checkout"
                        | "clean"
                        | "lfs"
                        | "fetch"
                        | "branch"
                )
            )),
            "sudo" => assert!(matches!(
                argv.get(1).map(String::as_str),
                Some("mount" | "chmod")
            )),
            "bash" => {
                assert_eq!(argv.len(), 3);
                assert_eq!(argv.get(1).map(String::as_str), Some("-c"));
                assert!(argv[2].starts_with("unset AGNOS_VERSION && source launch_env.sh"));
            }
            _ => unreachable!(),
        }
        return match tool {
            "git" => "fixture-git",
            "sudo" => "fixture-sudo",
            _ => "fixture-bash",
        };
    }
    match program {
        "/usr/bin/git" => {
            assert_eq!(first, Some(program));
            assert!(matches!(
                argv.get(1).map(String::as_str),
                Some(
                    "init"
                        | "config"
                        | "add"
                        | "commit"
                        | "rev-parse"
                        | "branch"
                        | "clone"
                        | "reset"
                )
            ));
            "system-git"
        }
        "/usr/lib/git-core/git" => {
            assert!(matches!(first, Some("git" | "/usr/lib/git-core/git")));
            assert!(matches!(
                argv.get(1).map(String::as_str),
                Some(
                    "-c" | "--exec-path"
                        | "rev-parse"
                        | "submodule--helper"
                        | "config"
                        | "version"
                        | "-C"
                        | "maintenance"
                        | "rev-list"
                )
            ));
            "git-core"
        }
        "/usr/lib/git-core/git-upload-pack" => {
            assert_eq!(first, Some("git-upload-pack"));
            assert_eq!(argv.len(), 2);
            assert!(argv[1].starts_with("/tmp/updated-source-"));
            assert!(argv[1].ends_with("/remote") || argv[1].ends_with("/remote/.git"));
            "git-upload-pack"
        }
        "/usr/lib/git-core/git-submodule" => {
            assert_eq!(first, Some(program));
            assert!(matches!(
                argv.get(1).map(String::as_str),
                Some("foreach" | "sync" | "update")
            ));
            "git-submodule"
        }
        "/usr/bin/git-lfs" => {
            assert_eq!(argv.len(), 2);
            assert_eq!(first, Some(program));
            assert_eq!(argv[1], "prune");
            "git-lfs"
        }
        "/bin/sh" => {
            assert_eq!(first, Some(program));
            assert_eq!(argv.get(1).map(String::as_str), Some("-c"));
            assert!(argv
                .get(2)
                .is_some_and(|arg| arg.starts_with("git-upload-pack '/tmp/updated-source-")));
            assert_eq!(argv.len(), 4);
            assert_eq!(argv[2], argv[3]);
            "shell-upload-pack"
        }
        "/usr/bin/basename" => {
            assert!(matches!(
                args.as_slice(),
                ["basename", "/usr/lib/git-core/git-submodule"]
                    | ["basename", "--", "/usr/lib/git-core/git-submodule"]
            ));
            "basename"
        }
        "/usr/bin/envsubst" => {
            assert!(matches!(
                args.as_slice(),
                ["envsubst", "usage: $dashless $USAGE"]
                    | ["envsubst", "--variables", "usage: $dashless $USAGE"]
            ));
            "envsubst"
        }
        "/usr/bin/gettext" => {
            assert_eq!(args, ["gettext", "usage: $dashless $USAGE"]);
            "gettext"
        }
        "/usr/bin/sed" => {
            assert!(matches!(
                args.as_slice(),
                ["sed", "-e", "s/-/ /"] | ["sed", "-e", "s/-/_/g"]
            ));
            "sed"
        }
        "/usr/bin/uname" => {
            assert_eq!(args, ["uname", "-s"]);
            "uname"
        }
        _ => panic!("unrecognized executable: {program} {argv:?}"),
    }
}

#[test]
fn native_all_five_cases_match_sealed_original() {
    let output = required("UPDATED_CHECK_OUTPUT");
    let report = run(
        &required("UPDATED_CHECK_BINARY"),
        &required("UPDATED_CHECK_LAUNCHER"),
        &output,
        &required("UPDATED_CHECK_SHIM"),
        &repo(),
    )
    .unwrap();
    assert_eq!(report["passed"], true);
    assert_eq!(report["original_observation"], "sealed_df0128aa_capture");
    let names = cases().into_iter().map(|case| case.0).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "check-fetch-metered",
            "missing-branch-failure",
            "tizi-branch-map-agnos-same-version",
            "agnos-model-manifest-boundary",
            "connectivity-thresholds"
        ]
    );
    assert_eq!(report["scenarios"].as_array().unwrap().len(), names.len());
    for name in names {
        let source = sealed_case(name);
        let native: Value =
            serde_json::from_slice(&fs::read(output.join(format!("{name}-native.json"))).unwrap())
                .unwrap();
        assert!(assert_equivalent(&source, &native).unwrap() > 0);
        assert!(report["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["scenario"] == name && row["passed"] == true));
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}
fn sealed_case(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/updated-original-df0128aa")
        .join(format!("{name}.json"));
    let expected = match name {
        "check-fetch-metered" => "051f172d2934a3f245736f48df1139f8cb108197e4b430cdf8cbc13ce251437d",
        "missing-branch-failure" => {
            "2ac87b6b182010fc835b22f1a00a4114fd512ee9c05ca3cd2aa147d350b2ee42"
        }
        "tizi-branch-map-agnos-same-version" => {
            "59532593affbe4e2ffae3449164c78b36a51e4ab2273bead306f7262d0a1e85f"
        }
        "agnos-model-manifest-boundary" => {
            "216cbde9a519bafab097714e7a4b8efd63f0a73a4a28ef3525508e832bfb71ce"
        }
        "connectivity-thresholds" => {
            "ced55dbb2f37b9f381925466fcd0b1b169806002e81c7d43bca30a3ed5a82c40"
        }
        _ => panic!("unknown scenario"),
    };
    let bytes = fs::read(path).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn raw_original_scenario_streams_keep_their_independent_capture_hashes() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/updated-original-df0128aa/raw-original");
    let manifest = fs::read(folder.join("SHA256SUMS")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&manifest)),
        "c7100b2d3b706cc76e7c115835707ecbde46ab3c2a6051e17ba4b731fd44e29b"
    );
    let lines = std::str::from_utf8(&manifest)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 10);
    for line in lines {
        let (digest, file) = line.split_once("  ").unwrap();
        assert!(!file.contains('/') && !file.contains(".."));
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(folder.join(file)).unwrap())),
            digest
        );
    }
}

#[test]
fn comparison_accepts_only_exact_obsolete_staging_removal() {
    let obsolete = json!({"argv":["sudo","rm","-rf","<ROOT>/data/stage"],"cwd":null});
    let kept = json!({"argv":["git","fetch"],"cwd":"<ROOT>/base"});
    let source = json!({"commands":[obsolete.clone(),kept.clone()],"snapshots":[{"consistent":true}],"agnos":[]});
    let native = json!({"commands":[kept],"snapshots":[{"consistent":true}],"agnos":[]});
    assert_eq!(assert_equivalent(&source, &native).unwrap(), 1);
    let mut altered = native.clone();
    altered["snapshots"][0]["consistent"] = json!(false);
    assert!(assert_equivalent(&source, &altered).is_err());
    let mut altered = native.clone();
    altered["commands"] = json!([]);
    assert!(assert_equivalent(&source, &altered).is_err());
    let mut altered = source.clone();
    altered["commands"] =
        json!([{"argv":["sudo","rm","-rf","<ROOT>/data/stage/child"],"cwd":null}]);
    assert!(assert_equivalent(&altered, &native).is_err());
    assert!(assert_equivalent(&native, &native).is_err());
}

#[test]
fn python_integral_wait_equality_does_not_change_other_snapshot_fields() {
    let obsolete = json!({"argv":["sudo","rm","-rf","<ROOT>/data/stage"],"cwd":null});
    let source =
        json!({"commands":[obsolete],"snapshots":[{"wait":5400.0,"consistent":true}],"agnos":[]});
    let native = json!({"commands":[],"snapshots":[{"wait":5400,"consistent":true}],"agnos":[]});
    assert_eq!(assert_equivalent(&source, &native).unwrap(), 1);
    for wrong_wait in [json!(5399), json!(5401), json!("5400"), json!(null)] {
        let mut changed = native.clone();
        changed["snapshots"][0]["wait"] = wrong_wait;
        assert!(assert_equivalent(&source, &changed).is_err());
    }
    let mut changed = native;
    changed["snapshots"][0]["consistent"] = json!(false);
    assert!(assert_equivalent(&source, &changed).is_err());
}

#[test]
fn native_cli_retains_eleven_sealed_original_admissions() {
    let receipts = required("UPDATED_CHECK_CLI_OUTPUT");
    fs::create_dir_all(&receipts).unwrap();
    let sealed = include_bytes!("fixtures/updated-original-df0128aa/cli-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(sealed)),
        "a9806418ef44248e24a494f37ec387195f6d87996e5de7278c0fc8205cf8372f"
    );
    let sealed: Value = serde_json::from_slice(sealed).unwrap();
    let originals = sealed.as_array().unwrap();
    assert_eq!(originals.len(), 11);
    let native = PathBuf::from(env!("CARGO_BIN_EXE_check-updated"));
    let native_sha = format!("{:x}", Sha256::digest(fs::read(&native).unwrap()));
    for (index, (name, args)) in [
        ("help", vec!["--help"]),
        ("short-help-attached", vec!["-hfoo"]),
        ("short-help-repeated", vec!["-hh"]),
        (
            "repeated-binary-before-help",
            vec!["--binary", "first", "--binary", "second", "--help"],
        ),
        (
            "abbreviated-options-before-help",
            vec![
                "--bina", "b", "--lau", "l", "--bind", "p", "--out", "o", "--help",
            ],
        ),
        ("missing-binary-value", vec!["--binary"]),
        ("unknown-option", vec!["--unknown"]),
        ("ambiguous-binary-prefix", vec!["--bin"]),
        ("help-equals", vec!["--help=bad"]),
        ("short-help-equals", vec!["-h=bad"]),
        (
            "incomplete-required-set",
            vec!["--binary", "b", "--launcher", "l"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut native_args = vec![native.as_os_str().to_owned()];
        native_args.extend(args.iter().map(OsString::from));
        let candidate = NativeRunner
            .run(&CommandSpec::new_os(
                native_args.clone(),
                Duration::from_secs(5),
            ))
            .unwrap();
        let original = &originals[index];
        assert_eq!(original["case"], name);
        assert_eq!(
            original["source_checker_sha256"],
            "bfa1686e464e5457b9a9bdc66b954e81f5994db24860d7f7b2869999f4e29b7d"
        );
        assert_eq!(
            original["source_python_sha256"],
            "53dab24bf9e0e9c490088a53d71770b6f027b057678b313648e75f7fc87a675e"
        );
        let historical = &original["original"];
        let old_args = historical["argv"].as_array().unwrap();
        assert_eq!(old_args.len(), args.len() + 2);
        for (actual, expected) in old_args.iter().skip(2).zip(&args) {
            assert_eq!(actual.as_str(), Some(*expected));
        }
        let receipt = json!({
            "case":name,
            "sealed_original":original,
            "native_checker_sha256":native_sha,
            "native":{
                "argv":native_args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                "argv_hex":native_args.iter().map(|arg| hex(arg.as_bytes())).collect::<Vec<_>>(),
                "code":candidate.code,
                "stdout":String::from_utf8_lossy(&candidate.stdout),
                "stderr":String::from_utf8_lossy(&candidate.stderr),
                "stdout_hex":hex(&candidate.stdout),
                "stderr_hex":hex(&candidate.stderr),
            }
        });
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(receipts.join(format!("{index:02}-{name}.json")))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&receipt).unwrap())
            .unwrap();
        file.write_all(b"\n").unwrap();
        file.sync_all().unwrap();
        assert_eq!(
            candidate.code,
            historical["code"].as_i64().unwrap() as i32,
            "CLI status for {args:?}: native stderr={}",
            String::from_utf8_lossy(&candidate.stderr)
        );
        assert!(matches!(candidate.code, 0 | 2));
        if candidate.code == 0 {
            assert!(String::from_utf8_lossy(&candidate.stdout).contains("usage:"));
            assert_eq!(historical["stderr_hex"], "");
            assert!(candidate.stderr.is_empty());
        } else {
            assert_eq!(historical["stdout_hex"], "");
            assert!(candidate.stdout.is_empty());
            assert!(!historical["stderr_hex"].as_str().unwrap().is_empty());
            assert!(!candidate.stderr.is_empty());
        }
    }
    assert_eq!(fs::read_dir(&receipts).unwrap().count(), 11);
    let parsed = parse(&[
        "--bina".into(),
        "first".into(),
        "--binary".into(),
        "last".into(),
        "--lau".into(),
        "launcher".into(),
        "--bind".into(),
        "binding".into(),
        "--out".into(),
        "output".into(),
    ])
    .unwrap()
    .unwrap();
    assert_eq!(parsed.binary, PathBuf::from("last"));
    assert_eq!(parsed.launcher, PathBuf::from("launcher"));
    assert_eq!(parsed.output, PathBuf::from("output"));
}

#[test]
fn preexisting_result_is_not_reused_as_pass() {
    let directory = tempfile::tempdir().unwrap();
    let receipt = directory.path().join("result.json");
    fs::write(&receipt, b"{\"passed\":true}\n").unwrap();
    let missing = directory.path().join("missing");
    assert!(run(&missing, &missing, directory.path(), &missing, &repo()).is_err());
    assert_eq!(fs::read(receipt).unwrap(), b"{\"passed\":true}\n");
}

#[test]
fn missing_candidate_cannot_leave_a_success_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("proof");
    let missing = directory.path().join("missing");
    assert!(run(&missing, &missing, &output, &missing, &repo()).is_err());
    assert!(output.is_dir());
    assert!(!output.join("result.json").exists());
}

#[test]
fn missing_native_shim_cannot_finalize_a_pass() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("failed-shim");
    assert!(run(
        &required("UPDATED_CHECK_BINARY"),
        &required("UPDATED_CHECK_LAUNCHER"),
        &output,
        &directory.path().join("missing-shim"),
        &repo(),
    )
    .is_err());
    assert!(!output.join("result.json").exists());
}

#[test]
fn historical_source_reappearance_mutation_and_symlink_fail_before_receipt() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let source_root = directory.path().join("root");
    let tools = source_root.join("rust/tools");
    fs::create_dir_all(&tools).unwrap();
    fs::copy(
        repo().join("rust/tools/updated_fixtures.py"),
        tools.join("updated_fixtures.py"),
    )
    .unwrap();
    let missing = directory.path().join("missing-binary");
    let attempt = |label: &str| {
        run(
            &missing,
            &missing,
            &directory.path().join(label),
            &missing,
            &source_root,
        )
        .unwrap_err()
        .to_string()
    };
    assert!(attempt("valid-absence").contains("No such file"));
    let reappeared = tools.join("updated_source.py");
    fs::write(&reappeared, b"unexpected source reappearance").unwrap();
    assert!(attempt("reappeared").contains("historical source changed"));
    fs::remove_file(&reappeared).unwrap();
    symlink("updated_fixtures.py", &reappeared).unwrap();
    assert!(attempt("symlink").contains("not regular"));
    fs::remove_file(&reappeared).unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &reappeared,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(attempt("fifo").contains("not regular"));
    fs::remove_file(&reappeared).unwrap();
    fs::write(tools.join("updated_fixtures.py"), b"mutated fixture").unwrap();
    assert!(attempt("fixture-drift").contains("historical source changed"));
    fs::write(
        tools.join("updated_fixtures.py"),
        vec![b'x'; 2 * 1024 * 1024 + 1],
    )
    .unwrap();
    assert!(attempt("oversized-fixture").contains("bounded admission"));
    fs::remove_dir_all(&tools).unwrap();
    assert!(attempt("missing-parent").contains("No such file"));
}

#[test]
fn same_elves_run_checkout_and_genuinely_absent_historical_roots() {
    let evidence = required("UPDATED_CHECK_ABSENCE_EVIDENCE");
    assert!(!evidence.exists(), "fresh absence evidence required");
    fs::create_dir_all(&evidence).unwrap();
    let absent = evidence.join("absent-root");
    let tools = absent.join("rust/tools");
    fs::create_dir_all(&tools).unwrap();
    fs::copy(
        repo().join("rust/tools/updated_fixtures.py"),
        tools.join("updated_fixtures.py"),
    )
    .unwrap();
    for name in ["check_updated.py", "updated_source.py"] {
        assert!(
            matches!(fs::symlink_metadata(tools.join(name)), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
        );
    }
    let checker_present = repo().join("rust/tools/check_updated.py").exists();
    let source_present = repo().join("rust/tools/updated_source.py").exists();
    assert_eq!(
        checker_present, source_present,
        "historical pair partially retired"
    );
    fs::write(
        evidence.join("checkout-historical-presence.json"),
        serde_json::to_vec_pretty(&json!({
            "checker":checker_present,
            "source":source_present,
            "admission_root_scope":["rust/tools/check_updated.py","rust/tools/updated_source.py","rust/tools/updated_fixtures.py"],
            "other_original_dependency_pins":"current checkout root, not private admission root"
        }))
            .unwrap(),
    )
    .unwrap();
    let checker = PathBuf::from(env!("CARGO_BIN_EXE_check-updated"));
    let binary = required("UPDATED_CHECK_BINARY");
    let launcher = required("UPDATED_CHECK_LAUNCHER");
    let shim = required("UPDATED_CHECK_SHIM");
    let strace = PathBuf::from("/usr/bin/strace");
    assert!(strace.is_file(), "cloud must install strace");
    let helper_source = tools.join("updated_fixtures.py");
    let system_execs = [
        "/usr/bin/git",
        "/usr/bin/git-lfs",
        "/usr/lib/git-core/git",
        "/usr/lib/git-core/git-submodule",
        "/usr/lib/git-core/git-upload-pack",
        "/bin/sh",
        "/usr/bin/basename",
        "/usr/bin/envsubst",
        "/usr/bin/gettext",
        "/usr/bin/sed",
        "/usr/bin/uname",
    ]
    .map(PathBuf::from);
    let mut seals = Vec::new();
    for path in [&checker, &binary, &launcher, &shim, &strace, &helper_source]
        .into_iter()
        .chain(system_execs.iter())
    {
        let bytes = fs::read(path).unwrap();
        seals.push((
            path.display().to_string(),
            format!("{:x}", Sha256::digest(bytes)),
        ));
    }
    fs::write(
        evidence.join("inputs-before.json"),
        serde_json::to_vec_pretty(&seals).unwrap(),
    )
    .unwrap();
    let mut reports = Vec::new();
    for (name, root) in [("checkout", repo()), ("absent", absent.clone())] {
        let output = evidence.join(format!("{name}-output"));
        let trace = evidence.join(format!("{name}.exec"));
        let command = CommandSpec::new_os(
            vec![
                strace.as_os_str().to_owned(),
                "-f".into(),
                "-qq".into(),
                "-s65535".into(),
                "-e".into(),
                "trace=execve,execveat".into(),
                "-o".into(),
                trace.clone().into(),
                checker.clone().into(),
                "--binary".into(),
                binary.clone().into(),
                "--launcher".into(),
                launcher.clone().into(),
                "--output".into(),
                output.clone().into(),
                "--root".into(),
                root.clone().into(),
            ],
            Duration::from_secs(180),
        );
        let result = NativeRunner.run(&command).unwrap();
        fs::write(evidence.join(format!("{name}.stdout")), &result.stdout).unwrap();
        fs::write(evidence.join(format!("{name}.stderr")), &result.stderr).unwrap();
        assert_eq!(
            result.code,
            0,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual: Value =
            serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
        assert_eq!(actual["original_observation"], "sealed_df0128aa_capture");
        let trace_bytes = fs::read_to_string(&trace).unwrap();
        let attempts = exec_attempts(&trace_bytes);
        assert_eq!(attempts.first().map(|row| row.0.as_str()), checker.to_str());
        let mut kinds = BTreeMap::<&str, usize>::new();
        for (program, argv) in &attempts {
            assert!(!program.contains("python"), "Python attempted: {program}");
            let kind = exec_kind(program, argv, &checker, &binary, &launcher);
            if kind == "checker" {
                assert_eq!(
                    argv,
                    &vec![
                        checker.display().to_string(),
                        "--binary".into(),
                        binary.display().to_string(),
                        "--launcher".into(),
                        launcher.display().to_string(),
                        "--output".into(),
                        output.display().to_string(),
                        "--root".into(),
                        root.display().to_string(),
                    ]
                );
            }
            *kinds.entry(kind).or_default() += 1;
        }
        // First native-only checkout trace: run 39540243688, job 119026080148,
        // artifact 12319827082. The 71 unfinished calls all resumed with = 0.
        assert_eq!(
            kinds,
            BTreeMap::from([
                ("checker", 1),
                ("candidate", 5),
                ("launcher", 322),
                ("fixture-git", 294),
                ("fixture-sudo", 26),
                ("fixture-bash", 2),
                ("system-git", 25),
                ("git-core", 198),
                ("git-upload-pack", 26),
                ("git-submodule", 16),
                ("git-lfs", 4),
                ("shell-upload-pack", 26),
                ("basename", 32),
                ("envsubst", 32),
                ("gettext", 16),
                ("sed", 48),
                ("uname", 16),
            ]),
            "actual native executable roster changed"
        );
        for path in [&checker, &binary, &launcher, &shim, &strace, &helper_source]
            .into_iter()
            .chain(system_execs.iter())
        {
            let digest = format!("{:x}", Sha256::digest(fs::read(path).unwrap()));
            assert_eq!(
                seals
                    .iter()
                    .find(|(p, _)| p == &path.display().to_string())
                    .unwrap()
                    .1,
                digest
            );
        }
        reports.push(actual["scenarios"].clone());
        for case in cases() {
            let file = format!("{}-native.json", case.0);
            let value: Value =
                serde_json::from_slice(&fs::read(output.join(file)).unwrap()).unwrap();
            assert!(assert_equivalent(&sealed_case(case.0), &value).is_ok());
        }
    }
    assert_eq!(reports[0], reports[1]);
    fs::write(
        evidence.join("inputs-after.json"),
        serde_json::to_vec_pretty(&seals).unwrap(),
    )
    .unwrap();
}
