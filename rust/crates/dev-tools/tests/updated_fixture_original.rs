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

fn preserve(name: &str, observation: &Value) {
    let root = PathBuf::from(
        env::var_os("UPDATED_FIXTURE_NATIVE_OUTPUT").expect("mandatory native observation output"),
    );
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec_pretty(observation).unwrap(),
    )
    .unwrap();
}

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
    let bytes = include_bytes!("fixtures/updated-fixture-original/git-original.json");
    assert_eq!(
        sha(bytes),
        "3a859802faa1b1c080def7260c31baf0cc7fe8e94ba7efdd182a03d3090fdc1a"
    );
    let value: Value = serde_json::from_slice(bytes).unwrap();
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
                assert_eq!(
                    expected,
                    &json!({"byte_count":951,"kind":"file","mode":0o755,"sha256":"3110cc0a1f9b57cdba2133d7d515121e6e47dae3fbe228f77527df861efb041d"}),
                    "original generated helper metadata: {lane}"
                );
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

fn overlay_original() -> Value {
    let bytes = include_bytes!("fixtures/updated-fixture-original/overlay-original.json");
    assert_eq!(
        sha(bytes),
        "338900085dc7b31c68fc1c743fc19e128679cb63e59a487307f86af6a49d3947"
    );
    let receipt: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(
        receipt["source_commit"],
        "ce145d062ad24f71da3e49e9b8b4fc676b35ef41"
    );
    assert_eq!(
        receipt["source_sha256"]["rust/tools/updated_fixtures.py"],
        "76607e02520f383227264257de0ed8505b9dc12e462227b176f2c919dfec2d60"
    );
    receipt
}
fn overlay_tree(root: &Path) -> Value {
    fn visit(root: &Path, at: &Path, result: &mut serde_json::Map<String, Value>) {
        let mut entries = fs::read_dir(at)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            if name.split('/').any(|part| part == ".git")
                || name == "bin/sudo"
                || name == "overlay-commands.jsonl"
            {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).unwrap();
            let mode = metadata.permissions().mode() & 0o7777;
            let value = if metadata.file_type().is_symlink() {
                json!({"kind":"symlink","mode":mode,"target":fs::read_link(&path).unwrap().to_str().unwrap()})
            } else if metadata.is_file() {
                let bytes = fs::read(&path).unwrap();
                json!({"kind":"file","mode":mode,"raw_hex":sha_hex(&bytes),"sha256":sha(&bytes)})
            } else if metadata.is_dir() {
                json!({"kind":"directory","mode":mode})
            } else {
                panic!("unsupported fixture entry: {}", path.display());
            };
            result.insert(name, value);
            if metadata.is_dir() {
                visit(root, &path, result);
            }
        }
    }
    let mut result = serde_json::Map::new();
    visit(root, root, &mut result);
    Value::Object(result)
}
fn filtered_original_tree(original: &Value) -> Value {
    let mut value = original.as_object().unwrap().clone();
    assert_eq!(
        value.get("bin/sudo"),
        Some(
            &json!({"byte_count":951,"kind":"file","mode":0o755,"sha256":"3110cc0a1f9b57cdba2133d7d515121e6e47dae3fbe228f77527df861efb041d"})
        ),
        "original generated helper metadata"
    );
    value.remove("bin/sudo");
    value.remove("overlay-commands.jsonl");
    Value::Object(value)
}
fn normalized_log(root: &Path) -> String {
    String::from_utf8(fs::read(root.join("overlay-commands.jsonl")).unwrap())
        .unwrap()
        .replace(root.to_str().unwrap(), "<ROOT>")
}
fn overlay_command(
    root: &Path,
    env: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    args: &[&str],
) -> std::process::Output {
    Command::new(root.join("bin/sudo"))
        .args(args)
        .envs(env)
        .output()
        .unwrap()
}

