//! Stage only already-admitted Conv+bias calls. Source admission is unchanged;
//! the selected primitive owns its internal reduction and once-only packing.
use super::{Artifact, MaskedDepthwise, Pointwise};
use crate::compiler_onnx::{
    lower::{IndexExpr, LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
    scalar::Binary,
    schedule::Schedule,
    Attribute, Model,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct Staged {
    layout: Layout,
    input: usize,
    packed: usize,
    weights: usize,
    bias: usize,
    output: usize,
}
enum Layout {
    MaskedDepthwise(MaskedDepthwise),
    Pointwise(Pointwise),
}
impl Layout {
    fn bytes(&self) -> [usize; 4] {
        match self {
            Self::MaskedDepthwise(layout) => layout.bytes(),
            Self::Pointwise(layout) => layout.bytes(),
        }
    }
    fn pack_weights(&self, raw: &[u8]) -> Result<Vec<u8>, String> {
        match self {
            Self::MaskedDepthwise(layout) => layout.pack_weights(raw),
            Self::Pointwise(layout) => layout.pack_weights(raw),
        }
    }
    fn input_artifact(
        &self,
        entry: &str,
    ) -> Result<crate::compiler_backend_cpu::matrix::graph::PipelineCall, String> {
        match self {
            Self::MaskedDepthwise(layout) => layout.input_artifact(entry),
            Self::Pointwise(layout) => layout.input_artifact(entry),
        }
    }
    fn artifact(&self, function: &str, entry: &str) -> Result<Artifact, String> {
        match self {
            Self::MaskedDepthwise(layout) => layout.artifact(function, entry),
            Self::Pointwise(layout) => layout.artifact(function, entry),
        }
    }
}
pub(crate) fn stage(
    graph: &LoweredGraph,
    model: &Model,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    reserved: &BTreeSet<usize>,
) -> Result<Vec<Staged>, String> {
    let mut staged = Vec::new();
    for node in model.graph.nodes.iter().filter(|n| n.op == "Conv") {
        if node.inputs.len() != 3 || node.outputs.len() != 1 {
            continue;
        }
        let ids = node
            .inputs
            .iter()
            .chain(&node.outputs)
            .map(|name| graph.names.get(name).copied())
            .collect::<Option<Vec<_>>>();
        let Some(ids) = ids else {
            continue;
        };
        let (input, weights, bias, output) = (ids[0], ids[1], ids[2], ids[3]);
        if reserved.contains(&output) {
            continue;
        }
        if !schedule.live.contains(&output) {
            continue;
        }
        let x = &graph.plans[input].ty;
        let w = &graph.plans[weights];
        let b = &graph.plans[bias];
        if x.dtype != 1
            || x.shape.len() != 4
            || x.shape[0] != 1
            || w.ty.dtype != 10
            || w.ty.shape.len() != 4
            || b.ty.dtype != 10
            || b.ty.shape != [w.ty.shape[0]]
            || graph.plans[output].ty.dtype != 1
        {
            continue;
        }
        let Some(Attribute::Integer(group)) = node.attributes.get("group") else {
            continue;
        };
        let Some(Attribute::Integers(pads)) = node.attributes.get("pads") else {
            continue;
        };
        let Ok(pads) = pads
            .iter()
            .map(|&p| usize::try_from(p))
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        let Ok(pads) = <Vec<usize> as TryInto<[usize; 4]>>::try_into(pads) else {
            continue;
        };
        let attributes_proved =
            node.attributes
                .iter()
                .all(|(name, value)| match (name.as_str(), value) {
                    ("group" | "pads", _) => true,
                    ("strides" | "dilations", Attribute::Integers(v)) => v == &[1, 1],
                    ("kernel_shape", Attribute::Integers(v)) => {
                        v.iter()
                            .map(|&v| usize::try_from(v))
                            .collect::<Result<Vec<_>, _>>()
                            .ok()
                            .as_deref()
                            == Some(&w.ty.shape[2..])
                    }
                    ("auto_pad", Attribute::String(v)) => v == "NOTSET",
                    _ => false,
                });
        if !attributes_proved {
            continue;
        }
        let input_shape = [x.shape[1], x.shape[2], x.shape[3]];
        let selected = if usize::try_from(*group).ok() == Some(x.shape[1])
            && w.ty.shape[..2] == [x.shape[1], 1]
        {
            MaskedDepthwise::new(input_shape, [w.ty.shape[2], w.ty.shape[3]], pads).map(|layout| {
                let output = [1, layout.channels, layout.output[0], layout.output[1]];
                (Layout::MaskedDepthwise(layout), output)
            })
        } else if *group == 1
            && w.ty.shape[1] == x.shape[1]
            && w.ty.shape[2..] == [1, 1]
            && pads == [0; 4]
        {
            Pointwise::new(input_shape, w.ty.shape[0]).map(|layout| {
                (
                    Layout::Pointwise(layout),
                    [1, w.ty.shape[0], x.shape[2], x.shape[3]],
                )
            })
        } else {
            continue;
        };
        let Ok((layout, output_shape)) = selected else {
            continue;
        };
        if graph.plans[output].ty.shape != output_shape {
            continue;
        }
        let (ScalarExpr::Constant(raw_weights), ScalarExpr::Constant(_)) =
            (&w.expression, &b.expression)
        else {
            continue;
        };
        let ScalarExpr::Binary {
            kind: Binary::Add,
            dtype: 1,
            left,
            right,
        } = &graph.plans[output].expression
        else {
            continue;
        };
        let (
            ScalarExpr::Load {
                plan: reduction,
                index: IndexExpr::Index,
            },
            ScalarExpr::Load {
                plan: loaded_bias, ..
            },
        ) = (left.as_ref(), right.as_ref())
        else {
            continue;
        };
        if *loaded_bias != bias
            || schedule.buffers[*reduction].held
            || schedule
                .calls
                .iter()
                .filter(|c| c.buffers[1..].contains(reduction))
                .count()
                != 1
        {
            continue;
        }
        let Some(reduction_call) = schedule.calls.iter().find(|c| c.buffers[0] == *reduction)
        else {
            continue;
        };
        if reduction_call.buffers != [*reduction, input, weights] {
            continue;
        }
        let Some(output_call) = schedule.calls.iter().find(|c| c.buffers[0] == output) else {
            continue;
        };
        if output_call.buffers != [output, *reduction, bias] {
            continue;
        }
        let packed_weights = layout.pack_weights(raw_weights)?;
        let packed = schedule.buffers.len();
        schedule.buffers.push(BufferSpec {
            device: "CPU".into(),
            bytes: layout.bytes()[1],
            held: false,
            supports_offset: true,
        });
        let weights = schedule.buffers.len();
        schedule.buffers.push(BufferSpec {
            device: "CPU".into(),
            bytes: packed_weights.len(),
            held: true,
            supports_offset: true,
        });
        initialized.insert(weights, packed_weights);
        schedule.calls.retain(|c| c.buffers[0] != *reduction);
        let at = schedule
            .calls
            .iter()
            .position(|c| c.buffers[0] == output)
            .ok_or("depthwise selected output call vanished")?;
        schedule.calls.splice(
            at..=at,
            [
                Call {
                    copy: false,
                    buffers: vec![packed, input],
                },
                Call {
                    copy: false,
                    buffers: vec![output, packed, weights, bias],
                },
            ],
        );
        staged.push(Staged {
            layout,
            input,
            packed,
            weights,
            bias,
            output,
        });
    }
    Ok(staged)
}
pub(crate) fn scheduled(
    staged: &[Staged],
    schedule: &Schedule,
) -> Result<BTreeMap<usize, crate::compiler_backend_cpu::matrix::graph::PipelineCall>, String> {
    let mut result = BTreeMap::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        for item in staged {
            let entry = format!("op_kernel_{index}");
            if call.buffers[0] == item.packed {
                if call.buffers != [item.packed, item.input] {
                    return Err("depthwise packing ABI changed".into());
                }
                result.insert(index, item.layout.input_artifact(&entry)?);
            } else if call.buffers[0] == item.output {
                if call.buffers != [item.output, item.packed, item.weights, item.bias] {
                    return Err("depthwise reduction ABI changed".into());
                }
                let kind = match item.layout {
                    Layout::MaskedDepthwise(_) => "depthwise",
                    Layout::Pointwise(_) => "pointwise",
                };
                let artifact = item
                    .layout
                    .artifact(&format!("native_{kind}_{index}"), &entry)?;
                result.insert(
                    index,
                    crate::compiler_backend_cpu::matrix::graph::PipelineCall {
                        source: artifact.source,
                        bytes: artifact.bytes.to_vec(),
                        workers: artifact.workers,
                        scalars: artifact.scalars,
                    },
                );
            }
        }
    }
    Ok(result)
}
