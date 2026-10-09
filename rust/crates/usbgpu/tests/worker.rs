use openpilot_usbgpu::{
    worker::{self, Info, Metadata, Runtime},
    Error,
};
use serde_json::json;
use std::os::unix::fs::FileExt;
struct Owned {
    runs: usize,
    nonfinite: bool,
}
impl Runtime for Owned {
    fn run(&mut self, _: &[u8], _: &Info, output: &mut [u8]) -> Result<(), Error> {
        self.runs += 1;
        for value in output.chunks_exact_mut(4) {
            value.copy_from_slice(
                &(if self.nonfinite {
                    f32::NAN
                } else {
                    self.runs as f32
                })
                .to_le_bytes(),
            );
        }
        Ok(())
    }
}
fn info() -> Info {
    let metadata: Metadata = serde_json::from_value(json!({"model_sha256":"0".repeat(64),"checkpoint":"owned",
        "inputs":[{"name":"desire","shape":[8]},{"name":"traffic_convention","shape":[1,2]},
            {"name":"action_t","shape":[1,2]}],"output_count":4,"output_slices":{"outputs":[0,4,null]}})).unwrap();
    Info::new(metadata, [1344, 760]).unwrap()
}
#[test]
fn shared_file_protocol_commits_complete_finite_output_before_success() {
    let info = info();
    let file = tempfile::tempfile().unwrap();
    let mut runtime = Owned {
        runs: 0,
        nonfinite: false,
    };
    let mut control = Vec::new();
    worker::serve(&mut runtime, &file, &info, &mut &b"rrq"[..], &mut control).unwrap();
    let text = String::from_utf8(control).unwrap();
    assert_eq!(text.lines().skip(1).collect::<Vec<_>>(), ["1", "1"]);
    let mut output = [0; 16];
    file.read_exact_at(&mut output, info.input_bytes as u64)
        .unwrap();
    assert!(output
        .chunks_exact(4)
        .all(|v| f32::from_le_bytes(v.try_into().unwrap()) == 2.));
    assert_eq!(runtime.runs, 2);
}
#[test]
fn nonfinite_output_stops_before_publication_and_next_command() {
    let info = info();
    let file = tempfile::tempfile().unwrap();
    file.set_len(info.size as u64).unwrap();
    file.write_all_at(&[0x5a; 16], info.input_bytes as u64)
        .unwrap();
    let mut runtime = Owned {
        runs: 0,
        nonfinite: true,
    };
    let mut control = Vec::new();
    let error =
        worker::serve(&mut runtime, &file, &info, &mut &b"rr"[..], &mut control).unwrap_err();
    worker::report_error(&error, &mut control).unwrap();
    assert_eq!(runtime.runs, 1);
    let text = String::from_utf8(control).unwrap();
    assert_eq!(
        text.lines().skip(1).collect::<Vec<_>>(),
        ["ERROR \"invalid precompiled model output\""]
    );
    let mut output = [0; 16];
    file.read_exact_at(&mut output, info.input_bytes as u64)
        .unwrap();
    assert_eq!(output, [0x5a; 16]);
}
#[test]
fn invalid_command_never_dispatches() {
    let mut runtime = Owned {
        runs: 0,
        nonfinite: false,
    };
    assert!(worker::serve(
        &mut runtime,
        &tempfile::tempfile().unwrap(),
        &info(),
        &mut &b"x"[..],
        &mut Vec::new()
    )
    .is_err());
    assert_eq!(runtime.runs, 0);
}
