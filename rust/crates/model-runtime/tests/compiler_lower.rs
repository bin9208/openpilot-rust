#[allow(dead_code)]
#[path = "../src/compiler_onnx/mod.rs"]
mod compiler_onnx;
use compiler_onnx::lower::{LoweredGraph, ScalarExpr};
use compiler_onnx::{Attribute, Graph, Model, Node, ShapeDimension, Tensor, Value};
use std::collections::BTreeMap;
fn value(name: &str, dtype: u32, shape: &[i64]) -> Value {
    Value {
        name: name.into(),
        dtype,
        shape: shape.iter().copied().map(ShapeDimension::Fixed).collect(),
        optional: false,
        sequence: false,
    }
}
fn node(op: &str, inputs: &[&str], output: &str) -> Node {
    Node {
        name: op.into(),
        op: op.into(),
        domain: String::new(),
        opset: 20,
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
        outputs: vec![output.into()],
        attributes: BTreeMap::new(),
    }
}
fn model(inputs: Vec<Value>, nodes: Vec<Node>) -> Model {
    Model {
        graph: Graph {
            inputs,
            nodes,
            outputs: vec![value("output", 1, &[])],
            initializers: BTreeMap::new(),
        },
        opsets: BTreeMap::new(),
        metadata_properties: BTreeMap::new(),
    }
}
fn f32s(shape: &[i64], values: &[f32]) -> Tensor {
    Tensor {
        dtype: 1,
        shape: shape.to_vec(),
        data: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}
fn i64s(values: &[i64]) -> Tensor {
    Tensor {
        dtype: 7,
        shape: vec![values.len() as i64],
        data: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}
fn run(model: &Model, inputs: BTreeMap<String, Tensor>) -> Tensor {
    LoweredGraph::lower(model)
        .unwrap()
        .evaluate(&inputs)
        .unwrap()
        .remove("output")
        .unwrap()
}
#[test]
fn lowers_broadcast_add_to_explicit_scalar_loads_and_index_maps() {
    let model = model(
        vec![value("a", 1, &[2, 1]), value("b", 1, &[1, 3])],
        vec![node("Add", &["a", "b"], "output")],
    );
    let lowered = LoweredGraph::lower(&model).unwrap();
    assert!(matches!(
        lowered.plans.last().unwrap().expression,
        ScalarExpr::Binary { .. }
    ));
    assert_eq!(
        run(
            &model,
            BTreeMap::from([
                ("a".into(), f32s(&[2, 1], &[1., 2.])),
                ("b".into(), f32s(&[1, 3], &[10., 20., 30.]))
            ])
        ),
        f32s(&[2, 3], &[11., 21., 31., 12., 22., 32.])
    );
}
#[test]
fn lowers_transpose_then_reshape_without_reordering_the_wrong_axis() {
    let mut transpose = node("Transpose", &["a"], "transposed");
    transpose
        .attributes
        .insert("perm".into(), Attribute::Integers(vec![1, 0]));
    let mut model = model(
        vec![value("a", 1, &[2, 3])],
        vec![
            transpose,
            node("Reshape", &["transposed", "shape"], "output"),
        ],
    );
    model
        .graph
        .initializers
        .insert("shape".into(), i64s(&[3, 2]));
    assert_eq!(
        run(
            &model,
            BTreeMap::from([("a".into(), f32s(&[2, 3], &[0., 1., 2., 3., 4., 5.]))])
        ),
        f32s(&[3, 2], &[0., 3., 1., 4., 2., 5.])
    );
}
#[test]
fn integer_division_truncates_toward_zero_and_preserves_zero_denominator_contract() {
    let tensor = |v: &[i32]| Tensor {
        dtype: 6,
        shape: vec![4],
        data: v.iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    let model = model(
        vec![value("a", 6, &[4]), value("b", 6, &[4])],
        vec![node("Div", &["a", "b"], "output")],
    );
    assert_eq!(
        run(
            &model,
            BTreeMap::from([
                ("a".into(), tensor(&[-7, 7, -7, 5])),
                ("b".into(), tensor(&[3, -3, -3, 0]))
            ])
        ),
        tensor(&[-2, -2, 2, 0])
    );
}
#[test]
fn rejects_incompatible_broadcasts_dynamic_controls_and_unimplemented_operators() {
    assert!(LoweredGraph::lower(&model(
        vec![value("a", 1, &[2]), value("b", 1, &[3])],
        vec![node("Add", &["a", "b"], "output")]
    ))
    .is_err());
    assert!(LoweredGraph::lower(&model(
        vec![value("a", 1, &[2]), value("shape", 7, &[1])],
        vec![node("Reshape", &["a", "shape"], "output")]
    ))
    .is_err());
    assert!(LoweredGraph::lower(&model(
        vec![value("a", 1, &[2])],
        vec![node("Sin", &["a"], "output")]
    ))
    .unwrap_err()
    .contains("unsupported offline ONNX operator"));
}

#[test]
fn boolean_subtraction_preserves_original_logical_negation_then_addition() {
    let model = model(
        vec![value("a", 9, &[4]), value("b", 9, &[4])],
        vec![node("Sub", &["a", "b"], "output")],
    );
    let tensor = |bytes: &[u8]| Tensor {
        dtype: 9,
        shape: vec![4],
        data: bytes.to_vec(),
    };
    assert_eq!(
        run(
            &model,
            BTreeMap::from([
                ("a".into(), tensor(&[0, 0, 1, 1])),
                ("b".into(), tensor(&[0, 1, 0, 1]))
            ])
        ),
        tensor(&[1, 0, 1, 1])
    );
}

#[test]
fn expands_and_slices_with_negative_steps_using_explicit_index_maps() {
    let mut m = model(
        vec![value("a", 1, &[1, 3])],
        vec![
            node("Expand", &["a", "shape"], "expanded"),
            node(
                "Slice",
                &["expanded", "starts", "ends", "axes", "steps"],
                "output",
            ),
        ],
    );
    for (name, values) in [
        ("shape", vec![2, 3]),
        ("starts", vec![1, 2]),
        ("ends", vec![-3, -4]),
        ("axes", vec![0, 1]),
        ("steps", vec![-1, -1]),
    ] {
        m.graph.initializers.insert(name.into(), i64s(&values));
    }
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[1, 3], &[1., 2., 3.]))])
        ),
        f32s(&[2, 3], &[3., 2., 1., 3., 2., 1.])
    );
}
#[test]
fn squeeze_unsqueeze_and_concat_preserve_tensor_order() {
    let mut squeeze = node("Squeeze", &["a"], "squeezed");
    squeeze
        .attributes
        .insert("axes".into(), Attribute::Integers(vec![0]));
    let mut unsqueeze = node("Unsqueeze", &["squeezed"], "restored");
    unsqueeze
        .attributes
        .insert("axes".into(), Attribute::Integers(vec![-1]));
    let mut concat = node("Concat", &["restored", "b"], "output");
    concat
        .attributes
        .insert("axis".into(), Attribute::Integer(1));
    let m = model(
        vec![value("a", 1, &[1, 2]), value("b", 1, &[2, 1])],
        vec![squeeze, unsqueeze, concat],
    );
    assert_eq!(
        run(
            &m,
            BTreeMap::from([
                ("a".into(), f32s(&[1, 2], &[1., 2.])),
                ("b".into(), f32s(&[2, 1], &[3., 4.]))
            ])
        ),
        f32s(&[2, 2], &[1., 3., 2., 4.])
    );
}
#[test]
fn where_relu_and_clip_broadcast_and_preserve_original_nan_relu_behavior() {
    let m = model(
        vec![
            value("condition", 9, &[2, 1]),
            value("a", 1, &[1, 3]),
            value("b", 1, &[1, 3]),
        ],
        vec![
            node("Where", &["condition", "a", "b"], "selected"),
            node("Relu", &["selected"], "output"),
        ],
    );
    let output = run(
        &m,
        BTreeMap::from([
            (
                "condition".into(),
                Tensor {
                    dtype: 9,
                    shape: vec![2, 1],
                    data: vec![1, 0],
                },
            ),
            ("a".into(), f32s(&[1, 3], &[-2., 3., f32::NAN])),
            ("b".into(), f32s(&[1, 3], &[4., -1., 5.])),
        ]),
    );
    assert_eq!(output, f32s(&[2, 3], &[0., 3., 0., 4., 0., 5.]));
    let mut m = model(
        vec![value("a", 1, &[4])],
        vec![node("Clip", &["a", "min", "max"], "output")],
    );
    m.graph.initializers.insert("min".into(), f32s(&[], &[-1.]));
    m.graph.initializers.insert("max".into(), f32s(&[], &[2.]));
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[4], &[-2., 0., 2., 4.]))])
        ),
        f32s(&[4], &[-1., 0., 2., 2.])
    );
}

