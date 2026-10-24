#[allow(dead_code)]
#[path = "../src/compiler_onnx/mod.rs"]
mod compiler_onnx;

use compiler_onnx::{Model, ShapeDimension};

fn driver() -> std::path::PathBuf {
    let crate_directory = option_env!("CARGO_MANIFEST_DIR")
        .map(std::path::Path::new)
        .unwrap_or_else(|| {
            std::path::Path::new(file!())
                .parent()
                .unwrap()
                .parent()
                .unwrap()
        });
    crate_directory.join("../../../openpilot/selfdrive/modeld/models/dmonitoring_model.onnx")
}

#[test]
fn imports_actual_driver_graph_without_tensor_execution() {
    let model = Model::load(&driver()).unwrap();
    assert_eq!(model.graph.nodes.len(), 201);
    assert_eq!(model.graph.initializers.len(), 209);
    assert_eq!(model.opsets.get(""), Some(&20));
    assert_eq!(model.graph.inputs[0].name, "input_img");
    assert_eq!(model.graph.inputs[0].dtype, 2);
    assert_eq!(
        model.graph.inputs[0].shape,
        vec![ShapeDimension::Fixed(1), ShapeDimension::Fixed(1382400)]
    );
    assert_eq!(model.graph.inputs[1].name, "calib");
    assert_eq!(model.graph.inputs[1].dtype, 1);
    assert_eq!(model.graph.outputs[0].dtype, 10);
    assert_eq!(
        model.graph.outputs[0].shape,
        vec![ShapeDimension::Fixed(1), ShapeDimension::Fixed(553)]
    );
    let metadata = model.pipeline_metadata().unwrap();
    assert_eq!(
        metadata.checkpoint,
        "a9462a65-1886-462a-8847-ad4624d9abfc/200"
    );
    assert_eq!(metadata.slices.len(), 20);
    assert_eq!(metadata.slices["features"], (41, 553));
    assert_eq!(metadata.slices["face_descs_lhd"], (0, 12));
    assert_eq!(
        model
            .graph
            .initializers
            .values()
            .filter(|t| t.dtype == 10)
            .count(),
        201
    );
    assert_eq!(
        model
            .graph
            .initializers
            .values()
            .filter(|t| t.dtype == 7)
            .count(),
        8
    );
    model.admit_offline().unwrap();
}

#[test]
fn importer_rejects_a_git_lfs_pointer_in_place_of_onnx() {
    assert!(Model::parse(b"version https://git-lfs.github.com/spec/v1\n").is_err());
}

#[test]
fn parser_rejects_truncation_varint_overflow_and_unsupported_wire_type() {
    for bytes in [&[0x3a, 0xff][..], &[0x80; 11][..], &[0x0b][..]] {
        assert!(Model::parse(bytes).is_err());
    }
}

#[test]
fn admits_exactly_the_original_27_operator_names() {
    let expected = [
        "Identity",
        "Cast",
        "Concat",
        "Sub",
        "Div",
        "Conv",
        "Gelu",
        "Mul",
        "Add",
        "ReduceMean",
        "Relu",
        "Sigmoid",
        "Reshape",
        "Gemm",
        "ReduceL2",
        "Clip",
        "Expand",
        "Slice",
        "Unsqueeze",
        "Transpose",
        "GatherND",
        "MatMul",
        "LayerNormalization",
        "Squeeze",
        "Where",
        "Softmax",
        "Gather",
    ];
    assert_eq!(compiler_onnx::STATIC_OPERATORS.len(), 27);
    for name in expected {
        assert!(compiler_onnx::STATIC_OPERATORS.contains(&name));
    }
}

fn integer(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut b = (value & 127) as u8;
        value >>= 7;
        if value != 0 {
            b |= 128;
        }
        out.push(b);
        if value == 0 {
            return out;
        }
    }
}
fn field(id: u32, wire: u8, value: &[u8]) -> Vec<u8> {
    let mut out = integer(((id << 3) | wire as u32) as u64);
    if wire == 2 {
        out.extend(integer(value.len() as u64));
    }
    out.extend(value);
    out
}
fn string(id: u32, value: &str) -> Vec<u8> {
    field(id, 2, value.as_bytes())
}
fn message(id: u32, fields: &[Vec<u8>]) -> Vec<u8> {
    field(id, 2, &fields.concat())
}
fn source_with_node(op: &str, inputs: &[&str]) -> Vec<u8> {
    let mut node = vec![string(4, op)];
    node.extend(inputs.iter().map(|s| string(1, s)));
    message(7, &[message(1, &node)])
}

