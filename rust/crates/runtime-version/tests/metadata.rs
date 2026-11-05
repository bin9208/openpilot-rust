use openpilot_runtime_version::{
    build_metadata_from_dict, git::normalize_origin, Error, JsonValue,
};

#[test]
fn defaults_when_keys_are_missing() {
    // Given an older build file with no metadata keys.
    let input = JsonValue::parse("{}").unwrap();
    // When constructing metadata.
    let build = build_metadata_from_dict(&input).unwrap();
    // Then missing values default and canonical/UI use those values.
    assert_eq!(
        build.canonical().unwrap().to_utf8().unwrap(),
        "unknown-unknown-unknown"
    );
    assert_eq!(
        build.ui_description().unwrap().to_utf8().unwrap(),
        "unknown / unknow / unknown"
    );
    assert!(!build.openpilot.is_dirty);
}
#[test]
fn nonstring_fields_are_retained_until_used() {
    // Given source-accepted nonstring fields.
    let input = JsonValue::parse(r#"{"channel":null,"openpilot":{"version":42,"git_commit":true,"git_origin":[],"build_style":{"x":1}}}"#).unwrap();
    // When constructing metadata.
    let build = build_metadata_from_dict(&input).unwrap();
    // Then f-string formatting succeeds while string/slice operations fail at access time.
    assert_eq!(
        build.canonical().unwrap().to_utf8().unwrap(),
        "42-True-{'x': 1}"
    );
    assert!(matches!(
        build.openpilot.short_version(),
        Err(Error::Attribute(_))
    ));
    assert!(matches!(build.ui_description(), Err(Error::Type(_))));
}
#[test]
fn slice_counts_unicode_points_when_commit_is_non_ascii() {
    // Given six multibyte characters and a seventh.
    let input = JsonValue::parse(r#"{"openpilot":{"git_commit":"한글😀abcd"}}"#).unwrap();
    // When formatting the source UI property.
    let output = build_metadata_from_dict(&input)
        .unwrap()
        .ui_description()
        .unwrap();
    // Then slicing follows Python characters, not bytes.
    assert_eq!(output.to_utf8().unwrap(), "unknown / 한글😀abc / unknown");
}
#[test]
fn normalization_replaces_only_first_occurrence_anywhere() {
    // Given patterns repeated outside URL prefixes/suffixes.
    let input = "xgit@git@host.git.githttps://https://:tail";
    // When applying source normalization.
    let output = normalize_origin(input);
    // Then each operation replaces once in its inherited order.
    assert_eq!(output, "xgit@host.githttps///:tail");
}

#[test]
#[ignore]
fn source_build_metadata_child() {
    use std::{fs, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("openpilot/common")).unwrap();
    fs::write(
        root.join("openpilot/common/version.h"),
        "#define COMMA_VERSION \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(root.join("RELEASES.md"), "Source build\n").unwrap();
    fs::write(root.join("build.json"), "stale invalid package metadata").unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(root)
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=ci@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-qm", "source"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/bin9208/openpilot-rust.git",
    ]);
    let head = git(&["rev-parse", "HEAD"]);
    std::env::set_current_dir(root).unwrap();
    let metadata = openpilot_runtime_version::get_build_metadata(root).unwrap();
    assert!(metadata.openpilot.git_commit.text_eq(&head));
    assert!(metadata.openpilot.version.text_eq("0.1.0"));
}

#[test]
fn source_build_reports_actual_git_commit_instead_of_old_package_metadata() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env("OPENPILOT_USER_SOURCE_BUILD", "1")
        .args([
            "--exact",
            "source_build_metadata_child",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