#[test]
fn original_overlay_success_and_repeated_mount_match_raw_receipt() {
    let original = overlay_original();
    let temp = tempfile::tempdir().unwrap();
    let seeded = seed(temp.path()).unwrap();
    let shim = Path::new(env!("CARGO_BIN_EXE_updated-fixture"));
    for lane in ["success", "repeat"] {
        let root = temp.path().join(lane);
        let prepared = setup(&root, &seeded, Path::new("/usr/bin/true"), shim).unwrap();
        let workdir = root.join("data/work");
        fs::create_dir(&workdir).unwrap();
        let stage = root.join("data/stage");
        let options = format!(
            "lowerdir={},upperdir={},workdir={}",
            root.join("base").display(),
            root.join("data/upper").display(),
            workdir.display()
        );
        let mount = [
            "mount",
            "-t",
            "overlay",
            "-o",
            &options,
            "none",
            stage.to_str().unwrap(),
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let steps = if lane == "success" {
            vec![
                mount.clone(),
                vec![
                    "chmod".into(),
                    "755".into(),
                    stage.join("launch_env.sh").to_str().unwrap().to_owned(),
                ],
                vec![
                    "umount".into(),
                    "-l".into(),
                    stage.to_str().unwrap().to_owned(),
                ],
                vec![
                    "rm".into(),
                    "-rf".into(),
                    stage.to_str().unwrap().to_owned(),
                ],
            ]
        } else {
            vec![mount.clone(), mount]
        };
        for (index, args) in steps.iter().enumerate() {
            let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
            let result = overlay_command(&root, &prepared.environment, &borrowed);
            let expected = &original["scenarios"][lane][index];
            preserve(
                &format!("{lane}-{index}"),
                &json!({"exit":result.status.code(),"stdout_hex":sha_hex(&result.stdout),"stderr_hex":sha_hex(&result.stderr),"log":normalized_log(&root),"tree":overlay_tree(&root)}),
            );
            assert_eq!(
                result.status.code(),
                expected["exit"]
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok()),
                "{lane} {index}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                sha_hex(&result.stdout),
                expected["stdout_hex"],
                "{lane} {index} stdout"
            );
            if result.status.success() {
                assert_eq!(
                    sha_hex(&result.stderr),
                    expected["stderr_hex"],
                    "{lane} {index} stderr"
                );
            }
            assert_eq!(
                normalized_log(&root),
                expected["log"]["normalized_root_text"],
                "{lane} {index} raw spaced JSONL"
            );
            assert_eq!(
                overlay_tree(&root),
                filtered_original_tree(&expected["tree"]),
                "{lane} {index} complete file bytes/modes/symlinks"
            );
        }
    }
}

#[test]
fn original_destination_symlink_escapes_are_rejected_without_touching_private_siblings() {
    let original = overlay_original();
    assert_eq!(original["scenarios"]["symlink-outside"][0]["exit"], 0);
    assert_eq!(
        original["scenarios"]["nested-symlink-outside"][0]["exit"],
        0
    );
    assert_ne!(
        original["scenarios"]["nested_outside_before"]["sha256"],
        original["scenarios"]["nested_outside_after"]["sha256"]
    );
    let temp = tempfile::tempdir().unwrap();
    let seeded = seed(temp.path()).unwrap();
    let shim = Path::new(env!("CARGO_BIN_EXE_updated-fixture"));
    for lane in ["nested-symlink-outside", "symlink-outside"] {
        let root = temp.path().join(lane);
        let prepared = setup(&root, &seeded, Path::new("/usr/bin/true"), shim).unwrap();
        let workdir = root.join("data/work");
        fs::create_dir(&workdir).unwrap();
        let stage = root.join("data/stage");
        let sibling = temp.path().join(format!("{lane}-private-sibling"));
        fs::create_dir(&sibling).unwrap();
        let sentinel = if lane == "symlink-outside" {
            let sentinel = sibling.join("sentinel");
            fs::write(&sentinel, b"private sibling, never a host path\n").unwrap();
            std::os::unix::fs::symlink(&sibling, &stage).unwrap();
            sentinel
        } else {
            let sentinel = sibling.join("version.h");
            fs::write(
                &sentinel,
                b"private original contents; outside the fixture root\n",
            )
            .unwrap();
            fs::create_dir_all(stage.join("common")).unwrap();
            std::os::unix::fs::symlink(&sentinel, stage.join("common/version.h")).unwrap();
            sentinel
        };
        let before = fs::read(&sentinel).unwrap();
        let options = format!(
            "lowerdir={},upperdir={},workdir={}",
            root.join("base").display(),
            root.join("data/upper").display(),
            workdir.display()
        );
        let result = overlay_command(
            &root,
            &prepared.environment,
            &[
                "mount",
                "-t",
                "overlay",
                "-o",
                &options,
                "none",
                stage.to_str().unwrap(),
            ],
        );
        preserve(
            lane,
            &json!({"exit":result.status.code(),"stdout_hex":sha_hex(&result.stdout),"stderr_hex":sha_hex(&result.stderr),"before_hex":sha_hex(&before),"after_hex":sha_hex(&fs::read(&sentinel).unwrap()),"log":normalized_log(&root)}),
        );
        assert!(
            !result.status.success(),
            "{lane} followed destination symlink"
        );
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            before,
            "{lane} changed outside sentinel"
        );
        let expected_log = original["scenarios"][lane][0]["log"]["normalized_root_text"]
            .as_str()
            .unwrap()
            .replace(
                original["temp_root"].as_str().unwrap(),
                temp.path().to_str().unwrap(),
            );
        assert_eq!(normalized_log(&root), expected_log);
    }
}

