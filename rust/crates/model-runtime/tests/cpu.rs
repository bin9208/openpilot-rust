#![cfg(feature = "native-skip-miri")]

use openpilot_model_runtime::CpuModel;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn bundle() -> (TempDir, Value) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("kernels.c");
    fs::write(&source, include_str!("fixtures/kernels.c")).unwrap();
    let mut compiler = Command::new("clang");
    compiler.args(["-shared", "-fPIC", "-O2"]);
    if std::env::var_os("MODEL_TEST_ASAN").is_some() {
        compiler.arg("-fsanitize=address");
    }
    assert!(compiler
        .arg(source)
        .arg("-o")
        .arg(dir.path().join("kernels.so"))
        .status()
        .unwrap()
        .success());
    let weights = [1.0_f32, 2.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    fs::write(dir.path().join("weights.bin"), &weights).unwrap();
    let graph = json!({
        "version":1, "backend":"cpu-clang", "arch":std::env::consts::ARCH,
        "weights_sha256":format!("{:x}", Sha256::digest(&weights)),
        "library_sha256":format!("{:x}", Sha256::digest(fs::read(dir.path().join("kernels.so")).unwrap())),
        "allocations":[{"bytes":16,"weight_offset":0},{"bytes":8,"weight_offset":null}],
        "views":[{"allocation":0,"offset":0,"bytes":8},
                 {"allocation":0,"offset":8,"bytes":4},
                 {"allocation":1,"offset":0,"bytes":8},
                 {"allocation":0,"offset":4,"bytes":8}],
        "inputs":[{"name":"input","view":2}],
        "outputs":[{"name":"sum","view":1},{"name":"alias","view":3}],
        "kernels":[{"buffers":2,"scalars":1},{"buffers":2,"scalars":0}],
        "calls":[{"kernel":0,"views":[0,2],"scalars":[0],"workers":2,"core_id":0},
                 {"kernel":1,"views":[1,0],"scalars":[],"workers":1,"core_id":null}]
    });
    save_graph(dir.path(), &graph);
    (dir, graph)
}

fn save_graph(path: &Path, graph: &Value) {
    fs::write(path.join("graph.json"), serde_json::to_vec(graph).unwrap()).unwrap();
}

fn load(path: &Path) -> Result<CpuModel, openpilot_model_runtime::Error> {
    // SAFETY: fixtures compile the audited kernels above with matching ranges and ABI.
    unsafe { CpuModel::load(path) }
}

#[test]
fn preserves_recurrent_state_and_overlapping_output_views() {
    let (dir, _) = bundle();
    let mut model = load(dir.path()).unwrap();
    for (input, expected) in [
        ([3.0_f32, 4.0], [8.0_f32, 13.0]),
        ([1.0, 2.0], [18.0, 29.0]),
    ] {
        model
            .write_input(
                "input",
                &input
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        model.run();
        let mut output = [0_u8; 8];
        model.read_output("alias", &mut output).unwrap();
        assert_eq!(
            output.to_vec(),
            expected
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        );
    }
    let mut fresh = load(dir.path()).unwrap();
    fresh.run();
    let mut output = [0_u8; 4];
    fresh.read_output("sum", &mut output).unwrap();
    assert_eq!(f32::from_le_bytes(output), 6.0);
}

#[test]
fn rejects_bad_bindings_and_lengths_without_mutating_state() {
    let (dir, _) = bundle();
    let mut model = load(dir.path()).unwrap();
    assert!(model.write_input("missing", &[0; 8]).is_err());
    assert!(model.write_input("input", &[0; 7]).is_err());
    assert!(model.read_output("sum", &mut [0; 8]).is_err());
    assert!(model.read_output("missing", &mut [0; 4]).is_err());
    model.run();
    let mut output = [0_u8; 4];
    model.read_output("sum", &mut output).unwrap();
    assert_eq!(f32::from_le_bytes(output), 6.0);
}

#[test]
fn rejects_corrupt_assets_and_short_weight_ranges() {
    let (dir, mut graph) = bundle();
    fs::write(dir.path().join("weights.bin"), [0_u8; 16]).unwrap();
    assert!(load(dir.path()).is_err());
    graph["weights_sha256"] = json!(format!("{:x}", Sha256::digest([0_u8; 16])));
    graph["allocations"][0]["weight_offset"] = json!(1);
    save_graph(dir.path(), &graph);
    assert!(load(dir.path()).is_err());
    graph["allocations"][0]["weight_offset"] = json!(0);
    save_graph(dir.path(), &graph);
    fs::write(dir.path().join("kernels.so"), b"broken").unwrap();
    assert!(load(dir.path()).is_err());
}

#[test]
fn rejects_missing_kernel_symbol() {
    let (dir, mut graph) = bundle();
    graph["kernels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"buffers":0,"scalars":0}));
    save_graph(dir.path(), &graph);
    assert!(load(dir.path()).is_err());
}

#[test]
fn rejects_unreferenced_weight_bytes_even_with_matching_checksum() {
    let (dir, mut graph) = bundle();
    fs::write(dir.path().join("weights.bin"), [0_u8; 17]).unwrap();
    graph["weights_sha256"] = json!(format!("{:x}", Sha256::digest([0_u8; 17])));
    save_graph(dir.path(), &graph);
    assert!(load(dir.path()).is_err());
}

#[test]
fn cli_replays_multiple_frames_in_one_native_process() {
    let (dir, _) = bundle();
    let input = [3.0_f32, 4.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    fs::write(dir.path().join("input.bin"), input).unwrap();
    let sequence = json!([
        {"inputs":{"input":"input.bin"},"outputs":{"sum":"first.bin"}},
        {"inputs":{"input":"input.bin"},"outputs":{"sum":"second.bin"}}
    ]);
    fs::write(
        dir.path().join("sequence.json"),
        serde_json::to_vec(&sequence).unwrap(),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_model-run"))
        .arg("--trusted-bundle")
        .arg(dir.path())
        .arg(dir.path().join("sequence.json"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(dir.path().join("first.bin")).unwrap(),
        13.0_f32.to_le_bytes()
    );
    assert_eq!(
        fs::read(dir.path().join("second.bin")).unwrap(),
        33.0_f32.to_le_bytes()
    );
}

#[test]
fn cli_named_stages_share_state_and_skip_unselected_calls() {
    let (dir, mut graph) = bundle();
    graph["version"] = json!(2);
    graph["entrypoints"] = json!([
        {"name":"update", "start":0, "end":1},
        {"name":"sum", "start":1, "end":2},
        {"name":"empty", "start":2, "end":2}
    ]);
    save_graph(dir.path(), &graph);
    let sequence = json!([
        {"entrypoint":"update", "inputs":{}, "outputs":{"sum":"before.bin"}},
        {"entrypoint":"empty", "inputs":{}, "outputs":{}},
        {"entrypoint":"sum", "inputs":{}, "outputs":{"sum":"after.bin"}},
        {"entrypoint":"sum", "inputs":{}, "outputs":{"sum":"again.bin"}}
    ]);
    fs::write(
        dir.path().join("sequence.json"),
        serde_json::to_vec(&sequence).unwrap(),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_model-run"))
        .arg("--trusted-bundle")
        .arg(dir.path())
        .arg(dir.path().join("sequence.json"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(dir.path().join("before.bin")).unwrap(),
        0.0_f32.to_le_bytes()
    );
    assert_eq!(
        fs::read(dir.path().join("after.bin")).unwrap(),
        6.0_f32.to_le_bytes()
    );
    assert_eq!(
        fs::read(dir.path().join("again.bin")).unwrap(),
        6.0_f32.to_le_bytes()
    );
}
