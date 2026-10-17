//! Expected values and child requests come from the unchanged original CLI.
use openpilot_dev_tools::{
    camera_sensor_generate::generate,
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::{ffi::OsString, fs, os::unix::ffi::OsStringExt, path::Path, time::Duration};
struct Peer<'a> {
    case: &'a serde_json::Value,
    calls: usize,
    reference: OsString,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        assert_eq!(spec.effective_argv(), vec![self.reference.clone()]);
        assert_eq!(spec.timeout, Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        assert_eq!(spec.stderr_limit, usize::MAX);
        assert!(!spec.merge_output && !spec.inherit_stdin && !spec.inherit_output);
        let requests = self.case["requests"].as_array().unwrap();
        if self.calls >= requests.len() {
            return Err("original reference failed before invocation".into());
        }
        assert_eq!(
            spec.input.as_deref(),
            Some(requests[self.calls].as_str().unwrap().as_bytes())
        );
        let first = self.calls == 0;
        self.calls += 1;
        let output = if first {
            self.case["config_json"]
                .as_str()
                .map(str::as_bytes)
                .unwrap_or(&[255])
                .to_vec()
        } else if let Some(output) = self.case["exposure_json"].as_str() {
            output.as_bytes().to_vec()
        } else {
            "[[13576, 1], [13577, 2]]\n"
                .repeat(requests[self.calls - 1].as_str().unwrap().lines().count())
                .into_bytes()
        };
        let code = self.case[if first {
            "config_code"
        } else {
            "exposure_code"
        }]
        .as_i64()
        .unwrap_or(0) as i32;
        Ok(Captured {
            code,
            stdout: output,
            stderr: vec![],
        })
    }
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_sensor_output_failure_phases_and_child_requests_are_preserved() {
    let record: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-data/sensor-original.json")).unwrap();
    for case in record["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let sensors = temp.path().join("openpilot/system/camerad/sensors");
        fs::create_dir_all(&sensors).unwrap();
        for (name, text) in record["sources"].as_object().unwrap() {
            fs::write(sensors.join(name), text.as_str().unwrap()).unwrap();
        }
        if case["source_mode"] == "missing" {
            fs::remove_dir_all(&sensors).unwrap();
        }
        if case["source_mode"] == "directory" {
            fs::create_dir(sensors.join("dir.cc")).unwrap();
        }
        if let Some(hex) = case["path_hex"].as_str() {
            fs::write(
                sensors.join(OsString::from_vec(decode(hex))),
                "raw filename",
            )
            .unwrap();
        }
        if let Some(hex) = case["selected_path_hex"].as_str() {
            fs::write(
                sensors.join(OsString::from_vec(decode(hex))),
                b"raw selected source",
            )
            .unwrap();
        }
        if let Some(files) = case["extra_sources_hex"].as_array() {
            for file in files {
                fs::write(
                    sensors.join(OsString::from_vec(decode(file[0].as_str().unwrap()))),
                    decode(file[1].as_str().unwrap()),
                )
                .unwrap();
            }
        }
        let reference = OsString::from("source-reference");
        let mut peer = Peer {
            case,
            calls: 0,
            reference: reference.clone(),
        };
        let mut output = Vec::new();
        let result = generate(&mut peer, temp.path(), Path::new(&reference), &mut output);
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{}: {result:?}",
            case["name"]
        );
        let expected = case["stdout_hex"]
            .as_str()
            .map(decode)
            .unwrap_or_else(|| case["stdout"].as_str().unwrap().as_bytes().to_vec());
        assert_eq!(output, expected, "{} output phase", case["name"]);
        assert_eq!(
            peer.calls,
            case["requests"].as_array().unwrap().len(),
            "{} child calls",
            case["name"]
        );
    }
}
#[test]
fn native_cli_has_original_required_arguments_and_needs_no_python() {
    let binary = env!("CARGO_BIN_EXE_generate-camerad-sensor-data");
    for args in [
        vec![],
        vec!["--source", "."],
        vec!["--reference", "missing"],
        vec!["--help"],
    ] {
        let output = std::process::Command::new(binary)
            .args(&args)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(if args == ["--help"] { 0 } else { 2 })
        );
    }
}

