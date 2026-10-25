//! Source-first adversarial tests. UNRUN locally; Actions owns execution.
use super::*;
use std::os::unix::{
    ffi::OsStringExt,
    fs::{symlink, PermissionsExt},
};

fn source_guard_fixture(present: bool) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir_all(root.path().join("rust/tools")).unwrap();
    if present {
        for name in SOURCE_LEAVES {
            // Guard metadata only: no original Python body or functional expected.
            fs::write(root.path().join(name), b"inert source marker\n").unwrap();
        }
    }
    root
}

#[test]
fn source_policy_schema_defaults_and_rejects_unknown_values() {
    assert_eq!(SourcePolicy::parse(None).unwrap(), SourcePolicy::Present);
    for (text, expected) in [
        ("present", SourcePolicy::Present),
        ("absent", SourcePolicy::Absent),
    ] {
        assert_eq!(
            SourcePolicy::parse(Some(std::ffi::OsStr::new(text))).unwrap(),
            expected
        );
        assert_eq!(expected.as_str(), text);
    }
    for value in [
        OsString::from(""),
        "auto".into(),
        "ABSENT".into(),
        OsString::from_vec(vec![0xff]),
    ] {
        let error = SourcePolicy::parse(Some(&value)).unwrap_err();
        assert!(error.is::<crate::argparse_scan::ArgumentError>());
        assert_eq!(
            error.to_string(),
            "--source-policy must be present or absent"
        );
    }
}

#[test]
fn source_policy_present_markers_and_absent_root_are_guard_metadata_only() {
    let root = source_guard_fixture(true);
    let guard = SourceGuard::open(root.path(), SourcePolicy::Present).unwrap();
    guard.verify().unwrap();
    assert_eq!(guard.root(), root.path());
    // Legacy authored argv uses this existing parent-component spelling.
    SourceGuard::open(&root.path().join("rust/tools/../.."), SourcePolicy::Present)
        .unwrap()
        .verify()
        .unwrap();
    let absent = source_guard_fixture(false);
    let guard = SourceGuard::open(absent.path(), SourcePolicy::Absent).unwrap();
    guard.verify().unwrap();
    assert!(SOURCE_LEAVES
        .iter()
        .all(|name| fs::symlink_metadata(absent.path().join(name)).is_err()));
}

#[test]
fn source_policy_present_detects_rewrite_replacement_and_hardlink_alias() {
    for change in ["rewrite", "replace", "hardlink"] {
        let root = source_guard_fixture(true);
        let path = root.path().join(SOURCE_LEAVES[0]);
        let guard = SourceGuard::open(root.path(), SourcePolicy::Present).unwrap();
        match change {
            "rewrite" => fs::write(&path, b"other source marker\n").unwrap(),
            "replace" => {
                fs::rename(&path, root.path().join("old-inode")).unwrap();
                fs::write(&path, b"inert source marker\n").unwrap();
            }
            "hardlink" => fs::hard_link(&path, root.path().join("alias")).unwrap(),
            _ => unreachable!(),
        }
        let expected = format!("logging source present file changed: {}", SOURCE_LEAVES[0]);
        assert_eq!(guard.verify().unwrap_err().to_string(), expected);
        assert_eq!(guard.verify().unwrap_err().to_string(), expected);
        let first: Result<()> = Err("original command failure".into());
        assert_eq!(
            guard.complete(first).unwrap_err().to_string(),
            "original command failure"
        );
    }
}

