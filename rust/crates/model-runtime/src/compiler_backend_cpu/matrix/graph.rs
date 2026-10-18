//! Explicit matrix pipeline planning. Admission is a separate caller decision.
use super::MatrixLayout;
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
    schedule::Schedule,
    Model,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct MatrixClosure {
    pub layout: MatrixLayout,
    pub input: usize,
    pub weights: usize,
    pub output: usize,
    pub reduction: usize,
    pub residual: bool,
}
/// Discover final-output matrices and the independently captured direct
/// matrix-plus-packed-input residual. Other epilogues retain their fences.
pub fn closures(graph: &LoweredGraph, model: &Model) -> Result<Vec<MatrixClosure>, String> {
    let schedule = crate::compiler_onnx::schedule::schedule_plans(graph, "CPU", true)?;
    let live = &schedule.live;
    let mut selected = vec![];
    for node in model.graph.nodes.iter().filter(|n| {
        n.op == "MatMul" && n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty()
    }) {
        let Some((&input, &weights, &output)) = graph
            .names
            .get(&node.inputs[0])
            .zip(graph.names.get(&node.inputs[1]))
            .zip(graph.names.get(&node.outputs[0]))
            .map(|((a, b), c)| (a, b, c))
        else {
            continue;
        };
        if !live.contains(&output) || !graph.outputs.values().any(|id| *id == output) {
            continue;
        }
        let (x, w) = (&graph.plans[input].ty, &graph.plans[weights].ty);
        if x.dtype != 1
            || w.dtype != 10
            || !(2..=3).contains(&x.shape.len())
            || matches!(graph.plans[input].expression, ScalarExpr::Constant(_))
            || !matches!(graph.plans[weights].expression, ScalarExpr::Constant(_))
        {
            continue;
        }
        let Ok(layout) = MatrixLayout::new(&x.shape, &w.shape) else {
            continue;
        };
        selected.push(MatrixClosure {
            layout,
            input,
            weights,
            output,
            reduction: output,
            residual: false,
        });
    }
    // Independently proved final-only square matrix plus its own packed input.
    // Held raw input is allowed; a held/intermediately reused matrix result is
    // a different original materialization and must retain its admission fence.
    for add in model.graph.nodes.iter().filter(|n| {
        n.op == "Add" && n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty()
    }) {
        let Some(&output) = graph.names.get(&add.outputs[0]) else {
            continue;
        };
        if !graph.outputs.values().any(|id| *id == output) {
            continue;
        }
        let Some(matrix) = model.graph.nodes.iter().find(|n| {
            n.op == "MatMul"
                && n.inputs.len() == 2
                && n.outputs.len() == 1
                && n.outputs[0] == add.inputs[0]
                && n.attributes.is_empty()
                && n.inputs[0] == add.inputs[1]
        }) else {
            continue;
        };
        let (input, weights, reduction) = (
            graph.names[&matrix.inputs[0]],
            graph.names[&matrix.inputs[1]],
            graph.names[&matrix.outputs[0]],
        );
        let (x, w) = (&graph.plans[input], &graph.plans[weights]);
        if x.ty.dtype != 1
            || x.ty.shape.len() != 2
            || x.ty.shape[0] != 1
            || w.ty.dtype != 1
            || matches!(
                x.expression,
                ScalarExpr::Input(_)
                    | ScalarExpr::Constant(_)
                    | ScalarExpr::Load { .. }
                    | ScalarExpr::Cast { .. }
            )
            || !matches!(w.expression, ScalarExpr::Constant(_))
            || graph.outputs.values().any(|id| *id == reduction)
            || schedule
                .calls
                .iter()
                .filter(|c| c.buffers[1..].contains(&reduction))
                .count()
                != 1
            || graph.plans[output].ty != x.ty
        {
            continue;
        }
        let Ok(layout) = MatrixLayout::new(&x.ty.shape, &w.ty.shape) else {
            continue;
        };
        if layout.input_channels != layout.output_channels {
            continue;
        }
        selected.push(MatrixClosure {
            layout,
            input,
            weights,
            reduction,
            output,
            residual: true,
        });
    }
    Ok(selected)
}
pub struct PipelineCall {
    pub source: String,
    pub bytes: Vec<usize>,
    pub workers: usize,
    pub scalars: usize,
}
/// Stage logical input, partition reduction, and undo a standalone output's
/// padding. A fused residual writes the dense output directly. Scratch lifetimes
/// reach the actual producer/consumer; only reduction partitions.
pub fn stage(
    selected: &[MatrixClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    stage_with_gemms(selected, &[], graph, schedule, initialized, scratch)
}
pub(crate) fn stage_with_gemms(
    selected: &[MatrixClosure],
    gemms: &[super::gemm::GemmClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    stage_with_chains(selected, gemms, &[], graph, schedule, initialized, scratch)
}
pub(crate) fn stage_with_chains(
    selected: &[MatrixClosure],
    gemms: &[super::gemm::GemmClosure],
    chains: &[super::chain::ChainClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let mut calls = BTreeMap::new();
    let mut staged_gemms = super::gemm::insert(gemms, graph, schedule, initialized, scratch)?;
    staged_gemms.extend(super::chain::insert(
        chains,
        graph,
        schedule,
        initialized,
        scratch,
    )?);
    for closure in selected {
        let layout = &closure.layout;
        let ScalarExpr::Constant(raw) = &graph.plans[closure.weights].expression else {
            return Err("matrix weight storage changed after selection".into());
        };
        let half;
        let raw = if graph.plans[closure.weights].ty.dtype == 1 {
            half = raw
                .chunks_exact(4)
                .flat_map(|v| {
                    crate::compiler_backend_cpu::image::half_storage(u32::from_le_bytes(
                        v.try_into().unwrap(),
                    ))
                    .to_le_bytes()
                })
                .collect::<Vec<_>>();
            half.as_slice()
        } else {
            raw.as_slice()
        };
        let mut packed = vec![0; layout.weight_elements * 2];
        layout.pack_weights_into(raw, &mut packed)?;
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
        let input = buffer(layout.input_elements * 2, false);
        let output = if closure.residual {
            closure.output
        } else {
            buffer(layout.output_elements * 4, false)
        };
        let weight = buffer(packed.len(), true);
        initialized.insert(weight, packed);
        scratch.insert(input);
        if !closure.residual {
            scratch.insert(output);
        }
        if closure.residual {
            schedule.calls.retain(|c| c.buffers[0] != closure.reduction);
        }
        let at = schedule
            .calls
            .iter()
            .position(|c| c.buffers[0] == closure.output)
            .ok_or("matrix selected output has no call")?;
        let indexed = chains.iter().find_map(|c| {
            c.normalization.as_ref().and_then(|n| {
                n.projection
                    .as_ref()
                    .filter(|p| p.input == closure.input)
                    .map(|p| (c, n, p))
            })
        });
        let pack_buffers = if let Some((chain, norm, projection)) = indexed {
            vec![
                input,
                projection.indices,
                projection.history,
                chain.terminal_output,
                norm.scalar,
            ]
        } else {
            vec![input, closure.input]
        };
        let mut pipeline = vec![
            Call {
                copy: false,
                buffers: pack_buffers,
            },
            Call {
                copy: false,
                buffers: vec![output, input, weight],
            },
        ];
        if !closure.residual {
            pipeline.push(Call {
                copy: false,
                buffers: vec![closure.output, output],
            });
        }
        schedule.calls.splice(at..=at, pipeline);
    }
    for (index, call) in schedule.calls.iter().enumerate() {
        for closure in selected {
            let layout = &closure.layout;
            // New scratch producer and reduction are immediately before the
            // logical output. Identify by adjacency, not shape or buffer size.
            if call.buffers[0] != closure.output {
                continue;
            }
            let boundaries = if closure.residual { 1 } else { 2 };
            if index < boundaries {
                return Err("matrix pipeline order missing".into());
            }
            let pack_at = index - boundaries;
            let reduce_at = pack_at + 1;
            let projection = chains.iter().find_map(|c| {
                c.normalization
                    .as_ref()
                    .and_then(|n| n.projection.as_ref())
                    .filter(|p| p.input == closure.input)
            });
            let pack = if let Some(p) = projection {
                super::indexed::artifact(
                    layout.input_channels,
                    graph.plans[p.history].ty.shape[1],
                    layout.batches * layout.rows,
                    &format!("native_indexed_pack_{pack_at}"),
                    &format!("op_kernel_{pack_at}"),
                )?
            } else {
                let p = layout.input_artifact(&format!("op_kernel_{pack_at}"))?;
                PipelineCall {
                    source: p.source,
                    bytes: p.bytes.to_vec(),
                    workers: p.workers,
                    scalars: p.scalars,
                }
            };
            let function = format!("native_matrix_{reduce_at}");
            let entry = format!("op_kernel_{reduce_at}");
            let reduce = if closure.residual {
                layout.residual_artifact(&function, &entry, 16)?
            } else {
                layout.artifact(
                    &function, &entry,
                    // Offline CPU profile: the original receipts explicitly pin
                    // CPU_COUNT=16. Never derive partitioning from this host.
                    16,
                )?
            };
            calls.insert(
                pack_at,
                PipelineCall {
                    source: pack.source,
                    bytes: pack.bytes,
                    workers: 1,
                    scalars: 0,
                },
            );
            calls.insert(
                reduce_at,
                PipelineCall {
                    source: reduce.source,
                    bytes: reduce.bytes.to_vec(),
                    workers: reduce.workers,
                    scalars: reduce.scalars,
                },
            );
            if !closure.residual {
                let undo = layout.output_artifact(&format!("op_kernel_{index}"))?;
                calls.insert(
                    index,
                    PipelineCall {
                        source: undo.source,
                        bytes: undo.bytes.to_vec(),
                        workers: 1,
                        scalars: 0,
                    },
                );
            }
        }
    }
    for (index, call) in super::gemm::render(&staged_gemms, schedule)? {
        if calls.insert(index, call).is_some() {
            return Err("Gemm and standalone matrix selected the same call".into());
        }
    }
    Ok(calls)
}
