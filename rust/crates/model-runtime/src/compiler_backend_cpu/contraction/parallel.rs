//! Closed independent depthwise producers sharing one packed input. This is
//! distinct from a Conv consuming its own residual or a quantized skip edge.
mod kernel;
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
    schedule::{schedule_plans, Schedule},
    Attribute, Model, Node,
};
use kernel::{Layout, Stage};
use std::collections::{BTreeMap, BTreeSet};
type PipelineCall = crate::compiler_backend_cpu::matrix::graph::PipelineCall;

pub(crate) struct Closure {
    layout: Layout,
    input: usize,
    pre: usize,
    weights: [usize; 2],
    bias: usize,
    gamma: usize,
    output: usize,
    held_pre: bool,
    pub owned: BTreeSet<usize>,
}
fn producer<'a>(model: &'a Model, name: &str, op: &str) -> Option<&'a Node> {
    model
        .graph
        .nodes
        .iter()
        .find(|n| n.op == op && n.outputs.len() == 1 && n.outputs[0] == name)
}
fn constant(graph: &LoweredGraph, id: usize, shape: &[usize]) -> bool {
    let p = &graph.plans[id];
    p.ty.dtype == 10 && p.ty.shape == shape && matches!(p.expression, ScalarExpr::Constant(_))
}
fn depthwise(node: &Node, channels: usize) -> bool {
    node.inputs.len() == 3
        && node
            .attributes
            .iter()
            .all(|(name, value)| match (name.as_str(), value) {
                ("group", Attribute::Integer(group)) => {
                    usize::try_from(*group).ok() == Some(channels)
                }
                ("strides" | "dilations" | "kernel_shape", Attribute::Integers(v)) => v == &[1, 1],
                ("pads", Attribute::Integers(v)) => v == &[0, 0, 0, 0],
                ("auto_pad", Attribute::String(v)) => matches!(v.as_str(), "NOTSET" | "VALID"),
                _ => false,
            })
        && matches!(node.attributes.get("group"), Some(Attribute::Integer(group)) if usize::try_from(*group).ok() == Some(channels))
}
pub(crate) fn closures(graph: &LoweredGraph, model: &Model) -> Result<Vec<Closure>, String> {
    let schedule = schedule_plans(graph, "CPU", true)?;
    let held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    let calls = schedule
        .calls
        .iter()
        .map(|c| (c.buffers[0], &c.buffers[1..]))
        .collect::<BTreeMap<_, _>>();
    let mut result: Vec<Closure> = Vec::new();
    for add in model.graph.nodes.iter().filter(|n| {
        n.op == "Add" && n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty()
    }) {
        let output = graph.names[&add.outputs[0]];
        if !held.contains(&output) {
            continue;
        }
        let Some(mul) = producer(model, &add.inputs[0], "Mul")
            .filter(|n| n.inputs.len() == 2 && n.attributes.is_empty())
        else {
            continue;
        };
        let Some(pre) = producer(model, &mul.inputs[0], "Conv") else {
            continue;
        };
        let Some(skip) = producer(model, &add.inputs[1], "Conv") else {
            continue;
        };
        if pre == skip
            || pre.inputs.len() != 3
            || skip.inputs.len() != 3
            || pre.inputs[0] != skip.inputs[0]
        {
            continue;
        }
        let input = graph.names[&pre.inputs[0]];
        let ty = &graph.plans[input].ty;
        if ty.dtype != 1
            || ty.shape.len() != 4
            || ty.shape[0] != 1
            || !matches!(graph.plans[input].expression, ScalarExpr::Input(_))
        {
            continue;
        }
        let Ok(layout) = Layout::new([ty.shape[1], ty.shape[2], ty.shape[3]]) else {
            continue;
        };
        let channels = ty.shape[1];
        if !depthwise(pre, channels) || !depthwise(skip, channels) {
            continue;
        }
        let pre_id = graph.names[&pre.outputs[0]];
        let skip_id = graph.names[&skip.outputs[0]];
        let product = graph.names[&mul.outputs[0]];
        let weights = [graph.names[&pre.inputs[1]], graph.names[&skip.inputs[1]]];
        let bias = graph.names[&pre.inputs[2]];
        let gamma = graph.names[&mul.inputs[1]];
        if graph.names[&skip.inputs[2]] != bias
            || weights[0] == weights[1]
            || bias == gamma
            || !weights
                .iter()
                .all(|&id| constant(graph, id, &[channels, 1, 1, 1]))
            || !constant(graph, bias, &[channels])
            || !constant(graph, gamma, &[channels, 1, 1])
            || [pre_id, skip_id, product, output]
                .iter()
                .any(|&id| graph.plans[id].ty != *ty)
        {
            continue;
        }
        let named = BTreeSet::from([pre_id, skip_id, product, output]);
        let mut owned = BTreeSet::new();
        let mut pending = vec![output];
        let mut supported = true;
        while let Some(id) = pending.pop() {
            if id == input
                || matches!(graph.plans[id].expression, ScalarExpr::Constant(_))
                || !owned.insert(id)
            {
                continue;
            }
            if graph.names.values().any(|&v| v == id) && !named.contains(&id) {
                supported = false;
                break;
            }
            let Some(inputs) = calls.get(&id) else {
                supported = false;
                break;
            };
            pending.extend(inputs.iter().copied());
        }
        if !supported
            || !named.is_subset(&owned)
            || owned
                .iter()
                .any(|id| held.contains(id) && *id != output && *id != pre_id)
            || schedule.calls.iter().any(|c| {
                !owned.contains(&c.buffers[0]) && c.buffers[1..].iter().any(|id| owned.contains(id))
            })
            || result.iter().any(|c| !c.owned.is_disjoint(&owned))
        {
            continue;
        }
        result.push(Closure {
            layout,
            input,
            pre: pre_id,
            weights,
            bias,
            gamma,
            output,
            held_pre: held.contains(&pre_id),
            owned,
        });
    }
    Ok(result)
}
pub(crate) struct Staged {
    layout: Layout,
    output: usize,
    stage: Stage,
}
pub(crate) fn insert(
    selected: &[Closure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
) -> Result<Vec<Staged>, String> {
    let mut staged = Vec::new();
    for c in selected {
        let mut buffer = |bytes, held| {
            let id = schedule.buffers.len();
            schedule.buffers.push(BufferSpec {
                device: "CPU".into(),
                bytes,
                held,
                supports_offset: true,
            });
            id
        };
        let packed = buffer(c.layout.input_bytes(), false);
        let mut weights = Vec::new();
        for &id in &c.weights {
            let ScalarExpr::Constant(raw) = &graph.plans[id].expression else {
                return Err("parallel Conv source weight changed after selection".into());
            };
            let raw = c.layout.pack_weights(raw)?;
            let id = buffer(raw.len(), true);
            initialized.insert(id, raw);
            weights.push(id);
        }
        let mut pipeline = vec![Call {
            copy: false,
            buffers: vec![packed, c.input],
        }];
        staged.push(Staged {
            layout: c.layout.clone(),
            output: packed,
            stage: Stage::Pack,
        });
        if c.held_pre {
            pipeline.push(Call {
                copy: false,
                buffers: vec![c.pre, packed, weights[0], c.bias],
            });
            staged.push(Staged {
                layout: c.layout.clone(),
                output: c.pre,
                stage: Stage::Producer,
            });
        }
        pipeline.push(Call {
            copy: false,
            buffers: if c.held_pre {
                vec![c.output, c.pre, c.gamma, packed, c.bias, weights[1]]
            } else {
                vec![c.output, packed, weights[0], c.bias, c.gamma, weights[1]]
            },
        });
        staged.push(Staged {
            layout: c.layout.clone(),
            output: c.output,
            stage: if c.held_pre {
                Stage::Held
            } else {
                Stage::Fused
            },
        });
        let at = schedule
            .calls
            .iter()
            .position(|call| c.owned.contains(&call.buffers[0]))
            .ok_or("parallel Conv calls disappeared")?;
        schedule
            .calls
            .retain(|call| !c.owned.contains(&call.buffers[0]));
        schedule.calls.splice(at..at, pipeline);
    }
    Ok(staged)
}
pub(crate) fn scheduled(
    staged: &[Staged],
    schedule: &Schedule,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let mut result = BTreeMap::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        if let Some(c) = staged.iter().find(|c| c.output == call.buffers[0]) {
            let artifact = c.layout.artifact(
                c.stage,
                &format!("native_parallel_{index}"),
                &format!("op_kernel_{index}"),
            )?;
            if artifact.bytes
                != call
                    .buffers
                    .iter()
                    .map(|&id| schedule.buffers[id].bytes)
                    .collect::<Vec<_>>()
            {
                return Err("parallel Conv staged capacities differ from typed ABI".into());
            }
            result.insert(index, artifact);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_parallel_profiles_have_closed_ownership() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/compiler/original-contraction-target18.json"
        )))
        .unwrap();
        let mut positives = 0;
        let mut negatives = 0;
        for case in fixture["cases"].as_array().unwrap() {
            let raw = case["onnx_hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let model = Model::parse(&raw).unwrap();
            let graph = LoweredGraph::lower_offline(&model).unwrap();
            let selected = closures(&graph, &model).unwrap();
            if case["name"].as_str().unwrap().starts_with("direct-") {
                assert!(selected.is_empty());
                continue;
            }
            assert_eq!(selected.len(), 1, "{}", case["name"]);
            let c = &selected[0];
            assert_eq!(c.held_pre, case["held"].as_bool().unwrap());
            assert!(c.owned.contains(&c.pre) && c.owned.contains(&c.output));
            assert!(!c.owned.contains(&c.input));
            assert!(c
                .owned
                .iter()
                .all(|&id| !matches!(graph.plans[id].expression, ScalarExpr::Constant(_))));
            positives += 1;
            let add = producer(&model, "output", "Add").unwrap();
            let mul = producer(&model, &add.inputs[0], "Mul").unwrap();
            let conv = producer(&model, &mul.inputs[0], "Conv").unwrap();
            let skip = producer(&model, &add.inputs[1], "Conv").unwrap();
            let rejected = |m: Model| {
                let g = LoweredGraph::lower_offline(&m).unwrap();
                assert!(closures(&g, &m).unwrap().is_empty(), "{}", case["name"]);
            };
            for name in [&skip.outputs[0], &mul.outputs[0]] {
                let mut m = model.clone();
                let mut binding = m.graph.outputs[0].clone();
                binding.name = name.clone();
                m.graph.outputs.push(binding);
                rejected(m);
                negatives += 1;
            }
            let mut m = model.clone();
            let mut outside = mul.clone();
            outside.outputs = vec!["outside".into()];
            m.graph.nodes.push(outside);
            let mut binding = m.graph.outputs[0].clone();
            binding.name = "outside".into();
            m.graph.outputs.push(binding);
            rejected(m);
            let mut m = model.clone();
            let target = m
                .graph
                .nodes
                .iter_mut()
                .find(|n| n.outputs == add.outputs && n.op == add.op)
                .unwrap();
            target.inputs.swap(0, 1);
            rejected(m);
            let mut m = model.clone();
            let bias = m.graph.initializers[&conv.inputs[2]].clone();
            m.graph.initializers.insert("second_bias".into(), bias);
            m.graph
                .nodes
                .iter_mut()
                .find(|n| n.outputs == skip.outputs && n.op == skip.op)
                .unwrap()
                .inputs[2] = "second_bias".into();
            rejected(m);
            let mut m = model.clone();
            m.graph
                .nodes
                .iter_mut()
                .find(|n| n.outputs == skip.outputs && n.op == skip.op)
                .unwrap()
                .inputs[0] = conv.outputs[0].clone();
            rejected(m);
            negatives += 4;
        }
        assert_eq!(positives, 24);
        assert_eq!(negatives, 144);
    }
}