#[test]
fn source_policy_absent_rejects_nodes_and_keeps_reappearance_sticky() {
    for name in SOURCE_LEAVES {
        for kind in ["regular", "directory", "symlink", "dangling"] {
            let root = source_guard_fixture(false);
            let path = root.path().join(name);
            let guard = SourceGuard::open(root.path(), SourcePolicy::Absent).unwrap();
            match kind {
                "regular" => fs::write(&path, b"reappeared inert entry").unwrap(),
                "directory" => fs::create_dir(&path).unwrap(),
                "symlink" => symlink(root.path(), &path).unwrap(),
                "dangling" => symlink("missing-target", &path).unwrap(),
                _ => unreachable!(),
            }
            let expected = format!("logging retired source reappeared: {name}");
            assert_eq!(guard.verify().unwrap_err().to_string(), expected);
            let rejected = SourceGuard::open(root.path(), SourcePolicy::Absent)
                .err()
                .unwrap();
            assert_eq!(rejected.to_string(), expected);
            if kind == "directory" {
                fs::remove_dir(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
            assert_eq!(guard.verify().unwrap_err().to_string(), expected);
            assert_eq!(guard.complete(Ok(())).unwrap_err().to_string(), expected);
        }
    }
}

#[test]
fn source_policy_parent_alias_replacement_and_special_leaf_fail_closed() {
    let replacement = source_guard_fixture(false);
    let replacement_guard = SourceGuard::open(replacement.path(), SourcePolicy::Absent).unwrap();
    fs::rename(
        replacement.path().join("rust/tools"),
        replacement.path().join("original-tools"),
    )
    .unwrap();
    fs::create_dir(replacement.path().join("rust/tools")).unwrap();
    assert_eq!(
        replacement_guard.verify().unwrap_err().to_string(),
        SOURCE_PARENT_ERROR
    );
    let permissions = source_guard_fixture(false);
    let permissions_guard = SourceGuard::open(permissions.path(), SourcePolicy::Absent).unwrap();
    fs::set_permissions(
        permissions.path().join("rust/tools"),
        fs::Permissions::from_mode(0),
    )
    .unwrap();
    let error = permissions_guard.verify().unwrap_err().to_string();
    fs::set_permissions(
        permissions.path().join("rust/tools"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    assert_eq!(error, SOURCE_PARENT_ERROR);
    let root = source_guard_fixture(false);
    let guard = SourceGuard::open(root.path(), SourcePolicy::Absent).unwrap();
    fs::rename(
        root.path().join("rust/tools"),
        root.path().join("old-tools"),
    )
    .unwrap();
    symlink(
        root.path().join("old-tools"),
        root.path().join("rust/tools"),
    )
    .unwrap();
    assert_eq!(
        guard.verify().unwrap_err().to_string(),
        "logging source parent boundary invalid"
    );
    assert_eq!(
        SourceGuard::open(root.path(), SourcePolicy::Absent)
            .err()
            .unwrap()
            .to_string(),
        "logging source parent boundary invalid"
    );
    fs::remove_file(root.path().join("rust/tools")).unwrap();
    fs::create_dir(root.path().join("rust/tools")).unwrap();
    assert_eq!(
        guard.verify().unwrap_err().to_string(),
        "logging source parent boundary invalid"
    );
    let alias = root.path().join("root-alias");
    symlink(root.path(), &alias).unwrap();
    assert_eq!(
        SourceGuard::open(&alias, SourcePolicy::Absent)
            .err()
            .unwrap()
            .to_string(),
        "logging source parent boundary invalid"
    );
    for policy in [SourcePolicy::Present, SourcePolicy::Absent] {
        let root = source_guard_fixture(policy == SourcePolicy::Present);
        let path = root.path().join(SOURCE_LEAVES[0]);
        if policy == SourcePolicy::Present {
            fs::remove_file(&path).unwrap();
        }
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &path,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(SourceGuard::open(root.path(), policy).is_err());
    }
}

#[test]
fn source_policy_admission_keeps_required_help_unknown_and_occupied_priorities() {
    assert!(run_args(&["--help".into(), "--source-policy".into(), "unknown".into()]).is_ok());
    assert_eq!(
        run_args(&["--source-policy".into(), "absent".into()])
            .unwrap_err()
            .to_string(),
        "--binary required"
    );
    let root = source_guard_fixture(false);
    let output = root.path().join("occupied");
    fs::write(&output, b"foreign").unwrap();
    let mut args: Vec<OsString> = vec![
        "--root".into(),
        root.path().as_os_str().to_owned(),
        "--binary".into(),
        "/missing/provider".into(),
        "--context".into(),
        "/missing/context".into(),
        "--output".into(),
        output.clone().into_os_string(),
        "--source-commit".into(),
        "a".repeat(40).into(),
        "--binary-sha256".into(),
        "b".repeat(64).into(),
        "--source-policy".into(),
        "unknown".into(),
    ];
    assert_eq!(
        run_args(&args).unwrap_err().to_string(),
        "logging evidence output already exists"
    );
    args.extend(["--unknown-source-option".into()]);
    assert_eq!(
        run_args(&args).unwrap_err().to_string(),
        "unrecognized arguments"
    );
    assert_eq!(fs::read(output).unwrap(), b"foreign");
}

#[test]
fn dispatch_requires_the_first_literal_not_a_later_argument() {
    assert!(selected(&["retained".into(), "--help".into()]));
    for args in [
        vec![],
        vec!["--retained".into()],
        vec!["--root".into(), "retained".into()],
    ] {
        assert!(!selected(&args));
    }
}

#[test]
fn retained_parser_refuses_builder_reference_and_source_hook_options() {
    for forbidden in ["--builder", "--reference", "--source-hook"] {
        let args: Vec<OsString> = vec![
            "--root".into(),
            "/missing/source".into(),
            "--binary".into(),
            "/missing/provider".into(),
            "--context".into(),
            "/missing/context".into(),
            "--output".into(),
            "/missing/output".into(),
            "--source-commit".into(),
            "a".repeat(40).into(),
            "--binary-sha256".into(),
            "b".repeat(64).into(),
        ];
        let suffixes: [Vec<OsString>; 3] = [
            vec![forbidden.into(), "/must-not-execute".into()],
            vec![format!("{forbidden}=/must-not-execute").into()],
            vec![OsString::from_vec(
                [format!("{forbidden}=").into_bytes(), vec![0xff]].concat(),
            )],
        ];
        for suffix in suffixes {
            let mut rejected = args.clone();
            rejected.extend(suffix);
            let error = run_args(&rejected).unwrap_err();
            assert!(error.is::<crate::argparse_scan::ArgumentError>());
            assert_eq!(
                error.to_string(),
                format!("unrecognized arguments: {forbidden}")
            );
        }
    }
    assert_eq!(
        run_args(&["--builder".into(), "/must-not-execute".into()])
            .unwrap_err()
            .to_string(),
        "--binary required"
    );
    assert!(run_args(&[
        "--help".into(),
        "--builder".into(),
        "/must-not-execute".into()
    ])
    .is_ok());
}

#[test]
fn help_needs_neither_source_context_provider_nor_output() {
    assert!(run_args(&["--help".into()]).is_ok());
}

#[test]
fn known_archived_cpp_and_rust_provider_pins_are_rejected_before_launch() {
    for sha in [
        "2a7e6e8e4a6450de1edc9c97ebb834e2c5054f0a27601fca93b83107039fff10",
        "d540c13737a12ff46215fade79aa81ec705b7fe4ab37297890cde9f5d3a0d4b6",
        "843f50b05c9cb025de1dd074990dbbb2c076a620edf7defe21734665a126f760",
        "837e6e106882bb296d7de69a5c14bd93e3edab22faf51b594e537eaa70275b91",
    ] {
        assert!(fresh_provider_pin(sha).is_err());
    }
    assert!(fresh_provider_pin("not-a-pin").is_err());
}

#[test]
fn occupied_output_precedes_missing_context_provider_source_or_git() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("foreign");
    fs::write(&output, b"foreign report bytes").unwrap();
    let args = vec![
        "--root".into(),
        temp.path().join("missing-source").into_os_string(),
        "--binary".into(),
        temp.path().join("missing-provider").into_os_string(),
        "--context".into(),
        temp.path().join("missing-context").into_os_string(),
        "--output".into(),
        output.clone().into_os_string(),
        "--source-commit".into(),
        "a".repeat(40).into(),
        "--binary-sha256".into(),
        "b".repeat(64).into(),
    ];
    let error = run_args(&args).unwrap_err().to_string();
    assert!(error.contains("output already exists"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), b"foreign report bytes");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn dangling_output_and_aliased_parent_are_rejected_without_touching_target() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let output = temp.path().join("output");
    symlink(&missing, &output).unwrap();
    assert!(absent_output(&output).is_err());
    assert!(!missing.exists());
    fs::remove_file(output).unwrap();
    symlink(temp.path(), temp.path().join("alias")).unwrap();
    assert!(absent_output(&temp.path().join("alias/new")).is_err());
    assert!(!temp.path().join("new").exists());
}

#[test]
fn member_names_reject_traversal_alias_spellings_and_executable_roles() {
    for name in [
        "/absolute",
        "../outside",
        "a/../b",
        "a//b",
        "a/./b",
        "a/",
        "",
        "a\\b",
        "a\0b",
        "a\nb",
    ] {
        assert!(member_name(name).is_err(), "accepted {name:?}");
    }
    assert!(member_name("recovered/console-0/invocation.json").is_ok());
    assert!(expected_members()
        .iter()
        .all(|(name, _, _)| !name.ends_with(".py") && !name.ends_with(".so")));
}

#[test]
fn duplicate_manifest_keys_and_unknown_source_hooks_are_not_last_value_wins() {
    assert!(parse_index(br#"{"schema":1,"schema":1}"#)
        .unwrap_err()
        .to_string()
        .contains("duplicate field"));
    assert!(parse_index(
        br#"{"schema":1,"members":{"x":{"size":1,"sha256":"a"},"x":{"size":1,"sha256":"a"}}}"#
    )
    .unwrap_err()
    .to_string()
    .contains("duplicate member"));
    assert!(parse_index(br#"{"schema":1,"source_hook":"run-me"}"#)
        .unwrap_err()
        .to_string()
        .contains("unknown field"));
}

#[test]
fn exact_member_roster_rejects_missing_extra_or_forged_pins() {
    let mut members = expected_members()
        .iter()
        .map(|(name, size, sha)| {
            (
                (*name).to_owned(),
                Pin {
                    size: *size,
                    sha256: (*sha).to_owned(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert!(validate_member_roster(&members).is_ok());
    let first = members.keys().next().unwrap().clone();
    let saved = members.remove(&first).unwrap();
    assert!(validate_member_roster(&members).is_err());
    members.insert(first.clone(), saved);
    members.get_mut(&first).unwrap().sha256 = "0".repeat(64);
    assert!(validate_member_roster(&members).is_err());
    members.insert(
        "recovered/execute-this".into(),
        Pin {
            size: 0,
            sha256: "0".repeat(64),
        },
    );
    assert!(validate_member_roster(&members).is_err());
}

#[test]
fn pinned_content_is_rechecked_not_accepted_as_a_new_baseline_after_admission() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("input");
    let index = temp.path().join("context.json");
    fs::write(&data, b"first").unwrap();
    fs::write(&index, b"{}").unwrap();
    let held = HeldFile::open(&data, 100).unwrap();
    let index = HeldFile::open(&index, 100).unwrap();
    let index_before = index.snapshot().unwrap();
    let inputs = Inputs {
        held: BTreeMap::from([("input".into(), held)]),
        pins: BTreeMap::from([(
            "input".into(),
            Pin {
                size: 5,
                sha256: "a7937b64b8caa58f03721bb6bacf5c78cb235febe0e70b1b84cd99541461a08e".into(),
            },
        )]),
        before: Value::Null,
        index,
        index_before,
        names: BTreeSet::from(["input".into(), "context.json".into()]),
        directory: temp.path().into(),
    };
    assert!(inputs.snapshot().is_ok());
    fs::write(&data, b"other").unwrap();
    assert!(inputs.snapshot().is_err());
}

#[test]
fn success_receipt_create_new_cannot_overwrite_foreign_bytes() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("report.json"), b"foreign receipt").unwrap();
    let (directory, _) = walk(temp.path(), true).unwrap();
    assert!(save(&directory, "report.json", &json!({"result":"pass"})).is_err());
    assert_eq!(
        fs::read(temp.path().join("report.json")).unwrap(),
        b"foreign receipt"
    );
}

#[test]
fn receipt_fd_cannot_be_redirected_by_renaming_and_replacing_output_directory() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    fs::create_dir(&output).unwrap();
    let (directory, _) = walk(&output, true).unwrap();
    fs::rename(&output, temp.path().join("owned-old")).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(output.join("report.json"), b"foreign").unwrap();
    save(&directory, "report.json", &json!({"result":"scoped"})).unwrap();
    assert_eq!(fs::read(output.join("report.json")).unwrap(), b"foreign");
    assert!(temp.path().join("owned-old/report.json").is_file());
    assert!(save(&directory, "../escape", &Value::Null).is_err());
    assert!(!temp.path().join("escape").exists());
}

#[test]
fn context_root_must_be_private_before_missing_index_is_opened() {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let error = match Inputs::open(&temp.path().join("missing.json"), temp.path()) {
        Ok(_) => panic!("public context root accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("private0700"));
}

#[test]
fn held_input_detects_content_change_inode_replacement_and_hardlink_alias() {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = temp.path().join("input");
    fs::write(&path, b"inert bytes").unwrap();
    let held = HeldFile::open(&path, 100).unwrap();
    let before = held.snapshot().unwrap();
    fs::write(&path, b"other bytes").unwrap();
    assert_ne!(held.snapshot().unwrap(), before);
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"inert bytes").unwrap();
    assert!(held.snapshot().is_err());
    fs::hard_link(&path, temp.path().join("alias")).unwrap();
    assert!(HeldFile::open(&path, 100).is_err());
}

#[test]
fn held_input_rejects_parent_symlink_fifo_and_over_bound_without_reading_them() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("regular"), b"1234").unwrap();
    symlink(temp.path(), temp.path().join("alias")).unwrap();
    assert!(HeldFile::open(&temp.path().join("alias/regular"), 100).is_err());
    assert!(HeldFile::open(&temp.path().join("regular"), 3).is_err());
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        temp.path().join("fifo"),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    assert!(HeldFile::open(&temp.path().join("fifo"), 100).is_err());
}

#[test]
fn directory_anchor_detects_renamed_input_parent_even_if_leaf_bytes_match() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("data");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("input"), b"data").unwrap();
    let held = HeldFile::open(&parent.join("input"), 100).unwrap();
    fs::rename(&parent, temp.path().join("old")).unwrap();
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("input"), b"data").unwrap();
    assert!(held.snapshot().is_err());
}

#[test]
fn comparison_cannot_pass_empty_missing_reordered_or_changed_payloads() {
    let a = vec![b"first".to_vec(), b"second".to_vec()];
    assert!(equal_packets(&a, &a, false).is_ok());
    for bad in [
        vec![],
        vec![b"first".to_vec()],
        vec![b"second".to_vec(), b"first".to_vec()],
        vec![b"first".to_vec(), b"changed".to_vec()],
    ] {
        assert!(equal_packets(&a, &bad, false).is_err());
    }
    assert!(equal_packets(&[], &[], false).is_err());
    let duplicate_threads = vec![b"same".to_vec(); 64];
    assert!(equal_packets(&duplicate_threads, &duplicate_threads, true).is_err());
}

#[test]
fn native_empty_response_preserves_filtered_one_instead_of_claiming_cpp_identity() {
    let command = json!({"op":"emit","level":40,"text":""});
    let original = Imported {
        packets: Vec::new(),
        stdout: Vec::new(),
        filename: "site.cc".into(),
        responses: vec![Response {
            command: command.clone(),
            response: json!({"sent":0,"dropped":0,"linger":100,"errors":0}),
        }],
    };
    let mut fresh = vec![json!({"command":command,"response":{"sent":0,"dropped":0,"filtered":1}})];
    assert!(compare_responses(&original, &fresh, false).is_ok());
    fresh[0]["response"]["filtered"] = json!(0);
    assert!(compare_responses(&original, &fresh, false).is_err());
}

#[test]
fn unlisted_symlink_or_executable_in_context_never_becomes_an_ignored_role() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("context.json"), b"{}").unwrap();
    let names = BTreeSet::from(["context.json".to_owned()]);
    assert!(layout(temp.path(), &names).is_ok());
    fs::write(temp.path().join("old-elf"), b"inert bytes").unwrap();
    assert!(layout(temp.path(), &names).is_err());
    fs::remove_file(temp.path().join("old-elf")).unwrap();
    symlink("context.json", temp.path().join("alias")).unwrap();
    assert!(layout(temp.path(), &names).is_err());
}

