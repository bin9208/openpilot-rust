//! Fast controls; hosted source capture supplies independent full-stream parity.
use openpilot_dev_tools::teleop_version::{bump, lookup};

#[test]
fn increment_is_final_component_python_integer_conversion_not_semver() {
    for (input, expected) in [
        ("1.0.1\n", "1.0.2\n"),
        ("release..009\nignored", "release..10\n"),
        ("1.-2\n", "1.-1\n"),
        ("1.+000\n", "1.1\n"),
        ("1. ١_٢ \n", "1.13\n"),
        (
            "1.999999999999999999999999999999\n",
            "1.1000000000000000000000000000000\n",
        ),
        ("1.2\rignored\n", "1.3\n"),
    ] {
        assert_eq!(bump(input.as_bytes()).unwrap(), expected.as_bytes());
    }
    for input in ["1.", "1.1__2", "1.0x10", "1.²", "1.\u{1c}2"] {
        assert_eq!(bump(input.as_bytes()).unwrap_err().class, "ValueError");
    }
    assert_eq!(bump(b"").unwrap_err().class, "EOFError");
    assert_eq!(bump(b"\xff").unwrap_err().class, "ValueError");
    assert!(bump(format!("1.{}", "9".repeat(4300)).as_bytes()).is_err());
    assert!(bump(format!("1.{}", "1".repeat(4300)).as_bytes()).is_ok());
    assert!(bump(format!("1.{}", "1".repeat(4301)).as_bytes()).is_err());
}

#[test]
fn lookup_uses_toml_keys_and_prints_the_value_without_trimming() {
    assert_eq!(
        lookup(b"[project]\nversion = '1.0.1'\n").unwrap(),
        b"1.0.1\n"
    );
    assert_eq!(lookup(b"project.version = ' 1.2 '\n").unwrap(), b" 1.2 \n");
    assert_eq!(lookup(b"").unwrap_err().class, "KeyError");
    assert_eq!(
        lookup(b"[project]\nname='x'\n").unwrap_err().class,
        "KeyError"
    );
    assert_eq!(lookup(b"project = 2\n").unwrap_err().class, "TypeError");
    assert_eq!(
        lookup(b"[project]\nversion=\n").unwrap_err().class,
        "TOMLDecodeError"
    );
    assert_eq!(lookup(b"\xff").unwrap_err().class, "UnicodeDecodeError");
}

#[test]
fn unsupported_incidental_version_types_are_explicit_and_raw_prefixes_survive() {
    assert_eq!(
        lookup(b"[project]\nversion=true\n").unwrap_err().class,
        "UnsupportedVersionType"
    );
    assert_eq!(bump(b"\xff.1\n").unwrap(), b"\xff.2\n");
}

