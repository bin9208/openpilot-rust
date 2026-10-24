//! Sealed original comparison remains independent of the current native loop.
#![cfg(feature = "paramsd-loop-native")]

use openpilot_dev_tools::paramsd_loop_reference::compare_file;
use std::path::PathBuf;

#[test]
fn actual_loop_output_matches_all_original_cases() {
    let path = PathBuf::from(
        std::env::var_os("PARAMSD_LOOP_NATIVE_RAW")
            .expect("PARAMSD_LOOP_NATIVE_RAW must name the current native-raw.json"),
    );
    assert!(path.is_absolute(), "native loop path must be absolute");
    let summary = compare_file(&path).expect("full sealed paramsd loop comparison");
    assert_eq!(summary.cases, 39);
    assert_eq!(summary.frames, 2420);
    assert_eq!(summary.saves, 4);
    assert_eq!(summary.gps_writes, 216);
}

#[test]
fn actual_checker_has_only_the_two_bound_native_execs() {
    use serde_json::Value;
    use std::fs;
    let required = |name: &str| PathBuf::from(std::env::var_os(name).expect(name));
    let checker = required("PARAMSD_LOOP_NATIVE_CHECKER");
    let candidate = required("PARAMSD_LOOP_NATIVE_CANDIDATE");
    let raw = required("PARAMSD_LOOP_NATIVE_RAW");
    let trace = fs::read_to_string(required("PARAMSD_LOOP_NATIVE_EXEC")).unwrap();
    assert_eq!(trace.matches("execve(").count(), 2, "extra exec attempt");
    assert!(!trace.contains("execveat("), "opaque exec attempt");
    let expression =
        regex::Regex::new(r#"execve\(("(?:[^"\\]|\\.)*"), (\[.*?\]), .*\) = 0"#).unwrap();
    let actual = expression
        .captures_iter(&trace)
        .map(|capture| {
            (
                serde_json::from_str::<String>(&capture[1]).unwrap(),
                serde_json::from_str::<Vec<String>>(&capture[2]).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let output = raw.parent().unwrap();
    assert_eq!(
        actual,
        vec![
            (
                checker.to_str().unwrap().to_owned(),
                vec![
                    checker.to_str().unwrap().to_owned(),
                    "--binary".into(),
                    candidate.to_str().unwrap().to_owned(),
                    "--evidence".into(),
                    output.to_str().unwrap().to_owned(),
                ],
            ),
            (
                candidate.to_str().unwrap().to_owned(),
                vec![
                    candidate.to_str().unwrap().to_owned(),
                    raw.to_str().unwrap().to_owned(),
                ],
            ),
        ]
    );
    let input: Value =
        serde_json::from_slice(&fs::read(output.join("input.json")).unwrap()).unwrap();
    openpilot_dev_tools::paramsd_loop_reference::verify_input(&input).unwrap();
    let result: Value =
        serde_json::from_slice(&fs::read(output.join("result.json")).unwrap()).unwrap();
    assert_eq!(result["result"], "pass");
    assert_eq!(result["cases"], 39);
    assert_eq!(result["frames"], 2420);
    assert_eq!(result["saves"], 4);
    assert_eq!(result["gps_writes"], 216);
}
