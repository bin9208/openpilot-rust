#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use sha2::{Digest, Sha256};
use std::fs;
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/compiler/original-matmul-cpu.json")).unwrap()
}
// Lossless fixture construction from the mandatory pinned model, avoiding a
// duplicate multi-megabyte copy of its weights. Both source and constructed
// prefix identities are independently captured original SHA256 values.
fn vi(data: &[u8], pos: &mut usize) -> usize {
    let mut n = 0;
    let mut shift = 0;
    loop {
        let b = data[*pos];
        *pos += 1;
        n |= ((b & 127) as usize) << shift;
        if b < 128 {
            return n;
        }
        shift += 7;
        assert!(shift < 64);
    }
}
fn fields(data: &[u8]) -> Vec<(usize, &[u8], &[u8])> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let start = pos;
        let tag = vi(data, &mut pos);
        let payload = match tag & 7 {
            2 => {
                let len = vi(data, &mut pos);
                let p = &data[pos..pos + len];
                pos += len;
                p
            }
            0 => {
                vi(data, &mut pos);
                &data[pos..pos]
            }
            1 => {
                pos += 8;
                &data[pos..pos]
            }
            5 => {
                pos += 4;
                &data[pos..pos]
            }
            _ => panic!("unsupported trusted fixture wire"),
        };
        out.push((tag >> 3, payload, &data[start..pos]));
    }
    out
}
fn varint(mut v: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while v > 127 {
        out.push((v as u8 & 127) | 128);
        v >>= 7;
    }
    out.push(v as u8);
    out
}
fn message(n: usize, data: &[u8]) -> Vec<u8> {
    [varint(n * 8 + 2), varint(data.len()), data.to_vec()].concat()
}
fn actual_prefix_source(case: &serde_json::Value) -> Vec<u8> {
    let directory = option_env!("CARGO_MANIFEST_DIR").unwrap_or("rust/crates/model-runtime");
    let path = std::path::Path::new(directory)
        .join("../../../openpilot/selfdrive/modeld/models")
        .join(case["source_model"].as_str().unwrap());
    let source = fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&source)),
        case["source_model_sha256"].as_str().unwrap()
    );
    let top = fields(&source);
    let graph = fields(top.iter().find(|v| v.0 == 7).unwrap().1);
    let nodes = graph
        .iter()
        .filter(|v| v.0 == 1)
        .take(case["last_node"].as_u64().unwrap() as usize + 1)
        .collect::<Vec<_>>();
    let required = nodes
        .iter()
        .flat_map(|v| fields(v.1))
        .filter(|v| v.0 == 1)
        .map(|v| v.1)
        .collect::<std::collections::BTreeSet<_>>();
    let mut prefix = nodes
        .iter()
        .flat_map(|v| v.2.iter().copied())
        .collect::<Vec<_>>();
    for (n, payload, raw) in &graph {
        if *n == 1 || *n == 12 {
            continue;
        }
        if *n == 5 {
            let name = fields(payload).into_iter().find(|v| v.0 == 8).unwrap().1;
            if !required.contains(name) {
                continue;
            }
        }
        prefix.extend_from_slice(raw);
    }
    let mut output = message(1, case["output_name"].as_str().unwrap().as_bytes());
    output.extend(message(2, &message(1, &[8, 1])));
    prefix.extend(message(12, &output));
    let bytes = top
        .iter()
        .flat_map(|(n, _, raw)| {
            if *n == 7 {
                message(7, &prefix)
            } else {
                raw.to_vec()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        case["prefix_sha256"].as_str().unwrap()
    );
    bytes
}
fn run(case: &serde_json::Value, actual: bool) -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let source = if actual {
        actual_prefix_source(case)
    } else {
        decode(case["onnx_hex"].as_str().unwrap())
    };
    let path = temp.path().join("model.onnx");
    fs::write(&path, &source).unwrap();
    let dest = temp.path().join("bundle");
    let compilation = compile_onnx(
        &path,
        &format!("{:x}", Sha256::digest(&source)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    if actual {
        model
            .write_input("input_img", &(0u8..=255).collect::<Vec<_>>().repeat(5400))
            .unwrap();
    } else {
        model
            .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
            .unwrap();
    }
    model.run();
    let name = &compilation.graph.outputs[0].name;
    let mut bytes = vec![0; model.output_size(name).unwrap()];
    model.read_output(name, &mut bytes).unwrap();
    bytes
}
#[test]
fn original_dynamic_image_matmul_and_gemm_outputs_match_native_compiled_artifacts() {
    for case in fixture()["cases"].as_array().unwrap() {
        assert_eq!(
            run(case, false),
            decode(case["expected_hex"].as_str().unwrap()),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn actual_driver_first_gemm_true_prefix_matches_original_hash() {
    let f = fixture();
    let c = &f["actual_driver_prefix97"];
    assert_eq!(
        format!("{:x}", Sha256::digest(run(c, true))),
        c["original"]["sha256"].as_str().unwrap()
    );
}
#[test]
fn unproved_matrix_operand_storage_aliases_reject_before_publication() {
    let f = fixture();
    for case in f["unsupported_aliases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(f["unsupported_scales"].as_array().unwrap())
    {
        let temp = tempfile::tempdir().unwrap();
        let source = decode(case["onnx_hex"].as_str().unwrap());
        let path = temp.path().join("model.onnx");
        fs::write(&path, &source).unwrap();
        let dest = temp.path().join("bundle");
        let result = compile_onnx(
            &path,
            &format!("{:x}", Sha256::digest(&source)),
            &CompileOptions {
                offline_bindings: true,
                ..CompileOptions::default()
            },
            &dest,
        );
        assert!(result.is_err(), "{}", case["name"]);
        assert!(!dest.exists());
        let error = result.err().unwrap();
        assert!(
            error.contains("alias") || error.contains("unit Gemm"),
            "{error}"
        );
    }
}
