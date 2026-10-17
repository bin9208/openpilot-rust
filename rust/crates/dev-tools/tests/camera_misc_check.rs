#[path = "support/camera_pair.rs"]
mod support;
use openpilot_dev_tools::{
    camera_misc_check::{check, generate_inputs},
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
};
use std::fs;
#[test]
fn all_original_requests_and_183_real_pixel_fixtures_are_generated_from_seed() {
    let live = support::fixture("misc", "live-original.json");
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("out");
    let input = generate_inputs(&output).unwrap();
    for (text, expected) in [
        (
            &String::from_utf8(input.source.clone()).unwrap(),
            &live["calls"][0],
        ),
        (&input.native, &live["calls"][1]),
    ] {
        assert_eq!(
            support::sha(text.replace(output.to_str().unwrap(), "$OUTPUT").as_bytes()),
            expected["canonical_sha256"]
        );
        assert_eq!(text.lines().count(), 7292);
    }
    let expected = live["fixtures"].as_object().unwrap();
    assert_eq!(
        fs::read_dir(output.join("fixtures")).unwrap().count(),
        expected.len()
    );
    for (name, record) in expected {
        let data = fs::read(output.join("fixtures").join(name)).unwrap();
        assert_eq!(support::sha(&data), record["sha256"]);
        assert_eq!(data.len(), record["bytes"].as_u64().unwrap() as usize);
    }
}
#[test]
fn original_process_artifacts_fixture_writes_and_error_order() {
    support::process_matrix("misc", check);
}
#[test]
fn comparison_does_not_decode_later_rows_before_an_earlier_mismatch() {
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
            let mut lines = vec!["null"; n];
            lines[0] = if self.calls == 1 { "1.0" } else { "2.0" };
            lines[1] = "invalid";
            Ok(Captured {
                code: 0,
                stdout: (lines.join("\n") + "\n").into_bytes(),
                stderr: vec![],
            })
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("reference");
    let native = tmp.path().join("native");
    let output = tmp.path().join("out");
    fs::write(&source, "ref").unwrap();
    fs::write(&native, "native").unwrap();
    let mut peer = Peer { calls: 0 };
    assert!(check(&source, &native, &output, "", "", &mut peer, &mut vec![]).is_err());
    assert_eq!(peer.calls, 2);
    assert!(output.join("failure.json").exists());
}
#[test]
fn native_cli_argument_contract() {
    support::cli_arguments(env!("CARGO_BIN_EXE_check-camerad-misc"));
}
#[test]
fn original_recursive_comparison_and_failure_serializer_boundaries() {
    support::depth_matrix("misc", check);
}
#[test]
fn raw_output_paths_preserve_original_encoding_and_splitline_failure_phases() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    };
    for case in support::fixture("misc", "paths-original.json")["cases"]
        .as_array()
        .unwrap()
    {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let suffix = case["suffix_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let output = root.join(OsString::from_vec([b"out-".as_slice(), &suffix].concat()));
        let reference = root.join("reference");
        let native = root.join("native");
        let sourcemark = root.join("source-called");
        let nativemark = root.join("native-called");
        for (binary, mark) in [(&reference, &sourcemark), (&native, &nativemark)] {
            fs::write(
                binary,
                format!(
                    "#!/bin/sh\n: > {}\nwhile IFS= read -r line; do printf 'null\\n'; done\n",
                    mark.display()
                ),
            )
            .unwrap();
            fs::set_permissions(binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_check-camerad-misc"))
            .arg("--reference")
            .arg(reference)
            .arg("--binary")
            .arg(native)
            .arg("--output")
            .arg(&output)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(case["exit"].as_i64().unwrap() as i32),
            "{case}: {:?}",
            result.stderr
        );
        assert_eq!(sourcemark.exists(), case["source_called"]);
        assert_eq!(nativemark.exists(), case["native_called"]);
        assert_eq!(
            fs::read_dir(output.join("fixtures")).unwrap().count(),
            case["fixtures"].as_u64().unwrap() as usize
        );
        let mut files = fs::read_dir(&output)
            .unwrap()
            .map(|v| v.unwrap().file_name().into_string().unwrap())
            .filter(|v| v != "fixtures")
            .collect::<Vec<_>>();
        files.sort();
        assert_eq!(serde_json::json!(files), case["files"]);
    }
}