#[test]
fn rejects_runtime_shape_controls_for_every_original_control_slot() {
    for (op, slots) in [
        ("Expand", vec![1]),
        ("Reshape", vec![1]),
        ("Squeeze", vec![1]),
        ("Unsqueeze", vec![1]),
        ("ReduceMean", vec![1]),
        ("ReduceL2", vec![1]),
        ("Slice", vec![1, 2, 3, 4]),
    ] {
        for slot in slots {
            let mut inputs = vec!["data"; slot + 1];
            for input in &mut inputs[1..] {
                *input = "";
            }
            inputs[slot] = "runtime_control";
            let model = Model::parse(&source_with_node(op, &inputs)).unwrap();
            assert!(model
                .admit_offline()
                .unwrap_err()
                .contains("runtime content"));
        }
    }
    Model::parse(&source_with_node("Squeeze", &["data", ""]))
        .unwrap()
        .admit_offline()
        .unwrap();
    assert!(Model::parse(&source_with_node("Constant", &[]))
        .unwrap()
        .admit_offline()
        .unwrap_err()
        .contains("unsupported"));
}

#[test]
fn imports_real_initializer_bytes_and_static_shape_controls() {
    let weight = message(
        5,
        &[
            field(1, 0, &integer(2)),
            field(2, 0, &integer(1)),
            string(8, "weight"),
            field(9, 2, &[0, 0, 160, 63, 0, 0, 32, 192]),
        ],
    );
    let shape = message(
        5,
        &[
            field(1, 0, &integer(1)),
            field(2, 0, &integer(7)),
            string(8, "shape"),
            field(7, 2, &integer(2)),
        ],
    );
    let node = message(
        1,
        &[
            string(4, "Reshape"),
            string(1, "weight"),
            string(1, "shape"),
        ],
    );
    let model = Model::parse(&message(7, &[weight, shape, node])).unwrap();
    assert_eq!(
        model.graph.initializers["weight"].data,
        [0, 0, 160, 63, 0, 0, 32, 192]
    );
    assert_eq!(model.graph.initializers["shape"].data, 2i64.to_le_bytes());
    model.admit_offline().unwrap();
}

#[test]
fn rejects_initializer_lengths_and_conflicting_data_fields() {
    for fields in [
        vec![field(9, 2, &[0, 0])],
        vec![
            field(9, 2, &[0, 0, 0, 0]),
            field(4, 5, &1.0f32.to_le_bytes()),
        ],
    ] {
        let mut tensor = vec![
            field(1, 0, &integer(1)),
            field(2, 0, &integer(1)),
            string(8, "weight"),
        ];
        tensor.extend(fields);
        assert!(Model::parse(&message(7, &[message(5, &tensor)])).is_err());
    }
}

#[test]
fn safe_slice_decoder_never_resolves_arbitrary_globals_or_reductions() {
    use compiler_onnx::metadata::decode_pickle_slices;
    for data in [
        b"cos\nsystem\n.".as_slice(),
        b"\x80\x04}\x8c\x02os\x8c\x06system\x93.",
        b"\x80\x04}NR.",
    ] {
        assert!(decode_pickle_slices(data).is_err());
    }
    // Valid inert dictionary: {'x': slice(-2, None, 1)}.
    let data =
        b"\x80\x04}(\x8c\x01x\x8c\x08builtins\x8c\x05slice\x93J\xfe\xff\xff\xffNK\x01\x87Ru.";
    assert_eq!(
        decode_pickle_slices(data).unwrap()["x"],
        [Some(-2), None, Some(1)]
    );
    for end in 0..data.len() {
        assert!(decode_pickle_slices(&data[..end]).is_err());
    }
}

#[test]
fn malformed_scalar_float_attribute_does_not_panic() {
    let attribute = message(
        5,
        &[
            string(1, "epsilon"),
            field(20, 0, &integer(1)),
            field(2, 2, &[]),
        ],
    );
    let source = message(
        7,
        &[message(1, &[string(4, "LayerNormalization"), attribute])],
    );
    assert!(Model::parse(&source).is_err());
}

