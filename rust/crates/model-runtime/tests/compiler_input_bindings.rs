#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use sha2::{Digest, Sha256};
use std::fs;

fn varint(mut value: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 128 {
        bytes.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}
fn number(field: usize, value: usize) -> Vec<u8> {
    [varint(field * 8), varint(value)].concat()
}
fn message(field: usize, value: &[u8]) -> Vec<u8> {
    [varint(field * 8 + 2), varint(value.len()), value.to_vec()].concat()
}
fn value(field: usize, name: &str, dtype: usize, elements: usize) -> Vec<u8> {
    message(
        field,
        &[
            message(1, name.as_bytes()),
            message(
                2,
                &message(
                    1,
                    &[
                        number(1, dtype),
                        message(2, &message(1, &number(1, elements))),
                    ]
                    .concat(),
                ),
            ),
        ]
        .concat(),
    )
}
fn source(declare_live: bool) -> Vec<u8> {
    let node = message(
        1,
        &[
            message(1, b"live"),
            message(1, b"offset"),
            message(2, b"output"),
            message(4, b"Add"),
        ]
        .concat(),
    );
    let constant = message(
        5,
        &[
            number(1, 2),
            number(2, 1),
            message(8, b"offset"),
            message(9, &[1f32.to_le_bytes(), 2f32.to_le_bytes()].concat()),
        ]
        .concat(),
    );
    [
        message(8, &number(2, 20)),
        message(
            7,
            &[
                node,
                constant,
                if declare_live {
                    value(11, "live", 1, 2)
                } else {
                    Vec::new()
                },
                value(11, "unused", 10, 3),
                value(12, "output", 1, 2),
            ]
            .concat(),
        ),
    ]
    .concat()
}

#[test]
fn unused_declared_input_retains_the_original_binding_contract() {
    let source = source(true);
    for offline_bindings in [false, true] {
        for memory_planning in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let path = temporary.path().join("model.onnx");
            let bundle = temporary.path().join("bundle");
            fs::write(&path, &source).unwrap();
            let compilation = compile_onnx(
                &path,
                &format!("{:x}", Sha256::digest(&source)),
                &CompileOptions {
                    offline_bindings,
                    memory_planning,
                    ..CompileOptions::default()
                },
                &bundle,
            )
            .unwrap();
            let names = compilation
                .graph
                .inputs
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>();
            assert_eq!(names, ["live", "unused"]);
            let unused = compilation.graph.inputs[1].view;
            let bytes = if offline_bindings { 12 } else { 6 };
            assert_eq!(compilation.graph.views[unused].bytes, bytes);
            assert!(compilation
                .graph
                .calls
                .iter()
                .all(|c| !c.views.contains(&unused)));
            // SAFETY: trusted local compiler output with checked input capacities.
            let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
            assert!(model.write_input("unknown", &[0; 8]).is_err());
            assert!(model.write_input("live", &[0; 7]).is_err());
            assert!(model.write_input("unused", &vec![0; bytes - 1]).is_err());
            model
                .write_input(
                    "live",
                    &[3f32.to_le_bytes(), (-4f32).to_le_bytes()].concat(),
                )
                .unwrap();
            for byte in [0xa5, 0x5a] {
                model.write_input("unused", &vec![byte; bytes]).unwrap();
                model.run();
                let mut output = [0u8; 8];
                model.read_output("output", &mut output).unwrap();
                assert_eq!(
                    output.to_vec(),
                    [4f32.to_le_bytes(), (-2f32).to_le_bytes()].concat()
                );
            }
        }
    }
}

#[test]
fn pruning_does_not_accept_an_undeclared_live_input() {
    let source = source(false);
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("model.onnx");
    let bundle = temporary.path().join("bundle");
    fs::write(&path, &source).unwrap();
    let error = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &bundle,
    )
    .unwrap_err();
    assert_eq!(error, "missing input 0 for Add");
    assert!(!bundle.exists());
}