#[test]
fn console_bridge_replaces_only_validated_callsite_prefix_and_preserves_all_else() {
    let a = b"/old/root/site.cc: plain\n/old/root/site.cc: prefix\n";
    let b = b"crates/probe.rs: plain\ncrates/probe.rs: prefix\n";
    let row = bridge(a, "/old/root/site.cc", b, "crates/probe.rs").unwrap();
    assert_eq!(row["projected_equal"], true);
    assert_eq!(row["raw_equal"], false);
    assert_eq!(row["left_replacements"], 2);
    for bad in [
        &b"crates/probe.rs: changed\n"[..],
        &b"/other/path: plain\n/other/path: prefix\n"[..],
        &b"crates/probe.rs: plain\r\ncrates/probe.rs: prefix\n"[..],
        &b"\xff"[..],
    ] {
        assert!(bridge(a, "/old/root/site.cc", bad, "crates/probe.rs").is_err());
    }
}

#[test]
fn imported_wire_validation_rejects_raw_record_level_lifetime_and_source_bypass() {
    let data = b"\x14{\"created\": 2.0, \"ctx\": {}, \"filename\": \"/root/rust/tools/native_logging_reference.cc\", \"funcname\": \"emit\", \"levelnum\": 20, \"lineno\": 1, \"msg\": \"plain\"}";
    let record: Value = serde_json::from_slice(&data[1..]).unwrap();
    assert!(wire_record(
        data,
        &record,
        1.0,
        3.0,
        "/root/rust/tools/native_logging_reference.cc",
        "cloudlog(20, text)\n",
        true,
        "",
        ""
    )
    .is_ok());
    assert!(wire_record(
        data,
        &record,
        2.5,
        3.0,
        "/root/rust/tools/native_logging_reference.cc",
        "cloudlog(20, text)\n",
        true,
        "",
        ""
    )
    .is_err());
    assert!(wire_record(
        data,
        &record,
        1.0,
        3.0,
        "/different/site.cc",
        "cloudlog(20, text)\n",
        true,
        "",
        ""
    )
    .is_err());
    let mut forged = record.clone();
    forged["msg"] = json!("changed");
    assert!(wire_record(
        data,
        &forged,
        1.0,
        3.0,
        "/root/rust/tools/native_logging_reference.cc",
        "cloudlog(20, text)\n",
        true,
        "",
        ""
    )
    .is_err());
    let mut bad_level = data.to_vec();
    bad_level[0] = 30;
    assert!(wire_record(
        &bad_level,
        &record,
        1.0,
        3.0,
        "/root/rust/tools/native_logging_reference.cc",
        "cloudlog(20, text)\n",
        true,
        "",
        ""
    )
    .is_err());
}
