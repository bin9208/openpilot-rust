use openpilot_dev_tools::user_docs::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn options(paths: &[&str]) -> Options {
    let mut options = Options::new(root());
    options.changed = paths.iter().map(|s| (*s).into()).collect();
    options
}
fn check_paths(paths: &[&str], body: &str) -> bool {
    check(&options(paths), body, &mut Vec::new(), &mut Vec::new()).unwrap()
}
#[test]
fn settings_require_both_languages_and_concrete_exceptions() {
    let code = "openpilot/selfdrive/carrot_settings.json";
    assert!(!check_paths(&[code], ""));
    assert!(!check_paths(&[code, "docs/user/ko/settings.md"], ""));
    assert!(check_paths(
        &[code, "docs/user/ko/settings.md", "docs/user/en/settings.md"],
        ""
    ));
    assert!(check_paths(
        &[code],
        "Docs-Not-Needed: Internal refactor with unchanged settings behavior."
    ));
    assert!(!check_paths(&[code], "Docs-Not-Needed: N/A"));
}
#[test]
fn mapping_rule_sets_and_independence_are_preserved() {
    let mapping = load_mapping(&options(&[]).map).unwrap();
    for id in [
        "settings-catalog-and-web",
        "cruise-buttons-and-shared-control",
        "radar",
        "tesla-vehicle-integration",
    ] {
        assert!(mapping.rules.iter().any(|r| r.id == id));
    }
    for (code, ko, en, id) in [
        (
            "openpilot/selfdrive/controls/radard.py",
            "docs/user/ko/radar.md",
            "docs/user/en/radar.md",
            "radar",
        ),
        (
            "openpilot/selfdrive/car/cruise.py",
            "docs/user/ko/buttons-presets.md",
            "docs/user/en/buttons-presets.md",
            "cruise-buttons-and-shared-control",
        ),
        (
            "opendbc_repo/opendbc/car/tesla/carstate.py",
            "docs/user/ko/tesla.md",
            "docs/user/en/tesla.md",
            "tesla-vehicle-integration",
        ),
    ] {
        let changed = vec![code.into(), ko.into()];
        assert!(find_missing_docs(&changed, &mapping)
            .iter()
            .any(|(r, _)| r.id == id));
        let mut changed = changed;
        changed.push(en.into());
        assert!(find_missing_docs(&changed, &mapping).is_empty());
        changed.push("openpilot/selfdrive/carrot_settings.json".into());
        assert_eq!(
            find_missing_docs(&changed, &mapping)
                .iter()
                .map(|(r, _)| r.id.as_str())
                .collect::<Vec<_>>(),
            ["settings-catalog-and-web"]
        );
    }
}
#[test]
fn advisory_does_not_hide_settings_and_writes_review_message() {
    let mut out = Vec::new();
    let opts = options(&[
        "openpilot/selfdrive/carrot/radar_motion/primary.py",
        "openpilot/selfdrive/carrot/web/src/features/tools/egpu_model.js",
    ]);
    assert!(check(&opts, "", &mut out, &mut Vec::new()).unwrap());
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("Docs review suggested [radar]"));
    assert!(out.contains("Docs review suggested [carrot-web-pages]"));
    assert!(!check_paths(
        &[
            "openpilot/selfdrive/carrot/radar_motion/primary.py",
            "openpilot/selfdrive/carrot_settings.json"
        ],
        ""
    ));
}
#[test]
fn reason_does_not_consume_the_next_line_or_count_utf8_bytes() {
    for body in [
        "Docs-Not-Needed:\nUnrelated instructions",
        "Docs-Not-Needed:\r\nUnrelated instructions",
        "Docs-Not-Needed: 이유없음",
        "Docs-Not-Needed: <reason>",
        "Docs-Not-Needed: 해당 없음",
        "Docs-Not-Needed:\nDocs-Not-Needed: A later valid explanation",
    ] {
        assert_eq!(extract_override_reason(body), "", "{body}");
    }
    assert_eq!(
        extract_override_reason(" \tdOcS-nOt-NeEdEd \t: Internal formatting change only.\r\n"),
        "Internal formatting change only."
    );
    assert!(
        !extract_override_reason("Docs-Not-Needed: 내부 함수만 분리했고 사용자 동작은 동일함")
            .is_empty()
    );
}
#[test]
fn globs_match_case_sensitive_unicode_paths_and_slashes() {
    for (path, pattern, expected) in [
        (" ./a\\b/한글.rs ", "a/*.rs", true),
        ("a/b.rs", "a/**.rs", true),
        (".hidden", "*", true),
        ("a/b.rs", "a/?.rs", true),
        ("a/한.rs", "a/?.rs", true),
        ("A/b.rs", "a/*", false),
        ("b", "[a-c]", true),
        ("d", "[!a-c]", true),
        ("b", "[!a-c]", false),
        ("]", "[]]", true),
        ("[", "[", true),
        ("a", "[z-a]", false),
        ("-", "[-a]", true),
        ("^", "[^a]", true),
        ("b", "[^a]", false),
    ] {
        assert_eq!(matches(path, pattern), expected, "{path} / {pattern}");
    }
}
#[test]
fn malformed_mapping_and_incompatible_options_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    for content in [
        r#"{"version":2,"rules":[]}"#,
        r#"{"version":1,"rules":[{"id":"x"}]}"#,
        r#"{"version":1,"rules":[{"id":"x","description":"x","code_paths":["*"],"doc_sets":[[]]}]}"#,
        r#"{"version":1,"rules":[{"id":"x","description":"x","code_paths":["*"],"doc_sets":[["docs"]],"enforcement":"disabled"}]}"#,
    ] {
        let path = directory.path().join("map.json");
        fs::write(&path, content).unwrap();
        assert!(load_mapping(&path).is_err());
    }
    let mut opts = options(&["x"]);
    opts.commit_overrides = true;
    assert!(check(&opts, "", &mut Vec::new(), &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("Git change range"));
}
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn commit(root: &Path, path: &str, content: &str, message: &str) -> String {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, content).unwrap();
    git(root, &["add", path]);
    git(root, &["commit", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}
fn repo() -> (tempfile::TempDir, Options) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "Docs checker test"]);
    git(
        dir.path(),
        &["config", "user.email", "docs-test@example.com"],
    );
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    git(dir.path(), &["config", "core.hooksPath", "/dev/null"]);
    let mut opts = options(&[]);
    opts.root = dir.path().into();
    opts.base = commit(dir.path(), "README", "base", "Base commit");
    (dir, opts)
}
fn passes(opts: &Options) -> bool {
    check(opts, "", &mut Vec::new(), &mut Vec::new()).unwrap()
}
const SETTINGS: &str = "openpilot/selfdrive/carrot_settings.json";
const REASON: &str = "Refactor\n\nDocs-Not-Needed: Settings behavior is unchanged.";
#[test]
fn push_exemption_is_opt_in_and_only_covers_its_commit() {
    for exempt_first in [true, false] {
        let (dir, mut opts) = repo();
        commit(
            dir.path(),
            SETTINGS,
            "one",
            if exempt_first {
                REASON
            } else {
                "Change default"
            },
        );
        assert!(!passes(&opts));
        opts.commit_overrides = true;
        assert_eq!(passes(&opts), exempt_first);
        commit(
            dir.path(),
            SETTINGS,
            "two",
            if exempt_first {
                "Change default"
            } else {
                REASON
            },
        );
        assert!(!passes(&opts));
    }
}
#[test]
fn later_bilingual_docs_cover_the_push() {
    let (dir, mut opts) = repo();
    opts.commit_overrides = true;
    commit(dir.path(), SETTINGS, "one", "Change default");
    commit(
        dir.path(),
        "docs/user/ko/settings.md",
        "ko",
        "Document Korean",
    );
    assert!(!passes(&opts));
    commit(
        dir.path(),
        "docs/user/en/settings.md",
        "en",
        "Document English",
    );
    assert!(passes(&opts));
}
#[test]
fn first_parent_merge_does_not_inherit_feature_commit_waiver() {
    let (dir, mut opts) = repo();
    opts.commit_overrides = true;
    git(dir.path(), &["checkout", "-b", "feature"]);
    commit(dir.path(), SETTINGS, "one", REASON);
    git(dir.path(), &["checkout", "main"]);
    commit(dir.path(), "README", "main", "Main work");
    git(
        dir.path(),
        &["merge", "--no-ff", "feature", "-m", "Merge feature"],
    );
    assert!(!passes(&opts));
}
#[test]
fn empty_zero_and_missing_base_fall_back_to_head_parent() {
    let (dir, mut opts) = repo();
    commit(dir.path(), SETTINGS, "one", "Change default");
    for base in [
        "".into(),
        "0".repeat(40),
        "0".repeat(64),
        "missing-ref".into(),
    ] {
        opts.base = base;
        assert!(!passes(&opts));
    }
}
#[test]
fn native_cli_uses_exit_codes_and_does_not_require_python() {
    let result = Command::new(env!("CARGO_BIN_EXE_check-user-docs"))
        .env_remove("PR_BODY")
        .env("PATH", "/nonexistent")
        .args(["--changed-file", SETTINGS])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("Required user-documentation update is missing"));
    let invalid = Command::new(env!("CARGO_BIN_EXE_check-user-docs"))
        .arg("--invalid")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
}

#[test]
fn python_whitespace_cannot_disguise_a_placeholder_docs_waiver() {
    for separator in ['\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}'] {
        let body = format!("Docs-Not-Needed: {separator}<reason>{separator}");
        assert_eq!(extract_override_reason(&body), "", "{separator:?}");
        assert!(!check_paths(&[SETTINGS], &body));
        let path = format!("{separator} ./openpilot/selfdrive/carrot_settings.json {separator}");
        assert_eq!(normalize_path(&path), SETTINGS);
        assert!(!check_paths(&[&path], ""));
    }
}

#[test]
fn unicode_casefold_cannot_disguise_a_placeholder_docs_waiver() {
    let body = "Docs-Not-Needed: <reaſon>";
    assert_eq!(extract_override_reason(body), "");
    assert!(!check_paths(&[SETTINGS], body));
}