#[test]
fn boolean_relu_promotes_to_original_integer_output_type() {
    let m = model(
        vec![value("a", 9, &[3])],
        vec![node("Relu", &["a"], "output")],
    );
    let output = run(
        &m,
        BTreeMap::from([(
            "a".into(),
            Tensor {
                dtype: 9,
                shape: vec![3],
                data: vec![0, 1, 1],
            },
        )]),
    );
    assert_eq!(
        output,
        Tensor {
            dtype: 6,
            shape: vec![3],
            data: [0i32, 1, 1]
                .into_iter()
                .flat_map(|v| v.to_le_bytes())
                .collect()
        }
    );
}

#[test]
fn mixed_division_uses_original_numerator_dtype_before_type_promotion() {
    let m = model(
        vec![value("a", 6, &[2]), value("b", 1, &[2])],
        vec![node("Div", &["a", "b"], "output")],
    );
    let ints = Tensor {
        dtype: 6,
        shape: vec![2],
        data: [-7i32, 7]
            .into_iter()
            .flat_map(|v| v.to_le_bytes())
            .collect(),
    };
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), ints), ("b".into(), f32s(&[2], &[3., 3.]))])
        ),
        f32s(&[2], &[-2., 2.])
    );
    let m = model(
        vec![value("a", 9, &[2]), value("b", 6, &[2])],
        vec![node("Div", &["a", "b"], "output")],
    );
    let ints = Tensor {
        dtype: 6,
        shape: vec![2],
        data: [2i32, 4]
            .into_iter()
            .flat_map(|v| v.to_le_bytes())
            .collect(),
    };
    assert_eq!(
        run(
            &m,
            BTreeMap::from([
                (
                    "a".into(),
                    Tensor {
                        dtype: 9,
                        shape: vec![2],
                        data: vec![1, 1]
                    }
                ),
                ("b".into(), ints)
            ])
        ),
        f32s(&[2], &[0.5, 0.25])
    );
}

