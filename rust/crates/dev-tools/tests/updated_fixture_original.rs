//! Fresh native Git/overlay fixture against the independently captured Python contract.
#![cfg(target_os = "linux")]
use openpilot_dev_tools::updated_fixture::{normalize, seed, setup};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
fn descriptor(path: &Path) -> Value {
    let metadata = fs::symlink_metadata(path).unwrap();
    let mode = metadata.permissions().mode() & 0o7777;
    if metadata.file_type().is_symlink() {
        json!({"mode":mode,"kind":"symlink","target":fs::read_link(path).unwrap().to_str().unwrap()})
    } else if metadata.is_file() {
        let bytes = fs::read(path).unwrap();
        let hex = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        json!({"mode":mode,"kind":"file","sha256":sha(&bytes),"hex":hex})
    } else if metadata.is_dir() {
        json!({"mode":mode,"kind":"directory"})
    } else {
        panic!("unsupported fixture file: {}", path.display());
    }
}
fn replace_temp(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(root.to_str().unwrap(), "<TEMP>")),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| replace_temp(item, root)).collect())
        }
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, item)| (key.clone(), replace_temp(item, root)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn original() -> Value {
    let path = PathBuf::from(
        env::var_os("UPDATED_FIXTURE_ORIGINAL_RECEIPT").expect("mandatory original receipt"),
    );
    let bytes = fs::read(path).unwrap();
    let expected =
        env::var("UPDATED_FIXTURE_ORIGINAL_SHA256").expect("mandatory original receipt SHA256");
    assert_eq!(sha(&bytes), expected);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["source_sha256"]["rust/tools/updated_fixtures.py"],
        "76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60"
    );
    value
}
fn chosen_env(values: &BTreeMap<std::ffi::OsString, std::ffi::OsString>) -> Value {
    let mut out = serde_json::Map::new();
    for name in [
        "PATH",
        "UPDATED_FIXTURE_ROOT",
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_GLOBAL",
        "TZ",
        "OPENPILOT_PREFIX",
    ] {
        out.insert(
            name.into(),
            values.get(OsStr::new(name)).map_or(Value::Null, |value| {
                Value::String(value.to_str().unwrap().into())
            }),
        );
    }
    Value::Object(out)
}

