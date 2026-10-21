#![cfg(feature = "paramsd-fixture-native")]
use openpilot_dev_tools::paramsd_loop_fixture_generate::{cases, generate};
use serde_json::Value;
use std::{fs, path::PathBuf};

fn required(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} required")))
}

#[test]
fn complete_loop_roster_and_validity_transitions() {
    let input = cases().unwrap();
    let cases = input["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 39);
    for (index, case) in cases[..22].iter().enumerate() {
        assert_eq!(case["name"], format!("migration-{index}"));
        assert!(case["frames"].as_array().unwrap().is_empty());
    }
    for (index, case) in cases[22..34].iter().enumerate() {
        assert_eq!(case["name"], format!("cache-{index}"));
        assert!(case["frames"].as_array().unwrap().is_empty());
    }
    assert_eq!(cases[34]["name"], "malformed-v2");
    assert_eq!(cases[35]["name"], "wrong-union");
    assert_eq!(cases[36]["name"], "v2-suppresses-migration");
    for (case, service) in cases[37..]
        .iter()
        .zip(["gpsLocation", "gpsLocationExternal"])
    {
        assert_eq!(case["name"], format!("loop-{service}"));
        assert_eq!(case["gps"], service);
        let frames = case["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 1210);
        assert_eq!(frames[80]["messages"].as_array().unwrap().len(), 0);
        assert_eq!(frames[81]["messages"].as_array().unwrap().len(), 1);
        assert_eq!(frames[90]["messages"].as_array().unwrap().len(), 3);
    }
}

#[test]
fn every_wire_byte_matches_fresh_unchanged_original_input() {
    let root = required("PARAMSD_LOOP_ROOT");
    let original = required("PARAMSD_LOOP_ORIGINAL_INPUT");
    let native_output = required("PARAMSD_LOOP_NATIVE_OUTPUT");
    let native_trace = required("PARAMSD_LOOP_NATIVE_EXEC");
    let native_binary = required("PARAMSD_LOOP_NATIVE_BINARY");
    assert!(native_binary.is_file());
    let trace = fs::read_to_string(native_trace).unwrap();
    let attempts = regex::Regex::new(r"\bexecve(?:at)?\(").unwrap();
    assert_eq!(
        attempts.find_iter(&trace).count(),
        1,
        "unexpected child exec"
    );
    let invocation =
        regex::Regex::new(r#"(?m)\bexecve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0$"#).unwrap();
    let captures = invocation.captures(&trace).expect("one native execve");
    let program: String = serde_json::from_str(&captures[1]).unwrap();
    let argv: Vec<String> = serde_json::from_str(&captures[2]).unwrap();
    assert_eq!(program, native_binary.to_str().unwrap());
    assert_eq!(
        argv,
        [
            program,
            "--root".into(),
            root.to_str().unwrap().into(),
            "--output".into(),
            native_output.to_str().unwrap().into(),
        ]
    );
    let temporary = tempfile::tempdir().unwrap();
    let library_output = temporary.path().join("library");
    generate(&root, &library_output).unwrap();
    let original: Value = serde_json::from_slice(&fs::read(original).unwrap()).unwrap();
    assert_eq!(original["cases"].as_array().unwrap().len(), 39);
    for path in [
        library_output.join("input.json"),
        native_output.join("input.json"),
    ] {
        let generated: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(generated, original, "{}", path.display());
    }
    assert_eq!(
        fs::read(library_output.join("input.json")).unwrap(),
        fs::read(native_output.join("input.json")).unwrap(),
        "library and CLI generation diverged"
    );
}

#[test]
fn retired_loop_fixture_is_optional_only_when_exact_path_is_absent() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for state in ["absent", "changed", "symlink", "directory"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        for name in [
            "rust/tools/paramsd_loop_fixture.py",
            "rust/tools/paramsd_fixture.py",
            "rust/crates/dev-tools/src/paramsd_loop_fixture_generate.rs",
            "rust/crates/dev-tools/src/paramsd_fixture_generate.rs",
        ] {
            let destination = root.join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(repo.join(name), destination).unwrap();
        }
        let retired = root.join("rust/tools/paramsd_loop_fixture.py");
        fs::remove_file(&retired).unwrap();
        match state {
            "absent" => {}
            "changed" => fs::write(&retired, b"changed original fixture").unwrap(),
            "symlink" => std::os::unix::fs::symlink(
                repo.join("rust/tools/paramsd_loop_fixture.py"),
                &retired,
            )
            .unwrap(),
            "directory" => fs::create_dir(&retired).unwrap(),
            _ => unreachable!(),
        }
        let output = temp.path().join("output");
        let result = generate(&root, &output);
        if state == "absent" {
            result.unwrap();
            let observed: Value =
                serde_json::from_slice(&fs::read(output.join("input.json")).unwrap()).unwrap();
            assert_eq!(observed, cases().unwrap());
        } else {
            assert!(result.is_err(), "{state} accepted");
            assert!(!output.exists());
        }
    }
}