#[test]
fn clip_attribute_bounds_follow_original_python_scalar_promotion() {
    let mut clip = node("Clip", &["a"], "output");
    clip.attributes.insert("min".into(), Attribute::Float(-0.5));
    clip.attributes.insert("max".into(), Attribute::Float(2.5));
    let m = model(vec![value("a", 6, &[4])], vec![clip]);
    let input = Tensor {
        dtype: 6,
        shape: vec![4],
        data: [-2i32, 0, 2, 4]
            .into_iter()
            .flat_map(|v| v.to_le_bytes())
            .collect(),
    };
    assert_eq!(
        run(&m, BTreeMap::from([("a".into(), input)])),
        f32s(&[4], &[-0.5, 0., 2., 2.5])
    );
}

#[test]
fn reduces_mean_and_l2_with_static_axes_and_original_keepdims() {
    let mut mean = node("ReduceMean", &["a", "axes"], "output");
    mean.attributes
        .insert("keepdims".into(), Attribute::Integer(0));
    let mut m = model(vec![value("a", 1, &[2, 3])], vec![mean]);
    m.graph.initializers.insert("axes".into(), i64s(&[-1]));
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 3], &[1., 2., 3., 4., 5., 6.]))])
        ),
        f32s(&[2], &[2., 5.])
    );
    let mut m = model(
        vec![value("a", 1, &[2, 2])],
        vec![node("ReduceL2", &["a", "axes"], "output")],
    );
    m.graph.initializers.insert("axes".into(), i64s(&[1]));
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 2], &[3., 4., 5., 12.]))])
        ),
        f32s(&[2, 1], &[5., 13.])
    );
}
#[test]
fn lowers_batched_matmul_and_gemm_scaling_bias() {
    let m = model(
        vec![value("a", 1, &[2, 2]), value("b", 1, &[2, 2])],
        vec![node("MatMul", &["a", "b"], "output")],
    );
    let inputs = BTreeMap::from([
        ("a".into(), f32s(&[2, 2], &[1., 2., 3., 4.])),
        ("b".into(), f32s(&[2, 2], &[5., 6., 7., 8.])),
    ]);
    assert_eq!(
        run(&m, inputs.clone()),
        f32s(&[2, 2], &[19., 22., 43., 50.])
    );
    let mut gemm = node("Gemm", &["a", "b", "bias"], "output");
    gemm.attributes
        .insert("alpha".into(), Attribute::Float(0.5));
    gemm.attributes.insert("beta".into(), Attribute::Float(2.));
    let mut m = model(
        vec![value("a", 1, &[2, 2]), value("b", 1, &[2, 2])],
        vec![gemm],
    );
    m.graph
        .initializers
        .insert("bias".into(), f32s(&[2], &[1., 2.]));
    assert_eq!(run(&m, inputs), f32s(&[2, 2], &[11.5, 15., 23.5, 29.]));
}