#[cfg(unix)]
#[test]
fn actual_native_cli_streams_reference_input_and_preserves_partial_child_failure() {
    use std::os::unix::fs::PermissionsExt;
    let binary = env!("CARGO_BIN_EXE_generate-camerad-sensor-data");
    let record: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/camera-data/sensor-original.json")).unwrap();
    for failure in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let folder = temporary.path().join("openpilot/system/camerad/sensors");
        fs::create_dir_all(&folder).unwrap();
        for (name, text) in record["sources"].as_object().unwrap() {
            fs::write(folder.join(name), text.as_str().unwrap()).unwrap();
        }
        let peer = temporary.path().join("source-reference");
        let code = if failure {
            "exit 7"
        } else {
            "printf '[[13576,1],[13577,2]]\\n'"
        };
        let shell = format!(
            r#"#!/bin/sh
IFS= read -r first
if [ "$first" = 'config 1' ]; then
 IFS= read -r second; IFS= read -r third
 [ "$second" = 'config 2' ] && [ "$third" = 'config 3' ] || exit 9
 printf '%s\n' '{{"analog_gain_max_idx":0}}' '{{"analog_gain_max_idx":0}}' '{{"analog_gain_max_idx":0}}'
else
 [ "$first" = 'exposure 2 5 0 0' ] || [ "$first" = 'exposure 3 5 0 0' ] || exit 9
 {code}
fi
"#
        );
        fs::write(&peer, shell).unwrap();
        fs::set_permissions(&peer, fs::Permissions::from_mode(0o755)).unwrap();
        let result = std::process::Command::new(binary)
            .arg("--source")
            .arg(temporary.path())
            .arg("--reference")
            .arg(&peer)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(result.status.success(), !failure, "{:?}", result.stderr);
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(output.contains("pub static OX03C10"));
        assert_eq!(
            output.contains("pub const OS04C10_GAIN_REGISTERS"),
            !failure
        );
        if failure {
            assert!(output.ends_with("};\n\n"));
            assert!(!output.contains("OX03C10_GAIN_REGISTERS"));
        }
    }
}

#[cfg(unix)]
#[test]
fn actual_reference_paths_match_original_path_normalization_and_path_lookup() {
    use std::os::unix::fs::PermissionsExt;
    let original: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/camera-data/sensor-paths-original.json"
    ))
    .unwrap();
    for case in original["cases"].as_array().unwrap() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path();
        let sensors = directory.join("openpilot/system/camerad/sensors");
        fs::create_dir_all(&sensors).unwrap();
        for (name, value) in original["sources"].as_object().unwrap() {
            fs::write(sensors.join(name), value.as_str().unwrap()).unwrap();
        }
        fs::create_dir(directory.join("sub")).unwrap();
        fs::create_dir(directory.join("~")).unwrap();
        for name in [
            OsString::from("reference"),
            OsString::from("sub/reference"),
            OsString::from("~/reference"),
            OsString::from_vec(b"raw-\xff".to_vec()),
        ] {
            let path = directory.join(name);
            fs::write(&path, original["reference_script"].as_str().unwrap()).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let bytes = decode(case["reference_hex"].as_str().unwrap());
        let placeholder = b"$ROOT";
        let reference = if let Some(at) = bytes
            .windows(placeholder.len())
            .position(|v| v == placeholder)
        {
            let mut value = bytes[..at].to_vec();
            value.extend(directory.as_os_str().as_encoded_bytes());
            value.extend(&bytes[at + placeholder.len()..]);
            value
        } else {
            bytes
        };
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_generate-camerad-sensor-data"))
            .arg("--source")
            .arg(directory)
            .arg("--reference")
            .arg(OsString::from_vec(reference))
            .current_dir(directory)
            .env(
                "PATH",
                if case["path_mode"] == "cwd" {
                    directory.as_os_str()
                } else {
                    std::ffi::OsStr::new("/nonexistent")
                },
            )
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{}: {:?}",
            case["name"],
            result.stderr
        );
        assert_eq!(
            result.stdout,
            decode(case["stdout_hex"].as_str().unwrap()),
            "{} output phase",
            case["name"]
        );
    }
}
