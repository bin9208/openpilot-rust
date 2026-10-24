//! Command subprocess contracts captured before porting the original fixture.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

#[test]
fn real_safe_command_recording_exit_and_side_effect_order() {
    let raw = include_bytes!("fixtures/timed-tools/command-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "38052295c0db815dc35d844feea2b026d1af6525b91ff6c16def886777d04c03"
    );
    let data: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 9);
    for case in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().to_str().unwrap();
        let link = temporary.path().join("etc/localtime");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::create_dir_all(temporary.path().join("zoneinfo/Asia")).unwrap();
        fs::write(temporary.path().join("zoneinfo/Asia/Seoul"), b"fixture").unwrap();
        fs::write(&link, b"old").unwrap();
        if case["name"].as_str().unwrap().starts_with("ln") {
            fs::remove_file(&link).unwrap();
        }
        let arguments = case["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().replace("<root>", directory))
            .collect::<Vec<_>>();
        let mut command = Command::new(env!("CARGO_BIN_EXE_timed-command-fixture"));
        command
            .args(arguments)
            .env("TIMED_FIXTURE_ROOT", temporary.path())
            .env_remove("TIMED_COMMAND_FAIL");
        if let Some(failed) = case["command_fail"].as_str() {
            command.env("TIMED_COMMAND_FAIL", failed);
        }
        let result = command.output().unwrap();
        assert_eq!(
            result.status.code(),
            case["exit_code"]
                .as_i64()
                .map(|v| i32::try_from(v).unwrap()),
            "{case}"
        );
        assert!(result.stdout.is_empty(), "{case}");
        assert_eq!(
            String::from_utf8_lossy(&result.stderr).contains("AssertionError"),
            !case["stderr_kind"].is_null(),
            "{case}"
        );
        let commands = fs::read_to_string(temporary.path().join("commands.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(&line.replace(directory, "<root>")).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_value(commands).unwrap(),
            case["commands"],
            "{case}"
        );
        let symlink = fs::read_link(&link)
            .ok()
            .map(|p| p.to_str().unwrap().replace(directory, "<root>"));
        assert_eq!(
            serde_json::to_value(symlink).unwrap(),
            case["localtime_symlink"],
            "{case}"
        );
        let bytes = if link.is_file() && !link.is_symlink() {
            Some(
                fs::read(&link)
                    .unwrap()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
            )
        } else {
            None
        };
        assert_eq!(
            serde_json::to_value(bytes).unwrap(),
            case["localtime_bytes_hex"],
            "{case}"
        );
    }
}

#[test]
fn command_fixture_rejects_escape_before_mutating_outside_root() {
    // Explicit original command fixture admission, not a production sudo shim.
    let temporary = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("keep");
    fs::write(&target, b"keep").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_timed-command-fixture"))
        .args([Path::new("rm"), Path::new("-f"), &target])
        .env("TIMED_FIXTURE_ROOT", temporary.path())
        .env_remove("TIMED_COMMAND_FAIL")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(fs::read(target).unwrap(), b"keep");
    assert!(temporary.path().join("commands.jsonl").exists());
}

#[test]
fn original_root_component_and_argument_arity_boundaries() {
    let bytes = include_bytes!("fixtures/timed-tools/command-extra-original.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "0b55b2d611f63b582cab483a72decdd7c80b00c9ab919a73afb6006f5d58c399"
    );
    let data: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 10);
    let mut expected = vec![];
    let mut observed = vec![];
    for case in data["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("root");
        let sibling = temporary.path().join("root-escape");
        fs::create_dir_all(directory.join("zoneinfo/Asia")).unwrap();
        fs::create_dir(&sibling).unwrap();
        fs::write(directory.join("zoneinfo/Asia/Seoul"), b"fixture").unwrap();
        fs::write(directory.join("keep"), b"keep").unwrap();
        fs::write(sibling.join("keep"), b"keep").unwrap();
        let root = directory.to_str().unwrap();
        let arguments = case["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().replace("<root>", root))
            .collect::<Vec<_>>();
        let result = Command::new(env!("CARGO_BIN_EXE_timed-command-fixture"))
            .args(arguments)
            .env(
                "TIMED_FIXTURE_ROOT",
                case["root_environment"]
                    .as_str()
                    .unwrap()
                    .replace("<root>", root),
            )
            .env_remove("TIMED_COMMAND_FAIL")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        let error = ["AssertionError", "ValueError", "IndexError"]
            .into_iter()
            .find(|kind| stderr.contains(kind));
        let commands = fs::read_to_string(directory.join("commands.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(&line.replace(root, "<root>")).unwrap())
            .collect::<Vec<_>>();
        let link = |path: &Path| {
            fs::read_link(path)
                .ok()
                .map(|p| p.to_str().unwrap().replace(root, "<root>"))
        };
        let keep = fs::read(sibling.join("keep"))
            .ok()
            .map(|v| v.iter().map(|b| format!("{b:02x}")).collect::<String>());
        observed.push(serde_json::json!({"name":case["name"],"arguments":case["arguments"],"root_environment":case["root_environment"],"exit_code":result.status.code(),"stdout_hex":result.stdout.iter().map(|b|format!("{b:02x}")).collect::<String>(),
            "error_kind":error,"commands":commands,"root_keep":directory.join("keep").exists(),"sibling_keep":keep,"root_link":link(&directory.join("link")),"sibling_link":link(&sibling.join("new"))}));
        expected.push(case.clone());
    }
    assert_eq!(observed, expected);
}
