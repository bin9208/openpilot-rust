use openpilot_updated::{
    common, markdown,
    paths::Paths,
    process,
    signals::{UserRequest, Wake},
    updater::remote_head,
};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    time::Duration,
};
#[test]
fn markdown_retains_original_nested_list_heading_and_escape_behavior() {
    // Given release-note syntax accepted by common/markdown.py.
    let text = "Title & \"quoted\"\n===\n* parent\n  * child\n* sibling\nplain";
    // When rendering the original subset.
    let html = markdown::parse(text, 2).unwrap();
    // Then nesting closes correctly and only source-defined escaping occurs.
    assert_eq!(html,"<h1>Title &amp; &quot;quoted&quot;</h1>\n<ul>\n<li>parent\n<ul>\n<li>child</li>\n</ul>\n</li>\n<li>sibling</li>\n</ul>\nplain\n");
}
#[test]
fn invalid_utf8_release_notes_use_raw_first_section_plus_newline() {
    // Given a release file with non-UTF8 bytes and an older section.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("RELEASES.md"), b"latest\xff\n\nold").unwrap();
    // When selecting the current release notes.
    let bytes = markdown::release_notes(root.path()).unwrap();
    // Then the unchanged source fallback preserves the raw current section.
    assert_eq!(bytes, b"latest\xff\n");
}
#[test]
fn copied_checkout_preserves_symlink_mode_and_only_publishes_explicit_flag() {
    // Given an owned checkout containing an executable and a relative symlink.
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("run"), b"payload").unwrap();
    std::fs::set_permissions(source.join("run"), std::fs::Permissions::from_mode(0o755)).unwrap();
    symlink("run", source.join("link")).unwrap();
    rustix::fs::setxattr(
        source.join("run"),
        "user.updated_fixture",
        b"preserved",
        rustix::fs::XattrFlags::empty(),
    )
    .unwrap();
    let paths = Paths {
        base: source.clone(),
        staging: root.path().join("stage"),
        lock: root.path().join("lock"),
        system_root: root.path().into(),
    };
    // When copying the view and setting/removing the explicit consistency flag.
    common::copy_tree(&source, &target).unwrap();
    assert!(!common::get_consistent_flag(&target));
    common::set_consistent_flag(&paths, &target, true).unwrap();
    assert!(common::get_consistent_flag(&target));
    common::set_consistent_flag(&paths, &target, false).unwrap();
    // Then links/mode survive and the marker is absent after invalidation.
    assert_eq!(
        std::fs::read_link(target.join("link")).unwrap(),
        std::path::Path::new("run")
    );
    assert_eq!(
        std::fs::metadata(target.join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    let mut attribute = [0; 16];
    let length = rustix::fs::getxattr(
        target.join("run"),
        "user.updated_fixture",
        &mut attribute[..],
    )
    .unwrap();
    assert_eq!(&attribute[..length], b"preserved");
    assert_eq!(
        std::fs::metadata(source.join("run"))
            .unwrap()
            .modified()
            .unwrap(),
        std::fs::metadata(target.join("run"))
            .unwrap()
            .modified()
            .unwrap()
    );
    assert!(!common::get_consistent_flag(&target));
}
#[test]
fn remote_refs_exclude_legacy_branches_and_reject_bad_hashes() {
    // Given ordinary, excluded and malformed remote output.
    let lines = [
        "abcdef123 refs/heads/dev",
        "abcdef refs/heads/release2",
        "ABCDEF refs/heads/dev",
        "abcd refs/heads/dev",
    ];
    // When parsing through the source full-match rules.
    let values: Vec<_> = lines.iter().map(|line| remote_head(line)).collect();
    // Then only the lowercase valid nonlegacy branch is accepted.
    assert_eq!(values, [Some(("abcdef123", "dev")), None, None, None]);
}
#[test]
fn command_env_appends_overrides_without_reusing_existing_indices() {
    // Given two inherited Git command-scope settings.
    let environment = process::command_environment("2").unwrap();
    // When examining the additional override entries.
    let pairs: Vec<_> = environment
        .iter()
        .map(|(key, value)| (key.to_str().unwrap(), value.to_str().unwrap()))
        .collect();
    // Then earlier indices remain untouched and the count includes all three overrides.
    assert_eq!(pairs[0], ("GIT_CONFIG_KEY_2", "gc.auto"));
    assert_eq!(pairs.last(), Some(&("GIT_CONFIG_COUNT", "5")));
}
#[test]
fn signal_request_survives_ready_clear_until_cycle_completion() {
    // Given a pending fetch request, as during the initial warmup wait.
    let wake = Wake::default();
    wake.send(UserRequest::Fetch).unwrap();
    // When the next iteration clears only the ready event.
    wake.clear_ready().unwrap();
    // Then the request persists; stopping unblocks a long wait immediately.
    assert_eq!(wake.request().unwrap(), UserRequest::Fetch);
    wake.stop();
    wake.sleep(Duration::from_secs(60)).unwrap();
    assert!(wake.stopped());
}

#[test]
fn stored_time_preserves_timezone_awareness_and_invalid_values_are_absent() {
    // Given real owned Params values with an offset or invalid timestamp.
    let root = tempfile::tempdir().unwrap();
    let params = openpilot_updated::Params::open(root.path()).unwrap();
    params
        .put("UpdaterLastFetchTime", b"2026-10-01T00:00:00+00:00")
        .unwrap();
    // When parsing through the updater's typed time boundary.
    let parsed = params.date("UpdaterLastFetchTime").unwrap();
    // Then an aware datetime stays distinct from the naive UTC loop clock.
    assert!(matches!(
        parsed,
        Some(openpilot_updated::ParamTime::Aware(_))
    ));
    params.put("InstallDate", b"invalid").unwrap();
    assert!(params.date("InstallDate").unwrap().is_none());
}