#[test]
fn repeated_reduction_axes_follow_original_single_axis_semantics() {
    let mut mean = node("ReduceMean", &["a", "axes"], "output");
    mean.attributes
        .insert("keepdims".into(), Attribute::Integer(0));
    let mut m = model(vec![value("a", 1, &[2, 2])], vec![mean]);
    m.graph.initializers.insert("axes".into(), i64s(&[1, 1]));
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 2], &[0., 1., 2., 3.]))])
        ),
        f32s(&[2], &[0.5, 2.5])
    );
}

#[test]
fn lowers_sigmoid_and_both_original_gelu_recipes() {
    for op in ["Sigmoid", "Gelu"] {
        let mut n = node(op, &["a"], "output");
        if op == "Gelu" {
            n.attributes
                .insert("approximate".into(), Attribute::String("none".into()));
        }
        let m = model(vec![value("a", 1, &[1])], vec![n]);
        assert_eq!(
            run(&m, BTreeMap::from([("a".into(), f32s(&[1], &[0.]))])),
            f32s(&[1], &[if op == "Sigmoid" { 0.5 } else { 0. }])
        );
    }
    let mut n = node("Gelu", &["a"], "output");
    n.attributes
        .insert("approximate".into(), Attribute::String("tanh".into()));
    let m = model(vec![value("a", 1, &[1])], vec![n]);
    assert_eq!(
        run(&m, BTreeMap::from([("a".into(), f32s(&[1], &[0.]))])),
        f32s(&[1], &[0.])
    );
}

#[test]
fn gelu_preserves_original_interpreter_typed_constant_precision() {
    for (approximate, expected) in [
        ("none", "505e3abd887622be000000005e62573f0e2dfa3f"),
        ("tanh", "c5f739bd909e22be000000005c58573f4330fa3f"),
    ] {
        let mut n = node("Gelu", &["a"], "output");
        n.attributes
            .insert("approximate".into(), Attribute::String(approximate.into()));
        let m = model(vec![value("a", 1, &[5])], vec![n]);
        let output = run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[5], &[-2., -1., 0., 1., 2.]))]),
        );
        let expected = expected
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(output.data, expected);
    }
}

#[test]
fn gather_and_gather_nd_lower_to_bounded_indirect_scalar_loads() {
    let mut gather = node("Gather", &["a", "indices"], "output");
    gather
        .attributes
        .insert("axis".into(), Attribute::Integer(1));
    let mut m = model(vec![value("a", 1, &[2, 3])], vec![gather]);
    m.graph
        .initializers
        .insert("indices".into(), i64s(&[-1, 0]));
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 3], &[0., 1., 2., 3., 4., 5.]))])
        ),
        f32s(&[2, 2], &[2., 0., 5., 3.])
    );
    let mut m = model(
        vec![value("a", 1, &[2, 3])],
        vec![node("GatherND", &["a", "indices"], "output")],
    );
    m.graph.initializers.insert(
        "indices".into(),
        Tensor {
            dtype: 7,
            shape: vec![2, 2],
            data: [0i64, 1, 1, 2]
                .into_iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        },
    );
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 3], &[0., 1., 2., 3., 4., 5.]))])
        ),
        f32s(&[2], &[1., 5.])
    );
}

#[test]
fn layer_normalization_publishes_all_original_three_outputs_and_softmax_defaults() {
    let mut n = node("LayerNormalization", &["a", "scale", "bias"], "output");
    n.outputs.extend(["mean".into(), "inverse".into()]);
    n.attributes.insert("epsilon".into(), Attribute::Float(0.));
    let mut m = model(vec![value("a", 1, &[2, 2])], vec![n]);
    m.graph
        .initializers
        .insert("scale".into(), f32s(&[2], &[1., 1.]));
    m.graph
        .initializers
        .insert("bias".into(), f32s(&[2], &[0., 0.]));
    m.graph
        .outputs
        .extend([value("mean", 1, &[2, 1]), value("inverse", 1, &[2, 1])]);
    let outputs = LoweredGraph::lower(&m)
        .unwrap()
        .evaluate(&BTreeMap::from([(
            "a".into(),
            f32s(&[2, 2], &[1., 3., 2., 4.]),
        )]))
        .unwrap();
    assert_eq!(outputs["output"], f32s(&[2, 2], &[-1., 1., -1., 1.]));
    assert_eq!(outputs["mean"], f32s(&[2, 1], &[2., 3.]));
    assert_eq!(outputs["inverse"], f32s(&[2, 1], &[1., 1.]));
    let m = model(
        vec![value("a", 1, &[2, 2])],
        vec![node("Softmax", &["a"], "output")],
    );
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 2], &[0., 0., 0., 0.]))])
        ),
        f32s(&[2, 2], &[0.5, 0.5, 0.5, 0.5])
    );
}

