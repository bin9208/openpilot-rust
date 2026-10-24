//! Closed two-Gemm/bias/Relu/residual pipelines, from independent original
//! final-f32 and internal packed-half captures. Selection owns exact plans only.
use super::{graph::PipelineCall, MatrixLayout};
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
    schedule::{schedule_plans, Schedule},
    Attribute, Model, Node,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct GemmClosure {
    pub root: usize,
    pub output: usize,
    pub first: MatrixLayout,
    pub second: MatrixLayout,
    pub weights: [usize; 2],
    pub biases: [usize; 2],
    pub relu_half: bool,
    pub owned: BTreeSet<usize>,
}
pub(super) fn producer<'a>(model: &'a Model, name: &str, op: &str) -> Option<&'a Node> {
    model
        .graph
        .nodes
        .iter()
        .find(|n| n.op == op && n.outputs.len() == 1 && n.outputs[0] == name)
}
pub(super) fn gemm(node: &Node) -> bool {
    node.inputs.len() == 3
        && node.outputs.len() == 1
        && node
            .attributes
            .iter()
            .all(|(key, value)| match key.as_str() {
                "transA" => matches!(value, Attribute::Integer(0)),
                "transB" => matches!(value, Attribute::Integer(1)),
                "alpha" | "beta" => {
                    matches!(value, Attribute::Float(v) if *v == 1.0)
                        || matches!(value, Attribute::Integer(1))
                }
                _ => false,
            })
        && matches!(node.attributes.get("transB"), Some(Attribute::Integer(1)))
}
pub(super) fn constant(graph: &LoweredGraph, id: usize, shape: &[usize]) -> bool {
    graph.plans[id].ty.dtype == 10
        && graph.plans[id].ty.shape == shape
        && matches!(graph.plans[id].expression, ScalarExpr::Constant(_))
}
/// Accept only a source-node-closed chain. Extra readers/held internal values
/// change original materialization, so they do not acquire this selection.
pub fn closures(graph: &LoweredGraph, model: &Model) -> Result<Vec<GemmClosure>, String> {
    let schedule = schedule_plans(graph, "CPU", true)?;
    let held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    let mut result = Vec::new();
    for add in model.graph.nodes.iter().filter(|n| {
        n.op == "Add" && n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty()
    }) {
        let Some(second_node) = producer(model, &add.inputs[1], "Gemm").filter(|n| gemm(n)) else {
            continue;
        };
        let Some(relu) = producer(model, &second_node.inputs[0], "Relu")
            .filter(|n| n.inputs.len() == 1 && n.attributes.is_empty())
        else {
            continue;
        };
        let Some(first_node) = producer(model, &relu.inputs[0], "Gemm").filter(|n| gemm(n)) else {
            continue;
        };
        if first_node.inputs[0] != add.inputs[0] {
            continue;
        }
        let root = graph.names[&add.inputs[0]];
        let add_output = graph.names[&add.outputs[0]];
        if !schedule.live.contains(&add_output) {
            continue;
        }
        if model.graph.nodes.iter().any(|node| {
            node.inputs.contains(&add.inputs[0])
                && node.outputs != first_node.outputs
                && node.outputs != add.outputs
                && node.outputs.iter().any(|name| {
                    graph
                        .names
                        .get(name)
                        .is_some_and(|id| schedule.live.contains(id))
                })
        }) {
            // Captured sharing is exactly the first Gemm, reversed Add and
            // optional raw root binding. A new live ALU/matrix user is separate.
            continue;
        }
        let ty = &graph.plans[root].ty;
        if ty.dtype != 1
            || ty.shape.len() != 2
            || ty.shape[0] != 1
            || matches!(
                graph.plans[root].expression,
                ScalarExpr::Input(_)
                    | ScalarExpr::Constant(_)
                    | ScalarExpr::Load { .. }
                    | ScalarExpr::Cast { .. }
            )
        {
            continue;
        }
        let weights = [
            graph.names[&first_node.inputs[1]],
            graph.names[&second_node.inputs[1]],
        ];
        let biases = [
            graph.names[&first_node.inputs[2]],
            graph.names[&second_node.inputs[2]],
        ];
        let n = ty.shape[1];
        let w1 = &graph.plans[weights[0]].ty.shape;
        if w1.len() != 2 || w1[1] != n {
            continue;
        }
        let k = w1[0];
        if !constant(graph, weights[0], &[k, n])
            || !constant(graph, weights[1], &[n, k])
            || !constant(graph, biases[0], &[k])
            || !constant(graph, biases[1], &[n])
        {
            continue;
        }
        let (Ok(first), Ok(second)) = (
            MatrixLayout::new(&[1, n], &[n, k]),
            MatrixLayout::new(&[1, k], &[k, n]),
        ) else {
            continue;
        };
        let relu_out = graph.names[&relu.outputs[0]];
        if graph.plans[relu_out].ty.dtype != 1
            || graph.plans[relu_out].ty.shape != [1, k]
            || graph.plans[add_output].ty != *ty
        {
            continue;
        }
        let (output, relu_half) = if held.contains(&add_output) {
            if model.graph.nodes.iter().any(|node| {
                node.inputs.contains(&add.outputs[0])
                    && node.outputs.iter().any(|name| {
                        graph
                            .names
                            .get(name)
                            .is_some_and(|id| schedule.live.contains(id))
                    })
            }) {
                continue;
            }
            (add_output, false)
        } else {
            let users = model
                .graph
                .nodes
                .iter()
                .filter(|n| n.inputs.contains(&add.outputs[0]))
                .collect::<Vec<_>>();
            let [next] = users.as_slice() else { continue };
            if next.op != "Relu"
                || next.inputs.len() != 1
                || next.outputs.len() != 1
                || !next.attributes.is_empty()
            {
                continue;
            }
            let output = graph.names[&next.outputs[0]];
            // A held Relu has another original storage profile; this unit
            // proves the unheld half materialization before a matrix consumer.
            if held.contains(&output) || graph.plans[output].ty != *ty {
                continue;
            }
            let consumers = model
                .graph
                .nodes
                .iter()
                .filter(|n| n.inputs.contains(&next.outputs[0]))
                .collect::<Vec<_>>();
            let [consumer] = consumers.as_slice() else {
                continue;
            };
            if consumer.op != "MatMul"
                || consumer.inputs.len() != 2
                || consumer.inputs[0] != next.outputs[0]
                || consumer.outputs.len() != 1
                || !consumer.attributes.is_empty()
                || !held.contains(&graph.names[&consumer.outputs[0]])
            {
                continue;
            }
            (output, true)
        };
        let expected_names = [
            graph.names[&first_node.outputs[0]],
            relu_out,
            graph.names[&second_node.outputs[0]],
            add_output,
            output,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        // The lowerer adds transpose and dot/bias materialization temporaries.
        // Walk their actual scheduled dependencies, stopping at root/constants.
        let calls = schedule
            .calls
            .iter()
            .map(|c| (c.buffers[0], &c.buffers[1..]))
            .collect::<BTreeMap<_, _>>();
        let mut owned = BTreeSet::new();
        let mut pending = vec![output];
        let mut supported = true;
        while let Some(id) = pending.pop() {
            if id == root
                || matches!(graph.plans[id].expression, ScalarExpr::Constant(_))
                || !owned.insert(id)
            {
                continue;
            }
            if graph.names.values().any(|v| *v == id) && !expected_names.contains(&id) {
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
            || !expected_names.is_subset(&owned)
            || owned.iter().any(|id| *id != output && held.contains(id))
            || schedule.calls.iter().any(|c| {
                !owned.contains(&c.buffers[0])
                    && c.buffers[1..]
                        .iter()
                        .any(|id| *id != output && owned.contains(id))
            })
            || result
                .iter()
                .any(|c: &GemmClosure| !c.owned.is_disjoint(&owned))
        {
            continue;
        }
        result.push(GemmClosure {
            root,
            output,
            first,
            second,
            weights,
            biases,
            relu_half,
            owned,
        });
    }
    Ok(result)
}

#[derive(Clone, Copy)]
pub(super) enum StageKind {
    Pack,
    Producer,
    Residual,
    Decode,
    Terminal,
    Norm,
    Divide,
    PackedDivide,
    Heads,
}
pub(super) struct Staged {
    pub output: usize,
    pub layout: MatrixLayout,
    pub kind: StageKind,
    pub relu_half: bool,
}
pub(super) fn buffer(schedule: &mut Schedule, bytes: usize, held: bool) -> usize {
    let id = schedule.buffers.len();
    schedule.buffers.push(BufferSpec {
        device: "CPU".into(),
        bytes,
        held,
        supports_offset: true,
    });
    id
}
pub(super) fn pack_weight(
    graph: &LoweredGraph,
    id: usize,
    layout: &MatrixLayout,
) -> Result<Vec<u8>, String> {
    let ScalarExpr::Constant(raw) = &graph.plans[id].expression else {
        return Err("selected Gemm initializer changed".into());
    };
    let (k, n) = (layout.input_channels, layout.output_channels);
    if raw.len() != k * n * 2 {
        return Err("selected Gemm initializer byte count changed".into());
    }
    let mut logical = vec![0; raw.len()];
    for i in 0..k {
        for o in 0..n {
            logical[(i * n + o) * 2..(i * n + o + 1) * 2]
                .copy_from_slice(&raw[(o * k + i) * 2..(o * k + i + 1) * 2]);
        }
    }
    let mut packed = vec![0; layout.weight_elements * 2];
    layout.pack_weights_into(&logical, &mut packed)?;
    Ok(packed)
}
/// Insert only after the complete captured closure's ownership has been proved.
/// All private buffers have visible producer/consumer calls for lifetime planning.
pub(super) fn insert(
    selected: &[GemmClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<Vec<Staged>, String> {
    let mut staged = Vec::new();
    for c in selected {
        let root = buffer(schedule, c.first.input_elements * 2, false);
        let hidden = buffer(schedule, c.second.input_elements * 2, false);
        scratch.extend([root, hidden]);
        let mut weights = Vec::new();
        for (id, layout) in c.weights.iter().zip([&c.first, &c.second]) {
            let raw = pack_weight(graph, *id, layout)?;
            let packed = buffer(schedule, raw.len(), true);
            initialized.insert(packed, raw);
            weights.push(packed);
        }
        let final_output = if c.relu_half {
            let id = buffer(schedule, c.first.input_elements * 2, false);
            scratch.insert(id);
            id
        } else {
            c.output
        };
        let mut pipeline = vec![
            Call {
                copy: false,
                buffers: vec![root, c.root],
            },
            Call {
                copy: false,
                buffers: vec![hidden, root, weights[0], c.biases[0]],
            },
            Call {
                copy: false,
                buffers: vec![final_output, root, hidden, weights[1], c.biases[1]],
            },
        ];
        staged.push(Staged {
            output: root,
            layout: c.first.clone(),
            kind: StageKind::Pack,
            relu_half: false,
        });
        staged.push(Staged {
            output: hidden,
            layout: c.first.clone(),
            kind: StageKind::Producer,
            relu_half: false,
        });
        staged.push(Staged {
            output: final_output,
            layout: c.second.clone(),
            kind: StageKind::Residual,
            relu_half: c.relu_half,
        });
        if c.relu_half {
            pipeline.push(Call {
                copy: false,
                buffers: vec![c.output, final_output],
            });
            staged.push(Staged {
                output: c.output,
                layout: c.second.clone(),
                kind: StageKind::Decode,
                relu_half: false,
            });
        }
        let old = schedule
            .calls
            .iter()
            .position(|call| call.buffers[0] == c.output)
            .ok_or("selected Gemm output call disappeared")?;
        // Keep unrelated calls and root producer order. Internal values have no
        // outside consumers, so insert the fused pipeline at its final output.
        let at = schedule.calls[..old]
            .iter()
            .filter(|call| !c.owned.contains(&call.buffers[0]))
            .count();
        schedule
            .calls
            .retain(|call| !c.owned.contains(&call.buffers[0]));
        schedule.calls.splice(at..at, pipeline);
    }
    Ok(staged)
}
pub(super) fn render(
    staged: &[Staged],
    schedule: &Schedule,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let mut calls = BTreeMap::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        let Some(s) = staged.iter().find(|s| s.output == call.buffers[0]) else {
            continue;
        };
        let entry = format!("op_kernel_{index}");
        let function = format!("native_gemm_{index}");
        let artifact = match s.kind {
            StageKind::Heads => {
                let widths = call.buffers[3..]
                    .iter()
                    .step_by(2)
                    .map(|&id| schedule.buffers[id].bytes / 2)
                    .collect::<Vec<_>>();
                super::branch::artifact(s.layout.input_channels, &widths, &function, &entry)?
            }
            StageKind::Pack | StageKind::Decode => {
                let a = if matches!(s.kind, StageKind::Pack) {
                    s.layout.input_artifact(&entry)?
                } else {
                    s.layout.packed_half_output_artifact(&entry)?
                };
                PipelineCall {
                    source: a.source,
                    bytes: a.bytes.to_vec(),
                    workers: a.workers,
                    scalars: a.scalars,
                }
            }
            StageKind::Producer => {
                let a = s.layout.biased_relu_artifact(&function, &entry, 16)?;
                PipelineCall {
                    source: a.source,
                    bytes: a.bytes.to_vec(),
                    workers: a.workers,
                    scalars: a.scalars,
                }
            }
            StageKind::Norm | StageKind::Divide | StageKind::PackedDivide => {
                let a = if matches!(s.kind, StageKind::Norm) {
                    crate::compiler_backend_cpu::normalization::RowNorm::new(
                        s.layout.output_channels,
                    )?
                    .artifact(&function, &entry)?
                } else if matches!(s.kind, StageKind::PackedDivide) {
                    crate::compiler_backend_cpu::normalization::packed_division_artifact(
                        s.layout.output_channels,
                        &function,
                        &entry,
                    )?
                } else {
                    crate::compiler_backend_cpu::normalization::division_artifact(
                        s.layout.output_channels,
                        &function,
                        &entry,
                    )?
                };
                PipelineCall {
                    source: a.source,
                    bytes: a.bytes.to_vec(),
                    workers: a.workers,
                    scalars: a.scalars,
                }
            }
            StageKind::Terminal => {
                let a = s.layout.biased_artifact(&function, &entry, 16)?;
                PipelineCall {
                    source: a.source,
                    bytes: a.bytes.to_vec(),
                    workers: a.workers,
                    scalars: a.scalars,
                }
            }
            StageKind::Residual => {
                let a = s
                    .layout
                    .biased_residual_artifact(&function, &entry, 16, s.relu_half)?;
                PipelineCall {
                    source: a.source,
                    bytes: a.bytes.to_vec(),
                    workers: a.workers,
                    scalars: a.scalars,
                }
            }
        };
        if artifact.bytes.len() != call.buffers.len()
            || call
                .buffers
                .iter()
                .zip(&artifact.bytes)
                .any(|(id, bytes)| schedule.buffers[*id].bytes != *bytes)
        {
            return Err("Gemm stage typed storage differs from scheduled ABI".into());
        }
        calls.insert(index, artifact);
    }
    if calls.len() != staged.len() {
        return Err("Gemm staged call lost during matrix planning".into());
    }
    Ok(calls)
}
