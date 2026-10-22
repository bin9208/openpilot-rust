#![cfg(target_os = "linux")]

use openpilot_dev_tools::qa_process::{CommandSpec, NativeRunner, Runner};
use serde_json::Value;
use std::{ffi::OsString, fs, path::PathBuf, time::Duration};

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

fn json(path: PathBuf) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
#[ignore = "hosted unchanged-original CLI capture and native ELF comparison"]
fn positional_manager_and_logging_keep_original_selected_stdout_and_summary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let original = env_path("MANAGER_QA_FRESH_ORIGINAL_CAPTURE");
    let output = env_path("MANAGER_QA_COMPAT_OUTPUT");
    let trace = env_path("MANAGER_QA_BINARY");
    let binding = env_path("MANAGER_QA_BINDING");
    let manager = env_path("MANAGER_QA_COMPAT_MANAGER_BINARY");
    let logging = env_path("MANAGER_QA_COMPAT_LOGGING_BINARY");
    assert!(!output.exists());
    fs::create_dir(&output).unwrap();
    let admissions = fs::read_to_string(original.join("cli-status.txt")).unwrap();
    let mut runner = NativeRunner;
    for (tool, binary) in [
        ("check_manager.py", &manager),
        ("check_manager_logging.py", &logging),
    ] {
        for (name, args) in [
            ("help", vec![OsString::from("--help")]),
            ("missing", vec![]),
            ("unknown", vec![OsString::from("--unknown")]),
        ] {
            let mut argv = vec![binary.as_os_str().to_os_string()];
            argv.extend(args);
            let mut command = CommandSpec::new_os(argv, Duration::from_secs(5));
            command.cwd = Some(root.clone());
            let result = runner.run(&command).unwrap();
            let prefix = format!("{tool} {name} ");
            let source = admissions
                .lines()
                .filter(|line| line.starts_with(&prefix))
                .collect::<Vec<_>>();
            assert_eq!(
                source.len(),
                1,
                "original CLI admission row missing or duplicated"
            );
            assert_eq!(source[0], format!("{prefix}{}", result.code));
            fs::write(output.join(format!("{tool}-{name}.stdout")), result.stdout).unwrap();
            fs::write(output.join(format!("{tool}-{name}.stderr")), result.stderr).unwrap();
        }
    }

    let selected = output.join("manager-selected");
    let mut command = CommandSpec::new_os(
        vec![
            manager.into_os_string(),
            trace.clone().into_os_string(),
            binding.clone().into_os_string(),
            selected.clone().into_os_string(),
            OsString::from("--scenario"),
            OsString::from("normal"),
            OsString::from("--scenario"),
            OsString::from("normal"),
            OsString::from("--scenario"),
            OsString::from("custom"),
        ],
        Duration::from_secs(240),
    );
    command.cwd = Some(root.clone());
    let selected_run = runner.run(&command).unwrap();
    fs::write(output.join("manager-selected.stdout"), &selected_run.stdout).unwrap();
    fs::write(output.join("manager-selected.stderr"), &selected_run.stderr).unwrap();
    fs::write(
        output.join("manager-selected.status"),
        selected_run.code.to_string(),
    )
    .unwrap();
    assert_eq!(
        selected_run.code,
        0,
        "{}",
        String::from_utf8_lossy(&selected_run.stderr)
    );
    assert_eq!(
        selected_run.stdout,
        fs::read(original.join("manager-selected.stdout")).unwrap()
    );
    assert_eq!(
        json(selected.join("summary.json")),
        json(original.join("manager-selected/summary.json"))
    );
    assert_eq!(
        json(selected.join("normal-native/result.json")),
        json(original.join("manager-selected/normal-source/result.json"))
    );
    assert_eq!(
        json(selected.join("custom-native/result.json")),
        json(original.join("manager-selected/custom-source/result.json"))
    );
    assert_eq!(
        json(selected.join("historical-source.json"))["fresh_original_execution"].as_bool(),
        Some(false)
    );

    let log_output = output.join("manager-logging");
    let mut command = CommandSpec::new_os(
        vec![
            logging.into_os_string(),
            trace.into_os_string(),
            binding.into_os_string(),
            log_output.clone().into_os_string(),
        ],
        Duration::from_secs(90),
    );
    command.cwd = Some(root);
    let logging_run = runner.run(&command).unwrap();
    fs::write(output.join("manager-logging.stdout"), &logging_run.stdout).unwrap();
    fs::write(output.join("manager-logging.stderr"), &logging_run.stderr).unwrap();
    fs::write(
        output.join("manager-logging.status"),
        logging_run.code.to_string(),
    )
    .unwrap();
    assert_eq!(
        logging_run.code,
        0,
        "{}",
        String::from_utf8_lossy(&logging_run.stderr)
    );
    assert_eq!(
        logging_run.stdout,
        fs::read(original.join("manager-logging.stdout")).unwrap()
    );
    let expected = json(original.join("manager-logging/summary.json"));
    let actual = json(log_output.join("summary.json"));
    assert_eq!(actual["source_order"], expected["source_order"]);
    assert_eq!(
        actual["synthetic_environment_only"],
        expected["synthetic_environment_only"]
    );
    assert_eq!(actual["records"].as_array().unwrap().len(), 4);
    assert_eq!(
        json(log_output.join("native/result.json")),
        json(original.join("manager-logging/source/result.json"))
    );
    assert_eq!(
        json(log_output.join("historical-source.json"))["fresh_original_execution"].as_bool(),
        Some(false)
    );
}
