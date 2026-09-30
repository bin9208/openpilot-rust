use openpilot_driving_modeld::parameters;
use std::{env, fs, path::Path, process::Command};

#[test]
fn constructor_probe() {
    if env::var_os("RUNTIME_PARAMS_CHILD").is_some() {
        parameters::open()
            .unwrap()
            .put("CameraYawTrimDeg", b"12.5")
            .unwrap();
    }
}

#[test]
fn runtime_constructor_preserves_original_namespace_and_root_selection() {
    assert!(
        !Path::new("/TICI").exists(),
        "default-root cases require a PC host"
    );
    for prefix in [None, Some("runtime-test"), Some("")] {
        for explicit_root in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let home = temporary.path().join("home");
            let root = if explicit_root {
                temporary.path().join("override")
            } else {
                home.join(format!(".comma{}", prefix.unwrap_or("")))
                    .join("params")
            };
            let mut command = Command::new(env::current_exe().unwrap());
            command
                .args(["--exact", "constructor_probe", "--nocapture"])
                .env("RUNTIME_PARAMS_CHILD", "1")
                .env("HOME", &home)
                .env_remove("OPENPILOT_PREFIX")
                .env_remove("PARAMS_ROOT");
            if let Some(value) = prefix {
                command.env("OPENPILOT_PREFIX", value);
            }
            if explicit_root {
                command.env("PARAMS_ROOT", &root);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "prefix={prefix:?}, override={explicit_root}: {output:?}"
            );
            assert_eq!(
                fs::read(root.join(prefix.unwrap_or("d")).join("CameraYawTrimDeg")).unwrap(),
                b"12.5"
            );
        }
    }
}