#[test]
fn original_overlay_error_cases_retain_ordered_spaced_logs_and_owned_tree() {
    let original = overlay_original();
    let temp = tempfile::tempdir().unwrap();
    let seeded = seed(temp.path()).unwrap();
    let root = temp.path().join("errors");
    let shim = Path::new(env!("CARGO_BIN_EXE_updated-fixture"));
    let prepared = setup(&root, &seeded, Path::new("/usr/bin/true"), shim).unwrap();
    fs::create_dir(root.join("data/work")).unwrap();
    let outside = temp.path().join("private-outside");
    fs::create_dir(&outside).unwrap();
    let sentinel = outside.join("sentinel");
    fs::write(&sentinel, b"private sibling, never a host path\n").unwrap();
    let before = fs::read(&sentinel).unwrap();
    let stage = root.join("data/stage");
    let cases = [
        vec!["echo".into(), "forbidden".into()],
        vec![
            "rm".into(),
            "-rf".into(),
            outside.to_str().unwrap().to_owned(),
        ],
        vec![
            "mount".into(),
            "-t".into(),
            "overlay".into(),
            "-o".into(),
            format!("lowerdir={}", root.join("base").display()),
            "none".into(),
            stage.to_str().unwrap().to_owned(),
        ],
        vec!["mount".into(), "-t".into(), "overlay".into()],
    ];
    for (index, args) in cases.iter().enumerate() {
        let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
        let result = overlay_command(&root, &prepared.environment, &borrowed);
        let expected = &original["scenarios"]["errors"][index];
        preserve(
            &format!("errors-{index}"),
            &json!({"exit":result.status.code(),"stdout_hex":sha_hex(&result.stdout),"stderr_hex":sha_hex(&result.stderr),"log":normalized_log(&root),"tree":overlay_tree(&root)}),
        );
        assert_eq!(
            result.status.code(),
            expected["exit"]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
        );
        assert_eq!(sha_hex(&result.stdout), expected["stdout_hex"]);
        assert!(!result.stderr.is_empty(), "error case must explain failure");
        let expected_log = expected["log"]["normalized_root_text"]
            .as_str()
            .unwrap()
            .replace(
                original["temp_root"].as_str().unwrap(),
                temp.path().to_str().unwrap(),
            );
        assert_eq!(
            normalized_log(&root),
            expected_log,
            "error {index} raw spaced JSONL"
        );
        assert_eq!(
            overlay_tree(&root),
            filtered_original_tree(&expected["tree"]),
            "error {index} tree"
        );
        assert_eq!(fs::read(&sentinel).unwrap(), before);
    }
}
