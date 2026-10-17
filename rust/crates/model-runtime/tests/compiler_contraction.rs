#![cfg(feature = "native-skip-miri")]
use openpilot_model_runtime::{
    compiler_backend_cpu::{compile_onnx, CompileOptions},
    CpuModel,
};
use sha2::{Digest, Sha256};
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|x| u8::from_str_radix(std::str::from_utf8(x).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn original_dynamic_contraction_contexts() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-mulacc-f32.json")).unwrap();
    for memory_planning in [false, true] {
        for case in fixture["cases"].as_array().unwrap() {
            let directory = tempfile::tempdir().unwrap();
            let bytes = decode(case["onnx_hex"].as_str().unwrap());
            let source = directory.path().join("model.onnx");
            std::fs::write(&source, &bytes).unwrap();
            let bundle = directory.path().join("bundle");
            let result = compile_onnx(
                &source,
                &format!("{:x}", Sha256::digest(&bytes)),
                &CompileOptions {
                    memory_planning,
                    offline_bindings: true,
                    ..CompileOptions::default()
                },
                &bundle,
            );
            if !matches!(
                case["context"].as_str().unwrap(),
                "unheld"
                    | "swapped-add"
                    | "reversed-mul"
                    | "permuted-inputs"
                    | "constant-two"
                    | "constant-half"
                    | "constant-normal"
            ) {
                let error = result.unwrap_err();
                assert!(
                    error.contains("requires source fusion scheduling"),
                    "{}: {error}",
                    case["name"]
                );
                assert!(!bundle.exists());
                continue;
            }
            let compilation = result.unwrap();
            let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
            for input in &compilation.graph.inputs {
                let name = &input.name;
                let value = &case["inputs"][name];
                model
                    .write_input(name, &decode(value.as_str().unwrap()))
                    .unwrap();
            }
            model.run();
            for (name, value) in case["outputs"].as_object().unwrap() {
                let expected = decode(value.as_str().unwrap());
                let mut actual = vec![0; model.output_size(name).unwrap()];
                model.read_output(name, &mut actual).unwrap();
                assert_eq!(actual, expected, "{}:{name}", case["name"]);
            }
        }
    }
}

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
    let source = std::fs::read(path).unwrap();
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

#[test]
fn pinned_driving_true_prefix21_matches_original() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-driving-contraction21.json"
    ))
    .unwrap();
    let bytes = actual_prefix_source(&case);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("prefix.onnx");
    std::fs::write(&source, &bytes).unwrap();
    let dest = directory.path().join("bundle");
    let result = compile_onnx(
        &source,
        &format!("{:x}", Sha256::digest(&bytes)),
        &CompileOptions {
            offline_bindings: true,
            ..CompileOptions::default()
        },
        &dest,
    )
    .unwrap();
    let mut model = unsafe { CpuModel::load(&dest) }.unwrap();
    for input in &result.graph.inputs {
        let raw = match input.name.as_str() {
            "img" => (0u8..=255).collect::<Vec<_>>().repeat(1536),
            "big_img" => (0u8..=255).rev().collect::<Vec<_>>().repeat(1536),
            "desire_pulse" => vec![0; 25 * 8 * 4],
            "features_buffer" => vec![0; 24 * 512 * 4],
            "traffic_convention" | "action_t" => vec![0; 8],
            _ => panic!("unexpected input {}", input.name),
        };
        model.write_input(&input.name, &raw).unwrap();
    }
    model.run();
    let name = case["output_name"].as_str().unwrap();
    let mut raw = vec![0; model.output_size(name).unwrap()];
    model.read_output(name, &mut raw).unwrap();
    assert_eq!(raw.len(), 524288);
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        case["sha256"].as_str().unwrap()
    );
}

#[test]
fn image_residual_materialization_matches_original() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/compiler/original-image-muladd-barrier.json"
    ))
    .unwrap();
    for memory_planning in [false, true] {
        for case in fixture["cases"].as_array().unwrap() {
            let directory = tempfile::tempdir().unwrap();
            let bytes = decode(case["onnx_hex"].as_str().unwrap());
            let source = directory.path().join("model.onnx");
            std::fs::write(&source, &bytes).unwrap();
            let bundle = directory.path().join("bundle");
            let result = compile_onnx(
                &source,
                &format!("{:x}", Sha256::digest(&bytes)),
                &CompileOptions {
                    offline_bindings: true,
                    memory_planning,
                    ..CompileOptions::default()
                },
                &bundle,
            );
            if case["admission"] == "reject" {
                assert!(result
                    .unwrap_err()
                    .contains("requires source fusion scheduling"));
                assert!(!bundle.exists());
                continue;
            }
            result.unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
            let mut model = unsafe { CpuModel::load(&bundle) }.unwrap();
            model
                .write_input("a", &decode(case["input_hex"].as_str().unwrap()))
                .unwrap();
            model.run();
            for (name, expected) in case["outputs"].as_object().unwrap() {
                let mut raw = vec![0; model.output_size(name).unwrap()];
                model.read_output(name, &mut raw).unwrap();
                assert_eq!(
                    raw,
                    decode(expected.as_str().unwrap()),
                    "{}:{name}",
                    case["name"]
                );
            }
        }
    }
}
