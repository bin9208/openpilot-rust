#[path = "support/camera_pair.rs"]
mod support;
use openpilot_dev_tools::{
    camera_exposure_check::{check, frames, generate_inputs},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::{fs, path::Path};
#[test]
fn original_seeded_all_frames_source_and_native_inputs_are_exact() {
    let live = support::fixture("exposure", "live-original.json");
    let full = generate_inputs(1200).unwrap();
    assert_eq!(
        support::sha(full.source.as_bytes()),
        live["calls"][0]["input_sha256"]
    );
    assert_eq!(
        support::sha(full.native.as_bytes()),
        live["calls"][1]["input_sha256"]
    );
    assert_eq!(full.scenarios, 99);
    assert_eq!(full.steps, 54108);
    assert_eq!(full.source.lines().count(), 54207);
    for case in support::fixture("exposure", "frames-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let result = frames(std::ffi::OsStr::new(case["frames"].as_str().unwrap()));
        assert_eq!(
            result.is_err(),
            !case["error"].is_null(),
            "{}",
            case["frames"]
        );
        if let Ok(frames) = result {
            let input = generate_inputs(frames).unwrap();
            assert_eq!(
                support::sha(input.source.as_bytes()),
                case["calls"][0]["input_sha256"]
            );
            assert_eq!(
                support::sha(input.native.as_bytes()),
                case["calls"][1]["input_sha256"]
            );
        }
    }
}
fn one(
    a: &Path,
    b: &Path,
    c: &Path,
    d: &str,
    e: &str,
    f: &mut dyn Runner,
    g: &mut dyn std::io::Write,
) -> Result<()> {
    check(a, b, c, d, e, 1, f, g)
}
#[test]
fn original_process_artifacts_and_error_order() {
    support::process_matrix("exposure", one);
}
#[test]
fn json_lines_are_decoded_lazily_after_both_children_and_cardinality() {
    struct Peer {
        calls: usize,
    }
    impl Runner for Peer {
        fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
            self.calls += 1;
            let n = String::from_utf8(spec.input.clone().unwrap())
                .unwrap()
                .lines()
                .count();
            let mut rows = vec!["null"; n];
            rows[0] = if self.calls == 1 { "1.0" } else { "2.0" };
            rows[1] = "invalid";
            Ok(Captured {
                code: 0,
                stdout: (rows.join("\n") + "\n").into_bytes(),
                stderr: vec![],
            })
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let refpath = temp.path().join("reference");
    let native = temp.path().join("native");
    let out = temp.path().join("out");
    fs::write(&refpath, "ref").unwrap();
    fs::write(&native, "native").unwrap();
    let mut peer = Peer { calls: 0 };
    assert!(check(&refpath, &native, &out, "", "", 1, &mut peer, &mut vec![]).is_err());
    assert_eq!(peer.calls, 2);
    assert!(out.join("failure.json").exists());
    assert!(!out.join("report.json").exists());
}
#[test]
fn native_cli_argument_contract() {
    let bin = env!("CARGO_BIN_EXE_check-camerad-exposure");
    support::cli_arguments(bin);
    for value in ["0", "1201", "1.0", "1\x1c"] {
        let r = std::process::Command::new(bin)
            .args([
                "--reference",
                "no",
                "--binary",
                "no",
                "--output",
                "no",
                "--frames",
                value,
            ])
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
    }
}
#[test]
fn original_recursive_comparison_and_failure_serializer_boundaries() {
    support::depth_matrix("exposure", one);
}
