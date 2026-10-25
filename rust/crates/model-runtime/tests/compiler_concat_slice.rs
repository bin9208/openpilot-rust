//! Exact whole-input Concat/Slice cancellation must prune unused head branches.
use openpilot_model_runtime::compiler_onnx::{
    lower::LoweredGraph, schedule::schedule_plans, Attribute, Graph, Model, Node, ShapeDimension,
    Tensor, Value,
};
use std::collections::BTreeMap;
fn model(shape: &[usize], dim: usize, selected: usize, negative: bool, held_concat: bool) -> Model {
    let widths = [3usize, 4, 6];
    let binding = |name: &str, shape: Vec<usize>| Value {
        name: name.into(),
        dtype: 1,
        shape: shape
            .into_iter()
            .map(|n| ShapeDimension::Fixed(n as i64))
            .collect(),
        optional: false,
        sequence: false,
    };
    let node = |op: &str,
                inputs: Vec<String>,
                output: &str,
                attributes: BTreeMap<String, Attribute>| Node {
        name: String::new(),
        op: op.into(),
        domain: String::new(),
        opset: 20,
        inputs,
        outputs: vec![output.into()],
        attributes,
    };
    let mut graph = Graph::default();
    for (name, width) in ["a", "b", "c"].into_iter().zip(widths) {
        let mut s = shape.to_vec();
        s[dim] = width;
        graph.inputs.push(binding(name, s));
    }
    graph.nodes.push(node(
        "Concat",
        vec!["a".into(), "b".into(), "c".into()],
        "joined",
        BTreeMap::from([("axis".into(), Attribute::Integer(dim as i64))]),
    ));
    let offset = widths[..selected].iter().sum::<usize>();
    let total = widths.iter().sum::<usize>();
    let start = offset as i64 - if negative { total as i64 } else { 0 };
    let end = (offset + widths[selected]) as i64
        - if negative && selected != 2 {
            total as i64
        } else {
            0
        };
    for (name, value) in [
        ("starts", start),
        ("ends", end),
        (
            "axes",
            if negative {
                dim as i64 - shape.len() as i64
            } else {
                dim as i64
            },
        ),
        ("steps", 1),
    ] {
        graph.initializers.insert(
            name.into(),
            Tensor {
                dtype: 7,
                shape: vec![1],
                data: value.to_le_bytes().to_vec(),
            },
        );
    }
    graph.nodes.push(node(
        "Slice",
        ["joined", "starts", "ends", "axes", "steps"]
            .map(str::to_owned)
            .to_vec(),
        "output",
        BTreeMap::new(),
    ));
    let mut result = shape.to_vec();
    result[dim] = widths[selected];
    graph.outputs.push(binding("output", result));
    if held_concat {
        let mut s = shape.to_vec();
        s[dim] = total;
        graph.outputs.push(binding("joined", s));
    }
    Model {
        graph,
        opsets: BTreeMap::from([(String::new(), 20)]),
        metadata_properties: BTreeMap::new(),
    }
}
#[test]
fn whole_concat_input_slice_is_exact_identity_and_prunes_unselected_inputs() {
    for (shape, dim) in [(vec![2, 1], 1), (vec![2, 1, 3], 1), (vec![1, 2, 3], 0)] {
        for selected in 0..3 {
            for negative in [false, true] {
                for held in [false, true] {
                    let model = model(&shape, dim, selected, negative, held);
                    let graph = LoweredGraph::lower(&model).unwrap();
                    let name = ["a", "b", "c"][selected];
                    assert_eq!(
                        graph.names["output"], graph.names[name],
                        "shape={shape:?} selected={selected} negative={negative} held={held}"
                    );
                    let schedule = schedule_plans(&graph, "CPU", true).unwrap();
                    for other in ["a", "b", "c"] {
                        assert_eq!(
                            schedule.live.contains(&graph.names[other]),
                            held || other == name
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn partial_or_strided_slice_does_not_acquire_whole_input_alias() {
    for (start, end, step) in [(3i64, 6i64, 1i64), (3, 7, 2), (2, 7, 1), (3, 8, 1)] {
        let mut model = model(&[2, 1], 1, 1, false, false);
        for (name, value) in [("starts", start), ("ends", end), ("steps", step)] {
            model.graph.initializers.get_mut(name).unwrap().data = value.to_le_bytes().to_vec();
        }
        let graph = LoweredGraph::lower(&model).unwrap();
        assert_ne!(graph.names["output"], graph.names["b"]);
    }
}

#[test]
fn concat_promotion_is_not_removed_from_selected_half_input() {
    let mut model = model(&[2, 1], 1, 1, false, false);
    model.graph.inputs[1].dtype = 10;
    let graph = LoweredGraph::lower(&model).unwrap();
    assert_ne!(graph.names["output"], graph.names["b"]);
    assert_eq!(graph.plans[graph.names["output"]].ty.dtype, 1);
}

#[test]
fn consecutive_constant_gather_nd_window_lowers_to_a_bounded_static_load() {
    use openpilot_model_runtime::compiler_onnx::{
        lower::ScalarExpr, render::render_cpu_elementwise,
    };
    for indices in [[-3i64, -2, -1], [1, 2, 3], [0, 1, 2]] {
        let mut m = model(&[2, 1], 1, 1, false, false);
        m.graph.inputs.truncate(1);
        m.graph.inputs[0].shape = [5, 2, 3].into_iter().map(ShapeDimension::Fixed).collect();
        m.graph.initializers = BTreeMap::from([(
            "indices".into(),
            Tensor {
                dtype: 7,
                shape: vec![3, 1],
                data: indices.into_iter().flat_map(i64::to_le_bytes).collect(),
            },
        )]);
        m.graph.nodes.truncate(1);
        m.graph.nodes[0].op = "GatherND".into();
        m.graph.nodes[0].inputs = vec!["a".into(), "indices".into()];
        m.graph.nodes[0].outputs = vec!["output".into()];
        m.graph.nodes[0].attributes.clear();
        m.graph.outputs[0].shape = [3, 2, 3].into_iter().map(ShapeDimension::Fixed).collect();
        let g = LoweredGraph::lower(&m).unwrap();
        let id = g.names["output"];
        assert!(
            matches!(g.plans[id].expression, ScalarExpr::Load { .. }),
            "{indices:?}"
        );
        assert_eq!(
            render_cpu_elementwise(&g, id, "gather_window")
                .unwrap()
                .inputs,
            vec![g.names["a"]]
        );
        let values = (0..30u32).flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        let output = g
            .evaluate(&BTreeMap::from([(
                "a".into(),
                Tensor {
                    dtype: 1,
                    shape: vec![5, 2, 3],
                    data: values.clone(),
                },
            )]))
            .unwrap();
        let first = if indices[0] < 0 {
            5 + indices[0]
        } else {
            indices[0]
        } as usize;
        assert_eq!(
            output["output"].data,
            values[first * 6 * 4..(first + 3) * 6 * 4]
        );
    }
}
