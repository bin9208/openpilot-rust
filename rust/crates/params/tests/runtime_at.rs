use openpilot_params::Params;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Command};

#[test]
fn runtime_namespace_child() {
    let Some(root) = std::env::var_os("UI_PARAMS_RUNTIME_PROBE") else {
        return;
    };
    let params = Params::for_runtime_at(std::path::Path::new(&root)).unwrap();
    params.put("IsMetric", b"1").unwrap();
}

#[test]
fn runtime_root_preserves_absent_empty_named_and_invalid_prefixes() {
    for (prefix, directory, valid) in [
        (None, "d", true),
        (Some(OsString::new()), "", true),
        (Some("ui-probe_148".into()), "ui-probe_148", true),
        (Some("../escape".into()), "", false),
        (Some(OsString::from_vec(vec![0xff])), "", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "runtime_namespace_child", "--nocapture"])
            .env("UI_PARAMS_RUNTIME_PROBE", root.path())
            .env_remove("OPENPILOT_PREFIX");
        if let Some(prefix) = &prefix {
            command.env("OPENPILOT_PREFIX", prefix);
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.success(),
            valid,
            "prefix={prefix:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if valid {
            assert_eq!(
                std::fs::read(root.path().join(directory).join("IsMetric")).unwrap(),
                b"1"
            );
        } else {
            assert!(!root.path().join("IsMetric").exists());
        }
        println!("prefix={prefix:?}; accepted={valid}; IsMetric namespace={directory:?}");
    }
}