#[test]
fn external_initializers_read_only_the_declared_source_range() {
    let directory = std::env::temp_dir().join(format!(
        "native-onnx-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let blob = [99, 98, 0, 0, 160, 63, 0, 0, 32, 192, 97];
    std::fs::write(directory.join("weights.bin"), blob).unwrap();
    let tensor = message(
        5,
        &[
            field(1, 0, &integer(2)),
            field(2, 0, &integer(1)),
            string(8, "weight"),
            field(14, 0, &integer(1)),
            message(13, &[string(1, "location"), string(2, "weights.bin")]),
            message(13, &[string(1, "offset"), string(2, "2")]),
            message(13, &[string(1, "length"), string(2, "8")]),
        ],
    );
    let source = message(7, &[tensor]);
    assert!(Model::parse(&source).unwrap_err().contains("origin"));
    let path = directory.join("model.onnx");
    std::fs::write(&path, &source).unwrap();
    assert_eq!(
        Model::load(&path).unwrap().graph.initializers["weight"].data,
        blob[2..10]
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn preserves_bool_initializer_conversion() {
    let initializer = |dtype| {
        message(
            5,
            &[
                field(1, 0, &integer(1)),
                field(2, 0, &integer(dtype)),
                string(8, "weight"),
                field(5, 2, &integer(2)),
            ],
        )
    };
    assert_eq!(
        Model::parse(&message(7, &[initializer(9)]))
            .unwrap()
            .graph
            .initializers["weight"]
            .data,
        [1]
    );
}

#[test]
fn rejects_attribute_kind_that_disagrees_with_its_scalar_value() {
    let attribute = message(
        5,
        &[
            string(1, "epsilon"),
            field(20, 0, &integer(1)),
            field(3, 0, &integer(1)),
        ],
    );
    assert!(Model::parse(&message(
        7,
        &[message(1, &[string(4, "LayerNormalization"), attribute])]
    ))
    .is_err());
}

#[test]
fn retains_optional_sequence_and_symbolic_shape_metadata_before_admission() {
    let shape = message(
        2,
        &[
            message(1, &[string(2, "batch")]),
            message(1, &[field(1, 0, &integer(3))]),
        ],
    );
    let tensor_type = message(1, &[field(1, 0, &integer(1)), shape]);
    let wrapped = message(
        9,
        &[message(1, &[message(4, &[message(1, &[tensor_type])])])],
    );
    let input = message(11, &[string(1, "input"), message(2, &[wrapped])]);
    let model = Model::parse(&message(7, &[input])).unwrap();
    let value = &model.graph.inputs[0];
    assert!(value.optional && value.sequence);
    assert_eq!(
        value.shape,
        vec![
            ShapeDimension::Symbol("batch".into()),
            ShapeDimension::Fixed(3)
        ]
    );
}

#[test]
fn typed_integer_float_initializers_match_original_numeric_casts_not_bitpatterns() {
    let values = [
        0i64,
        1,
        -1,
        2,
        15360,
        16256,
        2049,
        2051,
        32767,
        65504,
        65519,
        65520,
        65535,
        i32::MAX as i64,
        i32::MIN as i64,
        i64::MAX,
        i64::MIN,
    ];
    // Captured from unmodified OnnxPBParser (DEV=PYTHON), not ONNX-spec
    // reinterpretation: 15360 becomes half(15360), not half bits 0x3c00.
    for (dtype,expected) in [
        (1,"000000000000803f000080bf000000400000704600007e46001000450030004500feff4600e07f4700ef7f4700f07f4700ff7f470000004f000000cf0000005f000000df"),
        (10,"0000003c00bc00408073f073006802680078ff7bff7b007c007c007c00fc007c00fc"),
        (11,"0000000000000000000000000000f03f000000000000f0bf0000000000000040000000000000ce400000000000c0cf40000000000002a040000000000006a04000000000c0ffdf400000000000fcef4000000000e0fdef400000000000feef4000000000e0ffef400000c0ffffffdf41000000000000e0c1000000000000e043000000000000e0c3"),
        (16,"0000803f80bf004070467e460045004500478047804780478047004f00cf005f00df"),
    ] {
        let expected=expected.as_bytes().chunks_exact(2).map(|v|u8::from_str_radix(std::str::from_utf8(v).unwrap(),16).unwrap()).collect::<Vec<_>>();
        for storage_field in [5,7,11] {
            let packed=values.iter().flat_map(|&v|integer(v as u64)).collect::<Vec<_>>();
            let tensor=message(5,&[field(1,0,&integer(values.len() as u64)),field(2,0,&integer(dtype)),string(8,"weight"),field(storage_field,2,&packed)]);
            assert_eq!(Model::parse(&message(7,&[tensor])).unwrap().graph.initializers["weight"].data,expected);
        }
    }
}

#[test]
fn raw_scalar_bfloat16_preserves_original_parser_rejection() {
    let tensor = message(
        5,
        &[
            field(2, 0, &integer(16)),
            string(8, "weight"),
            field(9, 2, &[128, 63]),
        ],
    );
    assert!(Model::parse(&message(7, &[tensor])).is_err());
}
