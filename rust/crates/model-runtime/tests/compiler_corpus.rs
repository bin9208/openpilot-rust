//! Independent outputs captured from the original interpreter; the native
//! regression gate needs neither Python nor tinygrad to execute these cases.
#[allow(dead_code)]
#[path = "../src/compiler_onnx/mod.rs"]
mod compiler_onnx;
use compiler_onnx::{lower::LoweredGraph, Model, Tensor};
use std::collections::BTreeMap;
fn hex_decode(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn record(outputs: BTreeMap<String, Tensor>) -> String {
    outputs
        .into_iter()
        .map(|(name, tensor)| {
            format!(
                "ok\t{name}\t{}\t{}\t{}",
                tensor.dtype,
                tensor
                    .shape
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                tensor
                    .data
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn original_onnx_operator_captures_run_without_original_interpreter() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-lowering.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() >= 1629);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let bytes = hex_decode(case["onnx_hex"].as_str().unwrap());
        let inputs = case
            .get("inputs")
            .and_then(|v| v.as_array())
            .map(|inputs| {
                inputs
                    .iter()
                    .map(|v| {
                        (
                            v["name"].as_str().unwrap().to_string(),
                            Tensor {
                                dtype: v["dtype"].as_u64().unwrap() as u32,
                                shape: v["shape"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|v| v.as_i64().unwrap())
                                    .collect(),
                                data: hex_decode(v["data_hex"].as_str().unwrap()),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let native = Model::parse(&bytes)
            .and_then(|m| {
                if case["offline"].as_bool() == Some(true) {
                    LoweredGraph::lower_offline(&m)
                } else {
                    LoweredGraph::lower(&m)
                }
            })
            .and_then(|g| g.evaluate(&inputs));
        let expected = case["expected"].as_str().unwrap();
        if expected == "err" {
            assert!(
                native.is_err(),
                "{name}: original rejected but native accepted"
            );
        } else {
            assert_eq!(
                record(native.unwrap_or_else(|e| panic!("{name}: native failed: {e}"))),
                expected,
                "{name}: original/native output differs"
            );
        }
    }
}

#[path = "../src/compiler_launch.rs"]
mod compiler_launch;
fn dimensions(value: &serde_json::Value) -> Vec<compiler_launch::Dimension> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            if let Some(v) = v.get("i") {
                compiler_launch::Dimension::Integer(v.as_str().unwrap().parse().unwrap())
            } else if let Some(v) = v.get("f") {
                compiler_launch::Dimension::Float(v.as_str().unwrap().parse().unwrap())
            } else {
                compiler_launch::Dimension::Boolean(v["b"].as_bool().unwrap())
            }
        })
        .collect()
}
#[test]
fn original_launch_captures_preserve_all_4411_inputs_without_python() {
    use compiler_launch::{launch_dimensions, Dimension};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compiler/original-launch.json")).unwrap();
    let base = include_bytes!("fixtures/qcom/buffer_add.bin");
    let cases = fixture["cases"].as_array().unwrap();
    let unicode = fixture["unicode"].as_array().unwrap();
    assert_eq!(cases.len() + unicode.len(), 4411);
    for (index, case) in cases.iter().enumerate() {
        let global = dimensions(&case["global"]);
        let local = case.get("local").map(dimensions);
        let source = case
            .get("source")
            .unwrap_or(&fixture["default_source"])
            .as_str()
            .unwrap();
        let mut binary = if let Some(hex) = case.get("binary").and_then(|v| v.get("hex")) {
            hex_decode(hex.as_str().unwrap())
        } else {
            base.to_vec()
        };
        if let Some(registers) = case.get("binary").and_then(|v| v.get("registers")) {
            let offset = u32::from_le_bytes(binary[0x34..0x38].try_into().unwrap()) as usize + 0x14;
            binary[offset..offset + 4]
                .copy_from_slice(&(registers[0].as_u64().unwrap() as u32).to_le_bytes());
            binary[offset + 4..offset + 8]
                .copy_from_slice(&(registers[1].as_u64().unwrap() as u32).to_le_bytes());
        }
        let actual = launch_dimensions(&global, local.as_deref(), source, &binary);
        if let Some(error) = case.get("error") {
            assert_eq!(
                actual.unwrap_err(),
                error.as_str().unwrap(),
                "launch {index}"
            );
        } else {
            assert_eq!(
                actual.unwrap(),
                (dimensions(&case["groups"]), dimensions(&case["locals"])),
                "launch {index}"
            );
        }
    }
    for case in unicode {
        let point = case[0].as_u64().unwrap() as u32;
        let preserve = case[1].as_bool().unwrap();
        let source = format!("{}barrier(0);", char::from_u32(point).unwrap());
        let global = vec![
            Dimension::Integer(128),
            Dimension::Integer(1),
            Dimension::Integer(1),
        ];
        let (groups, local) = launch_dimensions(&global, None, &source, base).unwrap();
        let one = Dimension::Integer(1);
        assert_eq!(
            (groups, local),
            if preserve {
                (global, vec![one; 3])
            } else {
                (
                    vec![Dimension::Integer(2), one, one],
                    vec![Dimension::Integer(64), one, one],
                )
            },
            "Unicode U+{point:04X}"
        );
    }
}

#[cfg(feature = "native-skip-miri")]
#[test]
fn original_driver_import_tree_and_every_initializer_hash_match_without_python() {
    compare_import_fixture(
        include_str!("fixtures/compiler/original-driver-import.json"),
        918,
    );
}
#[cfg(feature = "native-skip-miri")]
#[test]
fn original_driving_import_tree_and_every_initializer_hash_match_without_python() {
    compare_import_fixture(
        include_str!("fixtures/compiler/original-driving-import.json"),
        1391,
    );
}
#[cfg(feature = "native-skip-miri")]
fn compare_import_fixture(json: &str, count: usize) {
    use compiler_onnx::{Attribute, ShapeDimension};
    use sha2::{Digest, Sha256};
    let fixture: serde_json::Value = serde_json::from_str(json).unwrap();
    let crate_directory = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::Path::new)
        .unwrap_or_else(|| {
            std::path::Path::new(file!())
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        });
    let path = crate_directory
        .join("../../..")
        .join(fixture["provenance"]["source"].as_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        fixture["provenance"]["source_sha256"].as_str().unwrap()
    );
    let model = Model::parse(&bytes).unwrap();
    let hex = |bytes: &[u8]| bytes.iter().map(|v| format!("{v:02x}")).collect::<String>();
    let names = |names: &[String]| {
        names
            .iter()
            .map(|s| hex(s.as_bytes()))
            .collect::<Vec<_>>()
            .join(",")
    };
    let integers = |values: &[i64]| {
        values
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut records = Vec::new();
    for (domain, version) in &model.opsets {
        records.push(format!("O\t{}\t{version}", hex(domain.as_bytes())));
    }
    for (kind, values) in [("I", &model.graph.inputs), ("O", &model.graph.outputs)] {
        for value in values {
            let shape = value
                .shape
                .iter()
                .map(|d| match d {
                    ShapeDimension::Fixed(n) => format!("i{n}"),
                    ShapeDimension::Symbol(s) => format!("s{}", hex(s.as_bytes())),
                    ShapeDimension::Unknown => "?".into(),
                })
                .collect::<Vec<_>>()
                .join(",");
            records.push(format!(
                "V\t{kind}\t{}\t{}\t{}\t{}\t{shape}",
                hex(value.name.as_bytes()),
                value.dtype,
                u8::from(value.optional),
                u8::from(value.sequence)
            ));
        }
    }
    for (index, node) in model.graph.nodes.iter().enumerate() {
        records.push(format!(
            "N\t{index}\t{}\t{}\t{}\t{}\t{}\t{}",
            hex(node.name.as_bytes()),
            hex(node.op.as_bytes()),
            hex(node.domain.as_bytes()),
            node.opset,
            names(&node.inputs),
            names(&node.outputs)
        ));
        for (name, value) in &node.attributes {
            let encoded = match value {
                Attribute::Float(v) => format!("f{}", hex(&v.to_le_bytes())),
                Attribute::Integer(v) => format!("i{v}"),
                Attribute::String(v) => format!("s{}", hex(v.as_bytes())),
                Attribute::Floats(v) => format!(
                    "F{}",
                    v.iter()
                        .map(|v| hex(&v.to_le_bytes()))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                Attribute::Integers(v) => format!("I{}", integers(v)),
                Attribute::Strings(v) => format!("S{}", names(v)),
                _ => panic!("pinned driver unexpectedly uses complex attribute family"),
            };
            records.push(format!("A\t{index}\t{}\t{encoded}", hex(name.as_bytes())));
        }
    }
    for (name, tensor) in &model.graph.initializers {
        records.push(format!(
            "T\t{}\t{}\t{}\t{:x}",
            hex(name.as_bytes()),
            tensor.dtype,
            integers(&tensor.shape),
            Sha256::digest(&tensor.data)
        ));
    }
    let metadata = model.pipeline_metadata().unwrap();
    records.push(format!("C\t{}", hex(metadata.checkpoint.as_bytes())));
    for (name, (start, stop)) in metadata.slices {
        records.push(format!("S\t{}\t{start}\t{stop}", hex(name.as_bytes())));
    }
    records.sort();
    let expected = fixture["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), count);
    assert_eq!(records, expected);
}

#[cfg(feature = "native-skip-miri")]
#[test]
fn both_pinned_model_graphs_lower_every_node_to_typed_plans() {
    let crate_directory = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::Path::new)
        .unwrap_or_else(|| {
            std::path::Path::new(file!())
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        });
    for (filename, count, width) in [
        ("dmonitoring_model.onnx", 201, 553),
        ("driving_supercombo.onnx", 352, 2576),
    ] {
        let path = crate_directory
            .join("../../../openpilot/selfdrive/modeld/models")
            .join(filename);
        let model = Model::load(&path).unwrap();
        assert_eq!(model.graph.nodes.len(), count);
        let lowered =
            LoweredGraph::lower(&model).unwrap_or_else(|error| panic!("{filename}: {error}"));
        assert_eq!(lowered.outputs.len(), 1);
        let output = &lowered.plans[*lowered.outputs.values().next().unwrap()].ty;
        assert_eq!(output.dtype, 10);
        assert_eq!(output.shape, [1, width]);
        // This gate deliberately proves typed graph coverage only. Numeric
        // execution and renderer/scheduler integration have separate gates.
    }
}

#[cfg(feature = "native-skip-miri")]
#[test]
fn every_original_model_intermediate_type_matches_in_both_binding_modes() {
    use sha2::{Digest, Sha256};
    let directory = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::Path::new)
        .unwrap_or_else(|| {
            std::path::Path::new(file!())
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        });
    for json in [
        include_str!("fixtures/compiler/dmonitoring_model-types.json"),
        include_str!("fixtures/compiler/dmonitoring_model-offline-types.json"),
        include_str!("fixtures/compiler/driving_supercombo-types.json"),
        include_str!("fixtures/compiler/driving_supercombo-offline-types.json"),
    ] {
        let fixture: serde_json::Value = serde_json::from_str(json).unwrap();
        let path = directory
            .join("../../..")
            .join(fixture["provenance"]["source"].as_str().unwrap());
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            fixture["provenance"]["source_sha256"].as_str().unwrap()
        );
        let model = Model::parse(&bytes).unwrap();
        let graph = if fixture["offline"].as_bool().unwrap() {
            LoweredGraph::lower_offline(&model)
        } else {
            LoweredGraph::lower(&model)
        }
        .unwrap();
        let mut actual = Vec::new();
        for node in &model.graph.nodes {
            for name in &node.outputs {
                let ty = &graph.plans[graph.names[name]].ty;
                actual.push(format!(
                    "{name}\t{}\t{}",
                    ty.dtype,
                    ty.shape
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }
        let expected = fixture["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
