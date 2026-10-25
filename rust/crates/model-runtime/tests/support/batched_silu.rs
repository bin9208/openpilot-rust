//! Independent original graphs and exact comparisons for fused SiLU.
use super::*;
use openpilot_model_runtime::compiler_onnx::{lower::LoweredGraph, Model};
fn provider() -> Config {
    let library = PathBuf::from(std::env::var_os("LLVM_PATH").expect("explicit LLVM_PATH"));
    let library_sha256 = format!("{:x}", Sha256::digest(fs::read(&library).unwrap()));
    assert!(matches!(
        library_sha256.as_str(),
        "5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0"
            | "4b93481bd1c9d42c0951275dd275d4a9d30db78c309ae2cc0405669bc6dbbf8a"
    ));
    Config {
        library,
        library_sha256,
        triple: "x86_64-none-unknown-elf".into(),
        cpu: "x86-64".into(),
        features: String::new(),
        passes: "default<O2>".into(),
        loop_unrolling: true,
        loop_vectorization: true,
        slp_vectorization: true,
        verify_each: true,
    }
}
fn fixture(config: &Config) -> Value {
    let bytes = include_bytes!("../fixtures/compiler/original-batched-silu-controls.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "6641eb8ed6c63e6a438b5adc3a98bd7d6219f280c00423b88df5a0a3a149ce82"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(fixture["captures"].as_array().unwrap().len(), 2);
    let receipt = fixture["captures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|capture| capture["library_sha256"] == config.library_sha256)
        .expect("independent original controls required for selected provider")
        .clone();
    assert_eq!(receipt["library_sha256"], config.library_sha256);
    assert_eq!(receipt["cache_disabled"], true);
    assert!(receipt["compile_to_obj_calls"].as_u64().unwrap() > 0);
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 35);
    assert_eq!(receipt["source_sha256"].as_object().unwrap().len(), 15);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for (name, expected) in receipt["source_sha256"].as_object().unwrap() {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(root.join(name)).unwrap())),
            expected.as_str().unwrap()
        );
    }
    receipt
}
fn decode(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn vi(mut n: usize) -> Vec<u8> {
    let mut out = vec![];
    while n > 127 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
    out
}
fn field(n: usize, value: usize) -> Vec<u8> {
    [vi(n * 8), vi(value)].concat()
}
fn message(n: usize, bytes: &[u8]) -> Vec<u8> {
    [vi(n * 8 + 2), vi(bytes.len()), bytes.to_vec()].concat()
}
fn value(field_id: usize, name: &str, shape: &[usize]) -> Vec<u8> {
    let dims = shape
        .iter()
        .flat_map(|n| message(1, &field(1, *n)))
        .collect::<Vec<_>>();
    message(
        field_id,
        &[
            message(1, name.as_bytes()),
            message(2, &message(1, &[field(1, 1), message(2, &dims)].concat())),
        ]
        .concat(),
    )
}
fn model(case: &Value) -> (Vec<u8>, Vec<u8>) {
    let dims = |key: &str| {
        case[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect::<Vec<_>>()
    };
    let x = dims("input_shape");
    let w = dims("weight_shape");
    let y = dims("output_shape");
    let repeat = |key: &str, bytes: usize| {
        decode(case[key].as_str().unwrap())
            .into_iter()
            .cycle()
            .take(bytes)
            .collect::<Vec<_>>()
    };
    let weights = repeat("weight_pattern_hex", w.iter().product::<usize>() * 2);
    let bias = repeat("bias_pattern_hex", w[1] * 2);
    let tensor = |name: &str, shape: &[usize], raw: &[u8], dtype: usize| {
        message(
            5,
            &[
                shape.iter().flat_map(|n| field(1, *n)).collect::<Vec<_>>(),
                field(2, dtype),
                message(8, name.as_bytes()),
                message(9, raw),
            ]
            .concat(),
        )
    };
    let node = |op: &str, inputs: &[&str], output: &str| {
        message(
            1,
            &[
                inputs
                    .iter()
                    .flat_map(|s| message(1, s.as_bytes()))
                    .collect::<Vec<_>>(),
                message(2, output.as_bytes()),
                message(4, op.as_bytes()),
            ]
            .concat(),
        )
    };
    let variant = case["variant"].as_str().unwrap();
    let mut nodes = [
        node("MatMul", &["a", "w"], "matrix"),
        node("Add", &["matrix", "bias"], "linear"),
        node(
            "Sigmoid",
            &[if variant == "wrong_sigmoid" {
                "matrix"
            } else {
                "linear"
            }],
            "sigmoid",
        ),
        node(
            "Mul",
            if variant == "commuted_mul" {
                &["sigmoid", "linear"]
            } else {
                &["linear", "sigmoid"]
            },
            "silu",
        ),
    ]
    .concat();
    let mut outputs = value(12, "silu", &y);
    if let Some(held) = variant.strip_prefix("held_") {
        outputs.extend(value(12, held, &y));
    } else if let Some(input) = variant.strip_suffix("_reader") {
        nodes.extend(if input == "matrix" {
            node("Relu", &[input], "extra")
        } else {
            node("Add", &[input, "delta"], "extra")
        });
        outputs.extend(value(12, "extra", &y));
    } else {
        assert!(matches!(
            variant,
            "final" | "wrong_sigmoid" | "commuted_mul"
        ));
    }
    let data = [
        message(8, &field(2, 20)),
        message(
            7,
            &[
                nodes,
                tensor("w", &w, &weights, 10),
                tensor("bias", &[w[1]], &bias, 10),
                tensor("delta", &[1], &0.25f32.to_le_bytes(), 1),
                value(11, "a", &x),
                outputs,
            ]
            .concat(),
        ),
    ]
    .concat();
    assert_eq!(
        format!("{:x}", Sha256::digest(&data)),
        case["onnx_sha256"].as_str().unwrap()
    );
    let words = case["input_words"].as_array().unwrap();
    let input = words
        .iter()
        .cycle()
        .take(x.iter().product())
        .flat_map(|v| (v.as_u64().unwrap() as u32).to_le_bytes())
        .collect();
    (data, input)
}
#[test]
fn original_exceptional_and_saturation_silu_whole_callers_are_exact() {
    let config = provider();
    let f = fixture(&config);
    let mut exact = 0;
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == "final")
    {
        let (source, input) = model(case);
        for memory_planning in [false, true] {
            eprintln!("{} memory={memory_planning}", case["name"]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("model.onnx");
            fs::write(&path, &source).unwrap();
            let destination = temp.path().join("bundle");
            let compilation = compile_onnx(
                &path,
                case["onnx_sha256"].as_str().unwrap(),
                &CompileOptions {
                    offline_bindings: true,
                    memory_planning,
                    llvm: Some((
                        config.clone(),
                        PathBuf::from(env!("CARGO_BIN_EXE_offline-llvm-compile")),
                    )),
                    ..CompileOptions::default()
                },
                &destination,
            )
            .unwrap();
            assert_eq!(compilation.graph.backend, "cpu-llvm");
            // SAFETY: locally compiled immutable bundle; exact original input
            // byte count, synchronous execution and output-sized destination.
            let mut native = unsafe { CpuModel::load(&destination) }.unwrap();
            native.write_input("a", &input).unwrap();
            native.run();
            let mut actual = vec![0; native.output_size("silu").unwrap()];
            native.read_output("silu", &mut actual).unwrap();
            assert_eq!(
                actual,
                decode(case["outputs_hex"]["silu"].as_str().unwrap()),
                "{} memory={memory_planning}",
                case["name"]
            );
            exact += 1;
        }
    }
    assert_eq!(exact, 38);
}

#[test]
fn held_extra_readers_and_operand_identity_do_not_acquire_silu_fusion() {
    let config = provider();
    let f = fixture(&config);
    let mut checked = 0;
    for case in f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] != "final")
    {
        let (source, _) = model(case);
        let source = Model::parse(&source).unwrap();
        let lowered = LoweredGraph::lower_offline(&source).unwrap();
        let selected = openpilot_model_runtime::compiler_backend_cpu::matrix::graph::closures(
            &lowered, &source,
        )
        .unwrap();
        assert!(
            !selected.iter().any(|c| c.silu.is_some()),
            "{} must preserve its original materialization/operand identity",
            case["name"]
        );
        if case["variant"] == "held_linear" {
            assert!(selected
                .iter()
                .any(|c| c.bias.is_some() && c.silu.is_none()));
        }
        checked += 1;
    }
    assert_eq!(checked, 16);
}

pub(super) fn actual_prefix_source(case: &Value) -> Vec<u8> {
    prefix_wire::source(case)
}
mod prefix_wire {
    use super::*;
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
    pub(super) fn source(case: &serde_json::Value) -> Vec<u8> {
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
}
