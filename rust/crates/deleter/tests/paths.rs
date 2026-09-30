use openpilot_deleter::platform;
use std::{env, fs, path::Path, process::Command};

#[test]
fn path_probe() {
    if env::var_os("DELETER_PATH_CHILD").is_some() {
        let root = platform::log_root().unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("marker"), b"path selected").unwrap();
    }
}

#[test]
fn runtime_paths_follow_source_prefix_and_empty_root_override() {
    assert!(!Path::new("/TICI").exists());
    for prefix in [None, Some("test"), Some("")] {
        for override_root in [None, Some(""), Some("override")] {
            let temporary = tempfile::tempdir().unwrap();
            let home = temporary.path().join("home");
            let expected = if override_root == Some("override") {
                temporary.path().join("override")
            } else {
                home.join(format!(".comma{}", prefix.unwrap_or("")))
                    .join("media/0/realdata")
            };
            let mut command = Command::new(env::current_exe().unwrap());
            command
                .args(["--exact", "path_probe", "--nocapture"])
                .env("DELETER_PATH_CHILD", "1")
                .env("HOME", &home)
                .env_remove("OPENPILOT_PREFIX")
                .env_remove("LOG_ROOT");
            if let Some(prefix) = prefix {
                command.env("OPENPILOT_PREFIX", prefix);
            }
            if let Some(root) = override_root {
                if root.is_empty() {
                    command.env("LOG_ROOT", "");
                } else {
                    command.env("LOG_ROOT", &expected);
                }
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(fs::read(expected.join("marker")).unwrap(), b"path selected");
        }
    }
}
