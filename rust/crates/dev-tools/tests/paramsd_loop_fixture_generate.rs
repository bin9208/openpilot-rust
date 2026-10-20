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
