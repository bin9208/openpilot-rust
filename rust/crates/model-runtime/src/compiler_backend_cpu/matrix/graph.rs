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
    pub bias: Option<usize>,
    /// Biased value and Sigmoid are owned by the selected SiLU profile.
    pub silu: Option<(usize, usize)>,
    /// Unheld SiLU is stored directly in the sole terminal matrix's half input.
    pub packed_silu: bool,
}
impl MatrixClosure {
    pub(crate) fn owned(&self) -> BTreeSet<usize> {
        let mut owned = BTreeSet::from([self.reduction, self.output]);
        if let Some((linear, sigmoid)) = self.silu {
            owned.extend([linear, sigmoid]);
        }
        owned
    }
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
            bias: None,
            silu: None,
            packed_silu: false,
        });
    }
    // A channel bias participates in the same original matrix DAG. Selecting
    // the raw matrix separately would change its materialization and schedule.
    for add in model.graph.nodes.iter().filter(|n| {
        n.op == "Add" && n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty()
    }) {
        let Some(&linear) = graph.names.get(&add.outputs[0]) else {
            continue;
        };
        let (output, silu, packed_silu) = if graph.outputs.values().any(|id| *id == linear) {
            (linear, None, false)
        } else {
            let Some(sigmoid) = model.graph.nodes.iter().find(|n| {
                n.op == "Sigmoid"
                    && n.inputs == add.outputs
                    && n.outputs.len() == 1
                    && n.attributes.is_empty()
            }) else {
                continue;
            };
            let Some(mul) = model.graph.nodes.iter().find(|n| {
                n.op == "Mul"
                    && n.inputs == [add.outputs[0].clone(), sigmoid.outputs[0].clone()]
                    && n.outputs.len() == 1
                    && n.attributes.is_empty()
            }) else {
                continue;
            };
            let (Some(&sigmoid_id), Some(&output)) = (
                graph.names.get(&sigmoid.outputs[0]),
                graph.names.get(&mul.outputs[0]),
            ) else {
                continue;
            };
            // The biased value has exactly two intentional source readers.
            // Identity, order and held visibility matter; equal shapes do not
            // permit replacing another Sigmoid or extra alias/view consumer.
            let users = |name: &str| {
                model
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| n.inputs.iter().any(|v| v == name))
                    .collect::<Vec<_>>()
            };
            let packed_silu = !graph.outputs.values().any(|id| *id == output);
            if packed_silu {
                // Only one exact terminal MatMul may consume this unheld value.
                // Held values and additional source/scheduled readers retain
                // their distinct original materialization, including dead
                // source readers that could otherwise alter fusion choices.
                let readers = users(&mul.outputs[0]);
                if readers.len() != 1 {
                    continue;
                }
                let consumer = readers[0];
                if consumer.op != "MatMul"
                    || consumer.inputs.len() != 2
                    || consumer.inputs[0] != mul.outputs[0]
                    || consumer.outputs.len() != 1
                    || !consumer.attributes.is_empty()
                    || !selected.iter().any(|c| {
                        c.input == output
                            && c.output == graph.names[&consumer.outputs[0]]
                            && c.output == c.reduction
                            && c.bias.is_none()
                            && !c.residual
                    })
                    || schedule
                        .calls
                        .iter()
                        .filter(|c| c.buffers[1..].contains(&output))
                        .count()
                        != 1
                {
                    continue;
                }
            }
            if graph.outputs.values().any(|id| *id == sigmoid_id)
                || users(&add.outputs[0]) != vec![sigmoid, mul]
                || users(&sigmoid.outputs[0]) != vec![mul]
                || graph.plans[linear].ty != graph.plans[output].ty
                || graph.plans[sigmoid_id].ty != graph.plans[output].ty
                || schedule.calls.iter().any(|c| {
                    ![linear, sigmoid_id, output].contains(&c.buffers[0])
                        && c.buffers[1..]
                            .iter()
                            .any(|id| [linear, sigmoid_id].contains(id))
                })
            {
                continue;
            }
            (output, Some((linear, sigmoid_id)), packed_silu)
        };
        let Some(matrix) = super::gemm::producer(model, &add.inputs[0], "MatMul")
            .filter(|n| n.inputs.len() == 2 && n.outputs.len() == 1 && n.attributes.is_empty())
        else {
            continue;
        };
        let (input, weights, reduction, bias) = (
            graph.names[&matrix.inputs[0]],
            graph.names[&matrix.inputs[1]],
            graph.names[&matrix.outputs[0]],
            graph.names[&add.inputs[1]],
        );
        let (x, w) = (&graph.plans[input], &graph.plans[weights]);
        if x.ty.dtype != 1
            || w.ty.dtype != 10
            || !(2..=3).contains(&x.ty.shape.len())
            || matches!(x.expression, ScalarExpr::Constant(_))
            || !matches!(w.expression, ScalarExpr::Constant(_))
            || graph.outputs.values().any(|id| *id == reduction)
            || (silu.is_some()
                && model
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| n.inputs.iter().any(|v| v == &matrix.outputs[0]))
                    .count()
                    != 1)
            || schedule
                .calls
                .iter()
                .filter(|c| c.buffers[1..].contains(&reduction))
                .count()
                != 1
        {
            continue;
        }
        let Ok(layout) = MatrixLayout::new(&x.ty.shape, &w.ty.shape) else {
            continue;
        };
        let mut shape = x.ty.shape.clone();
        *shape.last_mut().unwrap() = layout.output_channels;
        if graph.plans[output].ty.dtype != 1
            || graph.plans[output].ty.shape != shape
            || !super::gemm::constant(graph, bias, &[layout.output_channels])
        {
            continue;
        }
        selected.push(MatrixClosure {
            layout,
            input,
            weights,
            output,
            reduction,
            residual: false,
            bias: Some(bias),
            silu,
            packed_silu,
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
            bias: None,
            silu: None,
            packed_silu: false,
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
        let direct = closure.residual || closure.bias.is_some();
        if closure.packed_silu {
            let consumer = selected
                .iter()
                .find(|c| c.input == closure.output)
                .ok_or("packed SiLU lost its sole selected terminal matrix")?;
            if schedule.buffers[closure.output].held {
                return Err("packed SiLU cannot replace held logical storage".into());
            }
            schedule.buffers[closure.output].bytes = consumer.layout.input_elements * 2;
            scratch.insert(closure.output);
        }
        let packed_input = selected
            .iter()
            .any(|c| c.packed_silu && c.output == closure.input);
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
        let input = if packed_input {
            closure.input
        } else {
            buffer(layout.input_elements * 2, false)
        };
        let output = if direct {
            closure.output
        } else {
            buffer(layout.output_elements * 4, false)
        };
        let weight = buffer(packed.len(), true);
        initialized.insert(weight, packed);
        if !packed_input {
            scratch.insert(input);
        }
        if !direct {
            scratch.insert(output);
        }
        if direct {
            let owned = closure.owned();
            schedule
                .calls
                .retain(|c| c.buffers[0] == closure.output || !owned.contains(&c.buffers[0]));
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
        let mut reduction_buffers = vec![output, input, weight];
        if let Some(bias) = closure.bias {
            reduction_buffers.push(bias);
        }
        let mut pipeline = Vec::new();
        if !packed_input {
            pipeline.push(Call {
                copy: false,
                buffers: pack_buffers,
            });
        }
        pipeline.push(Call {
            copy: false,
            buffers: reduction_buffers,
        });
        if !direct {
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
            let direct = closure.residual || closure.bias.is_some();
            let packed_input = selected
                .iter()
                .any(|c| c.packed_silu && c.output == closure.input);
            let boundaries = usize::from(!direct) + usize::from(!packed_input);
            if index < boundaries {
                return Err("matrix pipeline order missing".into());
            }
            let reduce_at = index - usize::from(!direct);
            let pack_at = (!packed_input).then(|| reduce_at - 1);
            let projection = chains.iter().find_map(|c| {
                c.normalization
                    .as_ref()
                    .and_then(|n| n.projection.as_ref())
                    .filter(|p| p.input == closure.input)
            });
            let pack = if let Some(pack_at) = pack_at {
                Some(if let Some(p) = projection {
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
                })
            } else {
                None
            };
            let function = format!("native_matrix_{reduce_at}");
            let entry = format!("op_kernel_{reduce_at}");
            let reduce = if closure.bias.is_some() {
                let artifact = if closure.packed_silu {
                    layout.packed_silu_artifact(&function, &entry, 16)?
                } else if closure.silu.is_some() {
                    layout.broadcast_silu_artifact(&function, &entry, 16)?
                } else {
                    layout.broadcast_bias_artifact(&function, &entry, 16)?
                };
                PipelineCall {
                    source: artifact.source,
                    bytes: artifact.bytes.to_vec(),
                    workers: artifact.workers,
                    scalars: artifact.scalars,
                }
            } else {
                let artifact = if closure.residual {
                    layout.residual_artifact(&function, &entry, 16)?
                } else {
                    layout.artifact(
                        &function, &entry,
                        // Offline CPU profile: the original receipts explicitly pin
                        // CPU_COUNT=16. Never derive partitioning from this host.
                        16,
                    )?
                };
                PipelineCall {
                    source: artifact.source,
                    bytes: artifact.bytes.to_vec(),
                    workers: artifact.workers,
                    scalars: artifact.scalars,
                }
            };
            if let (Some(pack_at), Some(pack)) = (pack_at, pack) {
                calls.insert(
                    pack_at,
                    PipelineCall {
                        source: pack.source,
                        bytes: pack.bytes,
                        workers: 1,
                        scalars: 0,
                    },
                );
            }
            calls.insert(
                reduce_at,
                PipelineCall {
                    source: reduce.source,
                    bytes: reduce.bytes,
                    workers: reduce.workers,
                    scalars: reduce.scalars,
                },
            );
            if !direct {
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
