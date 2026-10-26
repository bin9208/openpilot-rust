//! 퇴역한 Planner 도구6개의 runtime 부재 계약. leaf 대역은 native shell이다.
use openpilot_dev_tools::{
    planner_ci_controller::plan::{CHECK_NAMES, MEMORY_CHECK_NAMES},
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const RETIRED: [&str; 6] = [
    "rust/tools/check_plannerd_ci.py",
    "rust/tools/tests/test_plannerd_ci.py",
    "rust/tools/check_plannerd_lifecycle.py",
    "rust/tools/plannerd_lifecycle_peer.py",
    "rust/tools/plannerd_ipc.py",
    "rust/tools/plannerd_gap_source.py",
];

const LEAF: &str = r#"#!/bin/sh
set -eu
printf 'leaf\n' >> "$PLANNER_RETIREMENT_CALLS"
case "$PLANNER_RETIREMENT_RESTORE_KIND" in
  none) ;;
  file) printf 'restored retired controller\n' > "$PLANNER_RETIREMENT_RESTORE_PATH" ;;
  symlink) ln -s -- "$PLANNER_RETIREMENT_RESTORE_TARGET" "$PLANNER_RETIREMENT_RESTORE_PATH" ;;
  *) exit 93 ;;
esac
printf 'out\n'
printf 'err\n' >&2
exit 42
"#;

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    leaf: PathBuf,
    output: PathBuf,
    calls: PathBuf,
    target: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let leaf = temp.path().join("native-leaf");
        fs::write(&leaf, LEAF).unwrap();
        fs::set_permissions(&leaf, fs::Permissions::from_mode(0o755)).unwrap();
        let target = temp.path().join("inert-target");
        fs::write(&target, b"data only, never executed\n").unwrap();
        Self {
            root,
            leaf,
            output: temp.path().join("output"),
            calls: temp.path().join("calls"),
            target,
            _temp: temp,
        }
    }

    fn marker(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        path
    }

    fn run(&self, memory: bool, restore_kind: &str, marker: &Path, target: &Path) -> Captured {
        let mut argv = vec![env!("CARGO_BIN_EXE_check-plannerd-ci").into()];
        if memory {
            argv.push("memory".into());
        }
        for (name, path) in [
            ("--root", self.root.clone()),
            ("--python", self.leaf.clone()),
            ("--bin-dir", self.root.join("bin")),
            ("--artifact", self.root.join("artifact")),
            ("--source-native", self.root.join("source")),
            ("--binding", self.root.join("params.so")),
            ("--output", self.output.clone()),
        ] {
            argv.extend([name.into(), path.to_str().unwrap().into()]);
        }
        argv.extend(["--timeout-seconds".into(), "5".into()]);
        let names: &[&str] = if memory {
            &MEMORY_CHECK_NAMES
        } else {
            &CHECK_NAMES
        };
        for name in names {
            argv.extend(["--growth-mib".into(), format!("{name}=0")]);
        }
        let mut command = CommandSpec::new(argv, Duration::from_secs(10));
        for (name, value) in [
            ("PLANNER_RETIREMENT_CALLS", self.calls.to_str().unwrap()),
            ("PLANNER_RETIREMENT_RESTORE_KIND", restore_kind),
            ("PLANNER_RETIREMENT_RESTORE_PATH", marker.to_str().unwrap()),
            (
                "PLANNER_RETIREMENT_RESTORE_TARGET",
                target.to_str().unwrap(),
            ),
        ] {
            command.env.insert(name.into(), value.into());
        }
        NativeRunner.run(&command).unwrap()
    }

    fn first42_was_recorded(&self) {
        assert_eq!(fs::read(&self.calls).unwrap(), b"leaf\n");
        let rows: Value =
            serde_json::from_slice(&fs::read(self.output.join("commands.json")).unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["name"], "abi");
        assert_eq!(rows[0]["returncode"], 42);
        assert_eq!(
            fs::read(self.output.join("abi.log")).unwrap(),
            b"out\nerr\n"
        );
        for name in ["policy.log", "owner.log", "fault.log", "result.json"] {
            assert!(!self.output.join(name).exists());
        }
    }

    fn rejected(&self, result: &Captured, relative: &str) {
        assert_eq!(result.code, 1, "{result:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(&format!(
                "retired Planner controller path exists: {relative}"
            )),
            "{result:?}"
        );
    }
}

#[test]
fn empty_runtime_root_reaches_first_native_leaf_and_preserves_status42() {
    let fixture = Fixture::new();
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
    let result = fixture.run(false, "none", &fixture.root.join("unused"), &fixture.target);
    assert_eq!(result.code, 42, "{result:?}");
    fixture.first42_was_recorded();
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 0);
}

#[test]
fn runtime_rejects_restored_files_and_directories_before_any_leaf_or_output() {
    for relative in RETIRED {
        for directory in [false, true] {
            let fixture = Fixture::new();
            let marker = fixture.marker(relative);
            if directory {
                fs::create_dir(&marker).unwrap();
            } else {
                fs::write(&marker, b"retired source restored\n").unwrap();
            }
            let result = fixture.run(false, "none", &marker, &fixture.target);
            fixture.rejected(&result, relative);
            assert!(!fixture.calls.exists());
            assert!(!fixture.output.exists());
        }
    }
}

#[test]
fn runtime_rejects_live_and_dangling_symlinks_before_any_leaf_or_output() {
    for relative in RETIRED {
        for dangling in [false, true] {
            let fixture = Fixture::new();
            let marker = fixture.marker(relative);
            let target = if dangling {
                fixture.root.join("missing-target")
            } else {
                fixture.target.clone()
            };
            symlink(&target, &marker).unwrap();
            let result = fixture.run(false, "none", &marker, &target);
            fixture.rejected(&result, relative);
            assert!(!fixture.calls.exists());
            assert!(!fixture.output.exists());
        }
    }
}

#[test]
fn runtime_rejects_file_restoration_even_after_the_first_leaf_failed() {
    for relative in RETIRED {
        let fixture = Fixture::new();
        let marker = fixture.marker(relative);
        let result = fixture.run(false, "file", &marker, &fixture.target);
        fixture.rejected(&result, relative);
        fixture.first42_was_recorded();
        assert_eq!(fs::read(&marker).unwrap(), b"restored retired controller\n");
    }
}

#[test]
fn runtime_rejects_symlink_restoration_even_after_the_first_leaf_failed() {
    for relative in RETIRED {
        for dangling in [false, true] {
            let fixture = Fixture::new();
            let marker = fixture.marker(relative);
            let target = if dangling {
                fixture.root.join("missing-target")
            } else {
                fixture.target.clone()
            };
            let result = fixture.run(false, "symlink", &marker, &target);
            fixture.rejected(&result, relative);
            fixture.first42_was_recorded();
            assert!(fs::symlink_metadata(&marker)
                .unwrap()
                .file_type()
                .is_symlink());
            assert_eq!(fs::read_link(&marker).unwrap(), target);
        }
    }
}

#[test]
fn memory_mode_keeps_controller_path_presence_outside_the_runtime_guard() {
    for relative in RETIRED {
        for dangling in [false, true] {
            let fixture = Fixture::new();
            let marker = fixture.marker(relative);
            if dangling {
                symlink(fixture.root.join("missing-target"), &marker).unwrap();
            } else {
                fs::write(&marker, b"independent controller remains\n").unwrap();
            }
            let result = fixture.run(true, "none", &marker, &fixture.target);
            assert_eq!(result.code, 42, "{result:?}");
            fixture.first42_was_recorded();
            assert!(fs::symlink_metadata(&marker).is_ok());
        }
    }
}