#[test]
fn seed_and_setup_preserve_captured_original_git_tree_and_files() {
    let receipt = original();
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path();
    let seeded = seed(directory).unwrap();
    assert_eq!(seeded.previous, receipt["previous_commit"]);
    assert_eq!(
        git(&seeded.remote, &["rev-parse", "HEAD"]),
        receipt["remote_head"]
    );
    assert_eq!(
        git(&seeded.remote, &["branch", "--format=%(refname:short)"]),
        receipt["remote_branches"]
    );
    assert_eq!(
        git(
            &seeded.remote,
            &[
                "log",
                "--format=%H%x09%an%x09%ae%x09%ad%x09%s",
                "--date=iso-strict"
            ]
        ),
        receipt["remote_log"]
    );
    assert_eq!(git(&seeded.remote, &["--version"]), receipt["git_version"]);
    for (name, expected) in receipt["remote_entries"].as_object().unwrap() {
        assert_eq!(
            &descriptor(&seeded.remote.join(name)),
            expected,
            "remote: {name}"
        );
    }
    let shim = Path::new(env!("CARGO_BIN_EXE_updated-fixture"));
    for lane in ["source", "native"] {
        let root = directory.join(lane);
        let prepared = setup(&root, &seeded, Path::new("/usr/bin/true"), shim).unwrap();
        assert_eq!(
            replace_temp(&prepared.config, directory),
            receipt["lanes"][lane]["config"]
        );
        assert_eq!(
            replace_temp(&chosen_env(&prepared.environment), directory),
            receipt["lanes"][lane]["environment"]
        );
        assert_eq!(
            git(&root.join("base"), &["rev-parse", "HEAD"]),
            receipt["lanes"][lane]["base_head"]
        );
        assert_eq!(
            git(&root.join("base"), &["branch", "--show-current"]),
            receipt["lanes"][lane]["base_branch"]
        );
        assert_eq!(
            git(&root.join("base"), &["status", "--porcelain=v1"]),
            receipt["lanes"][lane]["base_status"]
        );
        for (name, expected) in receipt["lanes"][lane]["entries"].as_object().unwrap() {
            if name == "bin/sudo" {
                continue;
            } // The emitted Python script becomes an owned ELF alias.
            assert_eq!(&descriptor(&root.join(name)), expected, "{lane}: {name}");
        }
        assert_eq!(fs::read_link(root.join("bin/sudo")).unwrap(), shim);
        let repeated = setup(&root, &seeded, Path::new("/usr/bin/true"), shim)
            .err()
            .expect("existing setup root must fail");
        assert_eq!(
            repeated.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AlreadyExists
        );
    }
    let repeated = seed(directory).err().expect("existing remote must fail");
    assert_eq!(
        repeated.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        receipt["errors"]["seed_existing_remote"]["exception"],
        "FileExistsError"
    );
    assert_eq!(
        receipt["errors"]["setup_existing_root"]["exception"],
        "FileExistsError"
    );
    assert_eq!(
        receipt["errors"]["git_missing_ref"]["exception"],
        "CalledProcessError"
    );
    let sample_root = directory.join("native");
    let sample = json!({"path":sample_root,"params":{"One":format!("{}{}{}",sha_hex(b"pre-"),sha_hex(sample_root.as_os_str().as_bytes()),sha_hex(b"-post"))}});
    assert_eq!(
        normalize(&sample, &sample_root).unwrap(),
        receipt["normalize_output"]
    );
}
fn sha_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn overlay_alias_keeps_owned_mount_chmod_umount_and_cleanup_boundaries() {
    let _receipt = original();
    let temp = tempfile::tempdir().unwrap();
    let seeded = seed(temp.path()).unwrap();
    let root = temp.path().join("overlay");
    let shim = Path::new(env!("CARGO_BIN_EXE_updated-fixture"));
    let prepared = setup(&root, &seeded, Path::new("/usr/bin/true"), shim).unwrap();
    let command = root.join("bin/sudo");
    let workdir = root.join("data/work");
    fs::create_dir(&workdir).unwrap();
    let stage = root.join("data/stage");
    let options = format!(
        "lowerdir={},upperdir={},workdir={}",
        root.join("base").display(),
        root.join("data/upper").display(),
        workdir.display()
    );
    let run = |args: &[&str]| {
        Command::new(&command)
            .args(args)
            .envs(&prepared.environment)
            .output()
            .unwrap()
    };
    assert!(run(&[
        "mount",
        "-t",
        "overlay",
        "-o",
        &options,
        "none",
        stage.to_str().unwrap()
    ])
    .status
    .success());
    assert_eq!(
        fs::read(stage.join("common/version.h")).unwrap(),
        b"#define COMMA_VERSION \"1.0\"\n"
    );
    assert_eq!(
        fs::read_link(stage.join("version-link")).unwrap(),
        Path::new("common/version.h")
    );
    fs::set_permissions(
        stage.join("launch_env.sh"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(run(&[
        "chmod",
        "755",
        stage.join("launch_env.sh").to_str().unwrap()
    ])
    .status
    .success());
    assert_eq!(
        fs::metadata(stage.join("launch_env.sh"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert!(run(&["umount", "-l", stage.to_str().unwrap()])
        .status
        .success());
    assert!(!run(&["rm", "-rf", temp.path().to_str().unwrap()])
        .status
        .success());
    assert!(stage.exists());
    assert!(run(&["rm", "-rf", stage.to_str().unwrap()])
        .status
        .success());
    assert!(!stage.exists());
    assert!(!run(&["mount", "/outside"]).status.success());
    let expected = vec![
        json!(["mount", "-t", "overlay", "-o", options, "none", stage]),
        json!(["chmod", "755", stage.join("launch_env.sh")]),
        json!(["umount", "-l", stage]),
        json!(["rm", "-rf", temp.path()]),
        json!(["rm", "-rf", stage]),
        json!(["mount", "/outside"]),
    ];
    let actual = fs::read_to_string(root.join("overlay-commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
