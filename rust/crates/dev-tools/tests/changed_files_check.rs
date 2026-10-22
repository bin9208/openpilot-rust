#![cfg(all(target_os = "linux", feature = "changed-files-native"))]
use openpilot_dev_tools::changed_files_check::{check_bytes, Phase};
use serde::Deserialize;
use serde_json::{json, value::RawValue};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    os::unix::{ffi::OsStrExt, fs::symlink},
    path::{Path, PathBuf},
    process::Command,
};

fn fixture(name: &str, expected: &str) -> Vec<u8> {
    let dir = PathBuf::from(
        std::env::var_os("AST_ORIGINAL_DIR").expect("mandatory authenticated original directory"),
    );
    let bytes = fs::read(dir.join(name)).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        expected,
        "immutable original {name}"
    );
    bytes
}
fn save(name: &str, value: &serde_json::Value) {
    let root =
        PathBuf::from(std::env::var_os("AST_PROOF").expect("mandatory actual-output directory"));
    let target = root.join(name);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn hex(raw: &str) -> Vec<u8> {
    assert_eq!(raw.len() % 2, 0);
    raw.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn hex_bytes(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Deserialize)]
struct SyntaxRow {
    name: String,
    source_hex: String,
    accepted: bool,
}
#[derive(Deserialize)]
struct SyntaxCorpus {
    rows: Vec<SyntaxRow>,
}
#[test]
fn all_72_original_raw_byte_admissions_use_actual_python312_syntax() {
    let raw = fixture(
        "syntax-observations.json",
        "b074376fd12f728bc3d45739e1c3b3d67b7070b7ed71f2b1a5f854d43ac74a75",
    );
    let corpus: SyntaxCorpus = serde_json::from_slice(&raw).unwrap();
    assert_eq!(corpus.rows.len(), 72);
    let mut rows = vec![];
    let mut differences = vec![];
    for row in corpus.rows {
        let result = check_bytes(&hex(&row.source_hex));
        let capability = result.as_ref().is_err_and(|e| e.phase == Phase::Capability);
        if capability || result.is_ok() != row.accepted {
            differences.push(row.name.clone());
        }
        rows.push(json!({"name":row.name,"source_hex":row.source_hex,"original_accepted":row.accepted,"native_accepted":result.is_ok(),"capability_error":capability,"native_diagnostic":result.err().map(|e|e.to_string())}));
    }
    save(
        "syntax-actual.json",
        &json!({"rows":rows,"differences":differences}),
    );
    // Regression derived from pinned CPython registry source, distinct from
    // the72 retained original observations. Dot fallback applies to aliases
    // only: utf.8 is not a module alias, while ansi_x3.4_1968 aliases ASCII.
    let mut registry = vec![];
    for (codec, expected) in [("utf.8", false), ("ansi_x3.4_1968", true)] {
        let source = format!("# coding: {codec}\nx=1\n");
        let actual = check_bytes(source.as_bytes());
        registry.push(json!({"codec":codec,"expected_from_pinned_registry":expected,"accepted":actual.is_ok(),"diagnostic":actual.as_ref().err().map(|e|e.to_string())}));
        save("registry-controls.json", &json!({"rows":registry}));
        assert_eq!(actual.is_ok(), expected, "registry dot boundary {codec}");
    }
    assert!(
        differences.is_empty(),
        "actual original byte-source mismatches: {differences:?}"
    );
}

#[derive(Deserialize)]
struct GitRow {
    name: String,
    base: String,
    argument: Option<String>,
    base_environment: Option<String>,
    selected_paths_hex: Vec<String>,
    accepted: bool,
    error_class: Option<String>,
    stdout: String,
    // Keep the frozen lone \udcff diagnostic opaque; Rust String cannot hold it.
    #[serde(default, rename = "error")]
    original_error: Option<Box<RawValue>>,
}
#[derive(Deserialize)]
struct GitCorpus {
    rows: Vec<GitRow>,
}
fn git(root: &Path, args: &[&OsStr]) -> Vec<u8> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn commit(root: &Path, message: &str) {
    git(root, &[OsStr::new("add"), OsStr::new(".")]);
    git(
        root,
        &[
            OsStr::new("commit"),
            OsStr::new("--allow-empty"),
            OsStr::new("-m"),
            OsStr::new(message),
        ],
    );
}
#[test]
fn all_twelve_original_git_lanes_keep_failure_order_raw_names_and_worktree_bytes() {
    let raw = fixture(
        "git-observations.json",
        "cd5947ae3b817eeae232e6b4d1b727e24c16a24afb716c8de3bc70773032f78a",
    );
    let corpus: GitCorpus = serde_json::from_slice(&raw).unwrap();
    assert_eq!(corpus.rows.len(), 12);
    let temp = tempfile::tempdir().unwrap();
    let mut observed = vec![];
    for row in corpus.rows {
        let root = temp.path().join(&row.name);
        fs::create_dir(&root).unwrap();
        git(&root, &[OsStr::new("init")]);
        git(
            &root,
            &[
                OsStr::new("config"),
                OsStr::new("user.name"),
                OsStr::new("CI original capture"),
            ],
        );
        git(
            &root,
            &[
                OsStr::new("config"),
                OsStr::new("user.email"),
                OsStr::new("ci@example.invalid"),
            ],
        );
        fs::write(root.join("base.txt"), b"base\n").unwrap();
        commit(&root, "base");
        git(
            &root,
            &[
                OsStr::new("update-ref"),
                OsStr::new("refs/remotes/origin/dev"),
                OsStr::new("HEAD"),
            ],
        );
        let base =
            String::from_utf8(git(&root, &[OsStr::new("rev-parse"), OsStr::new("HEAD")])).unwrap();
        let base = base.trim();
        let broken = b"def broken(:\n";
        match row.name.as_str() {
            "whitespace-before-python" => {
                fs::write(root.join("space.txt"), b"trailing \n").unwrap();
                fs::write(root.join("broken.py"), broken).unwrap();
            }
            "deleted-broken-python" => {
                fs::write(root.join("broken.py"), broken).unwrap();
                commit(&root, "temporary broken source");
                fs::remove_file(root.join("broken.py")).unwrap();
            }
            "symlink-python-skipped" => {
                fs::write(root.join("target.txt"), broken).unwrap();
                symlink("target.txt", root.join("broken.py")).unwrap();
            }
            "uppercase-suffix-skipped" => {
                fs::write(root.join("broken.PY"), broken).unwrap();
            }
            "raw-name-python" => {
                fs::write(root.join(OsStr::from_bytes(b"bad-\xff.py")), broken).unwrap();
            }
            "newline-name-python" => {
                fs::write(root.join("bad-\nname.py"), broken).unwrap();
            }
            "current-worktree-not-head" => {
                fs::write(root.join("broken.py"), b"x=1\n").unwrap();
            }
            "initial-earlier-syntax"
            | "empty-fallback"
            | "unset-fallback"
            | "explicit-base"
            | "env-explicit-base" => {
                fs::write(root.join("broken.py"), broken).unwrap();
            }
            other => panic!("unreviewed original Git scenario {other}"),
        }
        commit(&root, "earlier changed source");
        fs::write(root.join("last.txt"), b"last\n").unwrap();
        commit(&root, "later unrelated change");
        if row.name == "current-worktree-not-head" {
            fs::write(root.join("broken.py"), broken).unwrap();
        }
        let selected = git(
            &root,
            &[
                OsStr::new("diff"),
                OsStr::new("--name-only"),
                OsStr::new("-z"),
                OsStr::new("--diff-filter=ACMR"),
                OsStr::new(base),
                OsStr::new("HEAD"),
            ],
        );
        let paths = selected
            .split(|&b| b == 0)
            .filter(|p| !p.is_empty())
            .map(hex_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            paths, row.selected_paths_hex,
            "original path roster {}",
            row.name
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_check-changed-files"));
        cmd.current_dir(&root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        match &row.base_environment {
            None => {
                cmd.env_remove("BASE_SHA");
            }
            Some(v) => {
                cmd.env("BASE_SHA", if v == &row.base { base } else { v });
            }
        }
        if let Some(argument) = &row.argument {
            assert_eq!(argument, &row.base);
            cmd.args(["--base", base]);
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        observed.push(json!({"name":row.name,"original_accepted":row.accepted,"native_accepted":out.status.success(),"selected_paths_hex":paths,"stdout_hex":hex_bytes(&out.stdout),"stderr_hex":hex_bytes(&out.stderr),"original_error_raw":row.original_error.as_ref().map(|v| v.get())}));
        save("git-actual.json", &json!({"rows":observed}));
        assert_eq!(
            out.status.success(),
            row.accepted,
            "native Git lane {}: {stderr}",
            row.name
        );
        if row.accepted {
            assert_eq!(out.stdout, row.stdout.replace(&row.base, base).as_bytes());
        } else {
            assert!(
                !stderr.contains("Capability"),
                "unsupported input is not a Python rejection"
            );
            let phase = if row.error_class.as_deref() == Some("CalledProcessError") {
                "Whitespace"
            } else {
                "Syntax"
            };
            assert!(
                stderr.contains(phase),
                "native phase {}: {stderr}",
                row.name
            );
        }
    }
}

#[derive(Deserialize)]
struct UnicodeCorpus {
    xid_start_including_underscore: Vec<[u32; 2]>,
    xid_continue: Vec<[u32; 2]>,
}
fn in_original(ranges: &[[u32; 2]], code: u32) -> bool {
    let index = ranges.partition_point(|pair| pair[0] <= code);
    index != 0 && code <= ranges[index - 1][1]
}
#[test]
fn every_unicode_scalar_keeps_independent_python15_identifier_token_boundaries() {
    use openpilot_dev_tools::changed_files_check::identifier_is_python312;
    let raw = fixture(
        "unicode-tables.json",
        "0574107dd639e01c426d9497f38f1322881ece9e193805d7c96d9cd0a3a9b41b",
    );
    let corpus: UnicodeCorpus = serde_json::from_slice(&raw).unwrap();
    let mut differences = vec![];
    let mut count = 0u64;
    let mut total = 0u64;
    for code in 0..=0x10ffff {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        total += 1;
        // Suffix/prefix avoid Python keywords, retaining the exact scalar's
        // start/continue property and requiring one complete original Name token.
        for (source, expected, kind) in [
            (
                format!("{c}_a"),
                in_original(&corpus.xid_start_including_underscore, code),
                "start",
            ),
            (
                format!("a_{c}"),
                in_original(&corpus.xid_continue, code),
                "continue",
            ),
        ] {
            let actual = identifier_is_python312(&source);
            if actual != expected {
                count += 1;
                if differences.len() < 100 {
                    differences.push(
                        json!({"codepoint":code,"kind":kind,"original":expected,"native":actual}),
                    );
                }
            }
        }
    }
    save(
        "unicode-actual.json",
        &json!({"unicode_version":"15.0.0","scalar_count":total,"probes":total*2,"difference_count":count,"first_differences":differences}),
    );
    assert_eq!(total, 1_112_064);
    assert_eq!(count, 0, "{differences:?}");
}