#[test]
fn empty_gather_nd_tuple_and_layernorm_axis_edges_preserve_original_behavior() {
    let mut m = model(
        vec![value("a", 1, &[2, 3])],
        vec![node("GatherND", &["a", "indices"], "output")],
    );
    m.graph.initializers.insert(
        "indices".into(),
        Tensor {
            dtype: 7,
            shape: vec![2, 0],
            data: vec![],
        },
    );
    assert_eq!(
        run(
            &m,
            BTreeMap::from([("a".into(), f32s(&[2, 3], &[0., 1., 2., 3., 4., 5.]))])
        ),
        f32s(&[2, 0, 3], &[])
    );
    let mut norm = node("LayerNormalization", &["a", "scale"], "output");
    norm.outputs.push("mean".into());
    norm.attributes
        .insert("axis".into(), Attribute::Integer(-3));
    let mut m = model(vec![value("a", 1, &[2, 2])], vec![norm]);
    m.graph
        .initializers
        .insert("scale".into(), f32s(&[2], &[1., 1.]));
    m.graph.outputs.push(value("mean", 1, &[1, 1]));
    let outputs = LoweredGraph::lower(&m)
        .unwrap()
        .evaluate(&BTreeMap::from([(
            "a".into(),
            f32s(&[2, 2], &[1., 3., 2., 4.]),
        )]))
        .unwrap();
    assert_eq!(outputs["mean"], f32s(&[1, 1], &[2.5]));
    m.graph.nodes[0]
        .attributes
        .insert("axis".into(), Attribute::Integer(2));
    assert!(LoweredGraph::lower(&m).is_err());
}

#[test]
fn convolution_grouped_asymmetric_padding_and_bias() {
    let mut conv = node("Conv", &["a", "w", "b"], "output");
    conv.attributes
        .insert("group".into(), Attribute::Integer(2));
    conv.attributes
        .insert("pads".into(), Attribute::Integers(vec![1, 0]));
    let m = model(
        vec![
            value("a", 1, &[1, 2, 3]),
            value("w", 1, &[2, 1, 2]),
            value("b", 1, &[2]),
        ],
        vec![conv],
    );
    let output = run(
        &m,
        BTreeMap::from([
            ("a".into(), f32s(&[1, 2, 3], &[1., 2., 3., 4., 5., 6.])),
            ("w".into(), f32s(&[2, 1, 2], &[1., 2., 3., 4.])),
            ("b".into(), f32s(&[2], &[10., 20.])),
        ]),
    );
    assert_eq!(output, f32s(&[1, 2, 3], &[12., 15., 18., 36., 52., 59.]));
}

#[test]
fn offline_loader_specializes_symbolic_inputs_and_external_float_bindings() {
    let mut input = value("a", 10, &[2, 3]);
    input.shape = vec![
        ShapeDimension::Symbol("batch".into()),
        ShapeDimension::Fixed(1),
    ];
    let model = model(vec![input], vec![node("Identity", &["a"], "output")]);
    assert!(LoweredGraph::lower(&model).is_err());
    let graph = LoweredGraph::lower_offline(&model).unwrap();
    let output = graph
        .evaluate(&BTreeMap::from([("a".into(), f32s(&[1, 1], &[1.125]))]))
        .unwrap()
        .remove("output")
        .unwrap();
    assert_eq!(output, f32s(&[1, 1], &[1.125]));
    let model = model.clone();
    let mut nonhalf = model;
    nonhalf.graph.inputs[0].dtype = 6;
    let output = LoweredGraph::lower_offline(&nonhalf)
        .unwrap()
        .evaluate(&BTreeMap::from([(
            "a".into(),
            Tensor {
                dtype: 6,
                shape: vec![1, 1],
                data: 7i32.to_le_bytes().to_vec(),
            },
        )]))
        .unwrap()
        .remove("output")
        .unwrap();
    assert_eq!(output, f32s(&[1, 1], &[7.]));
}
