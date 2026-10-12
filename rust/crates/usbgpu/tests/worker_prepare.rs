use openpilot_usbgpu::{
    worker::{self, Info, Input, Metadata, Runtime},
    Error,
};
use std::{collections::BTreeMap, io::Cursor, os::unix::fs::FileExt};

struct Split;
impl Runtime for Split {
    fn run(&mut self, _: &[u8], _: &Info, output: &mut [u8]) -> Result<(), Error> {
        output.copy_from_slice(&42f32.to_le_bytes());
        Ok(())
    }
    fn prepare(&mut self, _: &[u8], _: &Info, _: &mut [u8]) -> Result<bool, Error> {
        Ok(false)
    }
}
struct Fused;
impl Runtime for Fused {
    fn run(&mut self, _: &[u8], _: &Info, output: &mut [u8]) -> Result<(), Error> {
        output.copy_from_slice(&42f32.to_le_bytes());
        Ok(())
    }
}
fn info() -> Info {
    Info::new(
        Metadata {
            model_sha256: "a".repeat(64),
            checkpoint: "owned-protocol".into(),
            inputs: vec![
                Input {
                    name: "desire".into(),
                    shape: vec![8],
                },
                Input {
                    name: "traffic_convention".into(),
                    shape: vec![1, 2],
                },
                Input {
                    name: "action_t".into(),
                    shape: vec![1, 2],
                },
            ],
            output_count: 1,
            output_slices: BTreeMap::from([("hidden_state".into(), [Some(0), Some(1), None])]),
        },
        [1344, 760],
    )
    .unwrap()
}
#[test]
fn split_prepare_preserves_previous_output_without_publishing() {
    // Given an existing output and a split runtime with a prepare-only command.
    let file = tempfile::tempfile().unwrap();
    let info = info();
    file.set_len(u64::try_from(info.size).unwrap()).unwrap();
    file.write_all_at(
        &17f32.to_le_bytes(),
        u64::try_from(info.input_bytes).unwrap(),
    )
    .unwrap();
    let mut reply = Vec::new();
    // When the actual worker protocol serves prepare then quit.
    worker::serve(
        &mut Split,
        &file,
        &info,
        &mut Cursor::new(b"pq"),
        &mut reply,
    )
    .unwrap();
    // Then it acknowledges no model output and preserves the previous bytes.
    assert!(reply.ends_with(b"\n0\n"));
    let mut output = [0; 4];
    file.read_exact_at(&mut output, u64::try_from(info.input_bytes).unwrap())
        .unwrap();
    assert_eq!(output, 17f32.to_le_bytes());
}
#[test]
fn fused_prepare_publishes_its_complete_current_policy() {
    // Given a fused runtime and a prepare-only request.
    let file = tempfile::tempfile().unwrap();
    let info = info();
    let mut reply = Vec::new();
    // When the actual worker protocol serves prepare then quit.
    worker::serve(
        &mut Fused,
        &file,
        &info,
        &mut Cursor::new(b"pq"),
        &mut reply,
    )
    .unwrap();
    // Then it publishes its full current output, preserving fused semantics.
    assert!(reply.ends_with(b"\n1\n"));
    let mut output = [0; 4];
    file.read_exact_at(&mut output, u64::try_from(info.input_bytes).unwrap())
        .unwrap();
    assert_eq!(output, 42f32.to_le_bytes());
}