use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn required(name: &str) -> PathBuf {
    std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} required"))
        .into()
}
fn decode(value: &Value) -> Vec<u8> {
    STANDARD.decode(value.as_str().unwrap()).unwrap()
}
fn command(
    argv: Vec<OsString>,
    cwd: &Path,
    input: Vec<u8>,
    env: &BTreeMap<OsString, OsString>,
) -> openpilot_dev_tools::qa_process::Captured {
    let mut spec = CommandSpec::new_os(argv, Duration::from_secs(15));
    spec.cwd = Some(cwd.into());
    spec.input = Some(input);
    spec.os_env = Some(env.clone());
    spec.stdout_limit = 1024 * 1024;
    spec.stderr_limit = 1024 * 1024;
    NativeRunner.run(&spec).unwrap()
}
/// Source-only original capture is produced once in Actions, then reused for
/// native computations and all disposable script branches/effects.
#[test]
#[ignore = "hosted original-only receipts and parent-integrated native script required"]
fn independent_original_computations_and_disposable_script_effects() {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "Git effects only in hosted disposable fixtures"
    );
    let original = required("TELEOP_VERSION_ORIGINAL");
    let cli = required("TELEOP_VERSION_BIN");
    let script = required("TELEOP_VERSION_SCRIPT");
    let evidence = required("TELEOP_VERSION_EVIDENCE");
    fs::create_dir(&evidence).unwrap();
    let manifest_bytes = fs::read(original.join("manifest.json")).unwrap();
    assert_eq!(
        sha(&manifest_bytes),
        std::env::var("TELEOP_VERSION_ORIGINAL_MANIFEST_SHA256").unwrap()
    );
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["native_expected_used"], false);
    assert_eq!(manifest["computations"], 34);
    assert_eq!(manifest["effects"], 10);
    assert_eq!(manifest["int_max_str_digits"], 4300);
    assert_eq!(
        manifest["source_commit"],
        "c810f7159ccecde0826e51c1ccc6364d0081cd8e"
    );
    let files = manifest["files"].as_object().unwrap();
    assert_eq!(
        files.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "computations.json",
            "script-effects.json",
            "snippets.json",
            "original-bump_tag.sh",
            "original-pyproject.toml"
        ])
    );
    let verify = || {
        for (name, pin) in files {
            assert!(!fs::symlink_metadata(original.join(name))
                .unwrap()
                .file_type()
                .is_symlink());
            let data = fs::read(original.join(name)).unwrap();
            assert_eq!(pin["bytes"], data.len());
            assert_eq!(pin["sha256"], sha(&data));
        }
    };
    verify();
    assert_eq!(
        manifest["source_pins_before"],
        manifest["source_pins_after"]
    );
    assert_eq!(
        sha(&fs::read(original.join("original-bump_tag.sh")).unwrap()),
        "34e3f49bcc8905086065d31b085d0c544ffe89092e4f303cdcaa5459c52424e0"
    );
    assert_eq!(
        sha(&fs::read(original.join("original-pyproject.toml")).unwrap()),
        "1a35c809ec724bc7de6536c42da261a69dc4662455c609f2d969e7389980daf9"
    );
    let cli_before = sha(&fs::read(&cli).unwrap());
    let script_before = sha(&fs::read(&script).unwrap());
    assert!(
        !fs::read_to_string(&script).unwrap().contains("python3"),
        "native script must not execute the original Python computations"
    );
    let rows: Vec<Value> =
        serde_json::from_slice(&fs::read(original.join("computations.json")).unwrap()).unwrap();
    let expected_names = BTreeSet::from([
        "lookup/actual",
        "lookup/quoted-key",
        "lookup/spaces",
        "lookup/multiline",
        "lookup/unicode",
        "lookup/missing-project",
        "lookup/missing-version",
        "lookup/project-scalar",
        "lookup/invalid-toml",
        "lookup/duplicate",
        "lookup/invalid-utf8",
        "lookup/missing-file",
        "lookup/directory",
        "bump/ordinary",
        "bump/single",
        "bump/leading",
        "bump/negative",
        "bump/plus",
        "bump/unicode",
        "bump/large",
        "bump/empty",
        "bump/blank",
        "bump/trailing-dot",
        "bump/underscore",
        "bump/base",
        "bump/not-decimal",
        "bump/control",
        "bump/newline",
        "bump/cr",
        "bump/raw-prefix",
        "bump/raw-tail",
        "bump/digit-limit",
        "bump/result-limit",
        "bump/at-limit",
    ]);
    assert_eq!(rows.len(), expected_names.len());
    assert_eq!(
        rows.iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected_names
    );
    let env = BTreeMap::from([(
        OsString::from("PYTHONINTMAXSTRDIGITS"),
        OsString::from("4300"),
    )]);
    let mut actual = Vec::new();
    for row in rows {
        let work = tempfile::tempdir().unwrap();
        let input = decode(&row["input_b64"]);
        let mode = row["mode"].as_str().unwrap();
        let mut args = vec![cli.clone().into_os_string()];
        if mode == "bump" {
            args.push("bump".into())
        } else {
            args.extend(["lookup".into(), "pyproject.toml".into()]);
            if mode == "directory" {
                fs::create_dir(work.path().join("pyproject.toml")).unwrap()
            } else if mode != "missing-file" {
                fs::write(work.path().join("pyproject.toml"), &input).unwrap()
            }
        }
        let result = command(
            args.clone(),
            work.path(),
            if mode == "bump" { input } else { vec![] },
            &env,
        );
        fs::write(evidence.join(format!("computation-{}.json",row["name"].as_str().unwrap().replace('/',"-"))),serde_json::to_vec_pretty(&json!({"argv_bytes":args.iter().map(|s|{use std::os::unix::ffi::OsStrExt;s.as_os_str().as_bytes().to_vec()}).collect::<Vec<_>>(),"status":result.code,"stdout_b64":STANDARD.encode(&result.stdout),"stderr_b64":STANDARD.encode(&result.stderr)})).unwrap()).unwrap();
        assert_eq!(
            result.code,
            row["status"].as_i64().unwrap() as i32,
            "{}",
            row["name"]
        );
        assert_eq!(result.stdout, decode(&row["stdout_b64"]), "{}", row["name"]);
        if row["status"] == 0 {
            assert!(result.stderr.is_empty())
        } else {
            assert!(!result.stderr.is_empty());
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains(row["error_class"].as_str().unwrap()),
                "{}: {}",
                row["name"],
                String::from_utf8_lossy(&result.stderr)
            )
        }
        actual.push(json!({"name":row["name"],"status":result.code,"stdout_b64":STANDARD.encode(&result.stdout),"stderr_b64":STANDARD.encode(&result.stderr)}));
    }
    let effects: Vec<Value> =
        serde_json::from_slice(&fs::read(original.join("script-effects.json")).unwrap()).unwrap();
    assert_eq!(effects.len(), 10);
    assert_eq!(
        effects
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "first-tag",
            "equal-increment",
            "ahead",
            "lower",
            "wrong-branch",
            "version-sort",
            "big-increment",
            "malformed-toml",
            "missing-version",
            "invalid-increment"
        ])
    );
    let mut env = BTreeMap::from([
        (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
        ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ("LC_ALL".into(), "C.UTF-8".into()),
        ("TELEOP_VERSION_BIN".into(), cli.clone().into_os_string()),
    ]);
    for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
        env.insert(key.into(), "Fixture".into());
    }
    for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
        env.insert(key.into(), "fixture@example.invalid".into());
    }
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
        env.insert(key.into(), "2026-10-22T00:00:00Z".into());
    }
    let mut observed_effects = Vec::new();
    for case in effects {
        let work = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let mut argv = vec![OsString::from("/usr/bin/git")];
            argv.extend(args.iter().map(|arg| OsString::from(*arg)));
            let r = command(argv, work.path(), vec![], &env);
            assert_eq!(r.code, 0, "{}", String::from_utf8_lossy(&r.stderr));
            r.stdout
        };
        git(&["init", "--initial-branch=master"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        fs::write(
            work.path().join("pyproject.toml"),
            decode(&case["initial_project_b64"]),
        )
        .unwrap();
        git(&["add", "pyproject.toml"]);
        git(&["commit", "-m", "Fixture"]);
        for tag in case["initial_tags"].as_array().unwrap() {
            git(&["tag", tag.as_str().unwrap()]);
        }
        if case["branch"] != "master" {
            git(&["checkout", "-b", case["branch"].as_str().unwrap()]);
        }
        assert_eq!(
            String::from_utf8(git(&["rev-parse", "HEAD"]))
                .unwrap()
                .trim(),
            case["before"]["head"].as_str().unwrap()
        );
        let result = command(
            vec!["/usr/bin/bash".into(), script.clone().into_os_string()],
            work.path(),
            vec![],
            &env,
        );
        fs::write(evidence.join(format!("effect-{}.json",case["name"].as_str().unwrap())),serde_json::to_vec_pretty(&json!({"status":result.code,"stdout_b64":STANDARD.encode(&result.stdout),"stderr_b64":STANDARD.encode(&result.stderr)})).unwrap()).unwrap();
        assert_eq!(
            result.code,
            case["invocation"]["status"].as_i64().unwrap() as i32
        );
        assert_eq!(result.stdout, decode(&case["invocation"]["stdout_b64"]));
        if case["invocation"]["status"] == 0 {
            assert_eq!(result.stderr, decode(&case["invocation"]["stderr_b64"]));
        } else {
            let categories = regex::Regex::new(r"(?m)^([\w.]+(?:Error|Exception)):").unwrap();
            let source = decode(&case["invocation"]["stderr_b64"]);
            let source = String::from_utf8_lossy(&source);
            let native = String::from_utf8_lossy(&result.stderr);
            let collect = |text: &str| {
                categories
                    .captures_iter(text)
                    .map(|c| c[1].rsplit('.').next().unwrap().to_owned())
                    .collect::<Vec<_>>()
            };
            assert_eq!(collect(&native), collect(&source));
            assert!(!result.stdout.windows(4).any(|w| w == b"PASS"));
        }
        let after = json!({"head":String::from_utf8(git(&["rev-parse","HEAD"])).unwrap().trim(),"tags":String::from_utf8(git(&["tag","--list"])).unwrap().lines().collect::<Vec<_>>(),"status_b64":STANDARD.encode(git(&["status","--porcelain"])),"project_b64":STANDARD.encode(fs::read(work.path().join("pyproject.toml")).unwrap()),"commits_b64":STANDARD.encode(git(&["log","--format=%s"]))});
        assert_eq!(after, case["after"], "{}", case["name"]);
        observed_effects.push(json!({"name":case["name"],"after":after,"status":result.code,"stdout_b64":STANDARD.encode(&result.stdout),"stderr_b64":STANDARD.encode(&result.stderr)}));
    }
    verify();
    assert_eq!(sha(&fs::read(cli).unwrap()), cli_before);
    assert_eq!(sha(&fs::read(script).unwrap()), script_before);
    fs::write(
        evidence.join("native-computations.json"),
        serde_json::to_vec_pretty(&actual).unwrap(),
    )
    .unwrap();
    fs::write(
        evidence.join("native-effects.json"),
        serde_json::to_vec_pretty(&observed_effects).unwrap(),
    )
    .unwrap();
}
