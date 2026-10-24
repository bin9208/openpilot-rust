//! Closed-reader discovery for residual blocks and a terminal biased Gemm,
//! normalized row, or fused Concat head tail. Staging and admission are separate.
use super::{
    gemm::{buffer, constant, gemm, pack_weight, producer, StageKind, Staged},
    graph::PipelineCall,
    MatrixLayout,
};
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    memory::Call,
    schedule::{schedule_plans, Schedule},
    Attribute, Model, Node,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct Block {
    pub root: usize,
    pub output: usize,
    pub first: MatrixLayout,
    pub second: MatrixLayout,
    pub weights: [usize; 2],
    pub biases: [usize; 2],
}
#[derive(Debug)]
pub struct ChainClosure {
    pub root: usize,
    pub output: usize,
    pub terminal_output: usize,
    pub normalization: Option<Normalization>,
    pub branch: Option<super::branch::Tail>,
    pub blocks: Vec<Block>,
    pub terminal: MatrixLayout,
    pub terminal_weight: usize,
    pub terminal_bias: usize,
    pub owned: BTreeSet<usize>,
}

#[derive(Debug)]
pub struct Normalization {
    pub scalar: usize,
    pub packed: bool,
    pub indexed: bool,
    pub projection: Option<IndexedProjection>,
    pub named: BTreeSet<usize>,
}
#[derive(Debug)]
pub struct IndexedProjection {
    pub input: usize,
    pub matrix_output: usize,
    pub history: usize,
    pub indices: usize,
    pub named: BTreeSet<usize>,
}
// The complete feature path is owned through the final matrix input. Its
// normalization/Concat/indexing must be fused into that matrix's half pack.
fn indexed_projection_tail(
    graph: &LoweredGraph,
    model: &Model,
    output: usize,
) -> Option<IndexedProjection> {
    let Ok(schedule) = schedule_plans(graph, "CPU", true) else {
        return None;
    };
    let held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    let next = |id: usize| {
        let mut users = model.graph.nodes.iter().filter(|node| {
            node.inputs
                .iter()
                .any(|name| graph.names.get(name) == Some(&id))
                && node.outputs.iter().any(|name| {
                    graph
                        .names
                        .get(name)
                        .is_some_and(|v| *v != id && schedule.live.contains(v))
                })
        });
        let first = users.next()?;
        if users.next().is_some() {
            None
        } else {
            Some(first)
        }
    };
    let shape = &graph.plans[output].ty.shape;
    if shape.len() != 2 || shape[0] != 1 {
        return None;
    }
    let channels = shape[1];
    let unsqueeze = next(output)?;
    if unsqueeze.op != "Unsqueeze"
        || unsqueeze.inputs.len() != 2
        || unsqueeze.outputs.len() != 1
        || !unsqueeze.attributes.is_empty()
    {
        return None;
    }
    let axes = model.graph.initializers.get(&unsqueeze.inputs[1])?;
    if axes.dtype != 7 || axes.shape != [1] || axes.data != 1i64.to_le_bytes() {
        return None;
    }
    let unsqueezed = graph.names[&unsqueeze.outputs[0]];
    if held.contains(&unsqueezed) || graph.plans[unsqueezed].ty.shape != [1, 1, channels] {
        return None;
    }
    let concat = next(unsqueezed)?;
    if concat.op != "Concat"
        || concat.inputs.len() != 2
        || concat.outputs.len() != 1
        || graph.names[&concat.inputs[1]] != unsqueezed
        || concat.attributes.len() != 1
        || !matches!(concat.attributes.get("axis"), Some(Attribute::Integer(1)))
    {
        return None;
    }
    let history = &graph.plans[graph.names[&concat.inputs[0]]];
    if history.ty.dtype != 1
        || history.ty.shape.len() != 3
        || history.ty.shape[0] != 1
        || history.ty.shape[2] != channels
        || !matches!(history.expression, ScalarExpr::Input(_))
    {
        return None;
    }
    let rows = history.ty.shape[1] + 1;
    let joined = graph.names[&concat.outputs[0]];
    if held.contains(&joined) || graph.plans[joined].ty.shape != [1, rows, channels] {
        return None;
    }
    let transposed = |node: &Node| {
        node.op == "Transpose"
            && node.inputs.len() == 1
            && node.outputs.len() == 1
            && node.attributes.len() == 1
            && matches!(node.attributes.get("perm"), Some(Attribute::Integers(v)) if v == &[1,0,2])
    };
    let first_transpose = next(joined)?;
    if !transposed(first_transpose) {
        return None;
    }
    let transposed_id = graph.names[&first_transpose.outputs[0]];
    if held.contains(&transposed_id) || graph.plans[transposed_id].ty.shape != [rows, 1, channels] {
        return None;
    }
    let gather = next(transposed_id)?;
    if gather.op != "GatherND"
        || gather.inputs.len() != 2
        || gather.outputs.len() != 1
        || gather.attributes.iter().any(|(key, value)| {
            !matches!((key.as_str(), value), ("batch_dims", Attribute::Integer(0)))
        })
    {
        return None;
    }
    let indices = model.graph.initializers.get(&gather.inputs[1])?;
    if indices.dtype != 7 || indices.shape.len() != 2 || indices.shape[1] != 1 {
        return None;
    }
    let Ok(window) = usize::try_from(indices.shape[0]) else {
        return None;
    };
    if window == 0
        || window > rows
        || indices.data.len() != window * 8
        || indices
            .data
            .chunks_exact(8)
            .enumerate()
            .any(|(i, raw)| i64::from_le_bytes(raw.try_into().unwrap()) != i as i64 - window as i64)
    {
        return None;
    }
    let gathered = graph.names[&gather.outputs[0]];
    if held.contains(&gathered) || graph.plans[gathered].ty.shape != [window, 1, channels] {
        return None;
    }
    let last_transpose = next(gathered)?;
    if !transposed(last_transpose) {
        return None;
    }
    let input = graph.names[&last_transpose.outputs[0]];
    if held.contains(&input) || graph.plans[input].ty.shape != [1, window, channels] {
        return None;
    }
    let matrix = next(input)?;
    if matrix.op != "MatMul"
        || !super::graph::closures(graph, model).is_ok_and(|c| {
            c.iter()
                .any(|c| c.input == input && c.reduction == graph.names[&matrix.outputs[0]])
        })
    {
        return None;
    }
    Some(IndexedProjection {
        input,
        matrix_output: graph.names[&matrix.outputs[0]],
        history: graph.names[&concat.inputs[0]],
        indices: graph.names[&gather.inputs[1]],
        named: BTreeSet::from([unsqueezed, joined, transposed_id, gathered, input]),
    })
}
fn normalized_tail(
    graph: &LoweredGraph,
    model: &Model,
    terminal: &Node,
    downstream: &[ChainClosure],
) -> Option<(usize, Normalization)> {
    let live = schedule_plans(graph, "CPU", true).ok()?.live;
    let users = |name: &str| {
        model
            .graph
            .nodes
            .iter()
            .filter(|n| {
                n.inputs.iter().any(|i| i == name)
                    && n.outputs
                        .iter()
                        .any(|o| graph.names.get(o).is_some_and(|id| live.contains(id)))
            })
            .collect::<Vec<_>>()
    };
    let terminal_name = &terminal.outputs[0];
    let readers = users(terminal_name);
    if readers.len() != 2 {
        return None;
    }
    let norm = *readers.iter().find(|n| n.op == "ReduceL2")?;
    let divide = *readers.iter().find(|n| n.op == "Div")?;
    let terminal_id = graph.names[terminal_name];
    let shape = &graph.plans[terminal_id].ty.shape;
    if shape.len() != 2
        || shape[0] != 1
        || graph.plans[terminal_id].ty.dtype != 1
        || norm.inputs.len() != 2
        || norm.inputs[0] != *terminal_name
        || norm.outputs.len() != 1
        || norm.attributes.iter().any(|(key, value)| {
            !matches!(
                (key.as_str(), value),
                ("keepdims", Attribute::Integer(1))
                    | ("noop_with_empty_axes", Attribute::Integer(0))
            )
        })
        || divide.inputs.len() != 2
        || divide.inputs[0] != *terminal_name
        || divide.outputs.len() != 1
        || !divide.attributes.is_empty()
    {
        return None;
    }
    let axes = model.graph.initializers.get(&norm.inputs[1])?;
    if axes.dtype != 7 || axes.shape != [1] || axes.data != (-1i64).to_le_bytes() {
        return None;
    }
    let norm_users = users(&norm.outputs[0]);
    let [clip] = norm_users.as_slice() else {
        return None;
    };
    if clip.op != "Clip"
        || clip.inputs.len() != 2
        || clip.inputs[0] != norm.outputs[0]
        || clip.outputs.len() != 1
        || !clip.attributes.is_empty()
    {
        return None;
    }
    let minimum = model.graph.initializers.get(&clip.inputs[1])?;
    if minimum.dtype != 10 || !minimum.shape.is_empty() || minimum.data != [0, 0] {
        return None;
    }
    let clip_users = users(&clip.outputs[0]);
    let [expand] = clip_users.as_slice() else {
        return None;
    };
    if expand.op != "Expand"
        || expand.inputs.len() != 2
        || expand.inputs[0] != clip.outputs[0]
        || expand.outputs.len() != 1
        || !expand.attributes.is_empty()
        || divide.inputs[1] != expand.outputs[0]
    {
        return None;
    }
    let expanded = model.graph.initializers.get(&expand.inputs[1])?;
    let expected = [1i64.to_le_bytes(), (shape[1] as i64).to_le_bytes()].concat();
    if expanded.dtype != 7 || expanded.shape != [2] || expanded.data != expected {
        return None;
    }
    let expand_users = users(&expand.outputs[0]);
    if expand_users.len() != 1 || expand_users[0] != divide {
        return None;
    }
    let output = graph.names[&divide.outputs[0]];
    let held_output = graph.outputs.values().any(|&id| id == output);
    let projection = if held_output {
        None
    } else {
        indexed_projection_tail(graph, model, output)
    };
    let indexed = projection.is_some();
    if !held_output && !indexed && !downstream.iter().any(|c| c.root == output) {
        return None;
    }
    if graph.plans[output].ty != graph.plans[terminal_id].ty
        || graph.plans[graph.names[&norm.outputs[0]]].ty.shape != [1, 1]
        || graph.plans[graph.names[&clip.outputs[0]]].ty.shape != [1, 1]
        || graph.plans[graph.names[&expand.outputs[0]]].ty != graph.plans[terminal_id].ty
    {
        return None;
    }
    let mut named: BTreeSet<usize> = [
        &norm.outputs[0],
        &clip.outputs[0],
        &expand.outputs[0],
        &divide.outputs[0],
    ]
    .into_iter()
    .map(|name| graph.names[name])
    .collect();
    if let Some(p) = &projection {
        named.extend(&p.named);
    }
    Some((
        output,
        Normalization {
            scalar: graph.names[&clip.outputs[0]],
            packed: !held_output && !indexed,
            indexed,
            projection,
            named,
        },
    ))
}
fn block(
    graph: &LoweredGraph,
    model: &Model,
    activation: &str,
) -> Option<(Block, BTreeSet<usize>)> {
    let activated = producer(model, activation, "Relu")?;
    if activated.inputs.len() != 1 || !activated.attributes.is_empty() {
        return None;
    }
    let add = producer(model, &activated.inputs[0], "Add")?;
    if add.inputs.len() != 2 || !add.attributes.is_empty() {
        return None;
    }
    let second = producer(model, &add.inputs[1], "Gemm").filter(|n| gemm(n))?;
    let hidden = producer(model, &second.inputs[0], "Relu")?;
    if hidden.inputs.len() != 1 || !hidden.attributes.is_empty() {
        return None;
    }
    let first = producer(model, &hidden.inputs[0], "Gemm").filter(|n| gemm(n))?;
    if first.inputs[0] != add.inputs[0] {
        return None;
    }
    let root = *graph.names.get(&add.inputs[0])?;
    let output = *graph.names.get(activation)?;
    let ty = &graph.plans[root].ty;
    if ty.dtype != 1 || ty.shape.len() != 2 || ty.shape[0] != 1 {
        return None;
    }
    let weights = [
        graph.names[&first.inputs[1]],
        graph.names[&second.inputs[1]],
    ];
    let biases = [
        graph.names[&first.inputs[2]],
        graph.names[&second.inputs[2]],
    ];
    let n = ty.shape[1];
    let shape = &graph.plans[weights[0]].ty.shape;
    if shape.len() != 2 || shape[1] != n {
        return None;
    }
    let k = shape[0];
    if !constant(graph, weights[0], &[k, n])
        || !constant(graph, weights[1], &[n, k])
        || !constant(graph, biases[0], &[k])
        || !constant(graph, biases[1], &[n])
        || graph.plans[graph.names[&hidden.outputs[0]]].ty.shape != [1, k]
        || graph.plans[graph.names[&hidden.outputs[0]]].ty.dtype != 1
        || graph.plans[output].ty != *ty
        || graph.plans[graph.names[&add.outputs[0]]].ty != *ty
    {
        return None;
    }
    let first_layout = MatrixLayout::new(&[1, n], &[n, k]).ok()?;
    let second_layout = MatrixLayout::new(&[1, k], &[k, n]).ok()?;
    let named = [
        &first.outputs[0],
        &hidden.outputs[0],
        &second.outputs[0],
        &add.outputs[0],
        &activated.outputs[0],
    ]
    .into_iter()
    .map(|name| graph.names[name])
    .collect();
    Some((
        Block {
            root,
            output,
            first: first_layout,
            second: second_layout,
            weights,
            biases,
        },
        named,
    ))
}
/// Discover the complete paired-reader chain before any scheduling mutation.
/// Held Gemm/Concat and exactly owned normalized tails retain their distinct
/// original storage and reduction profiles.
pub fn closures(graph: &LoweredGraph, model: &Model) -> Result<Vec<ChainClosure>, String> {
    let schedule = schedule_plans(graph, "CPU", true)?;
    let held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    let calls = schedule
        .calls
        .iter()
        .map(|c| (c.buffers[0], &c.buffers[1..]))
        .collect::<BTreeMap<_, _>>();
    let mut selected: Vec<ChainClosure> = Vec::new();
    for terminal_node in model
        .graph
        .nodes
        .iter()
        .filter(|n| n.op == "Gemm" && gemm(n))
        .rev()
    {
        let source_terminal = graph.names[&terminal_node.outputs[0]];
        let branch = super::branch::tail(graph, model, terminal_node);
        let terminal_output = branch.as_ref().map_or(source_terminal, |b| b.output);
        if !schedule.live.contains(&terminal_output) {
            continue;
        }
        let (output, normalization) = if branch.is_some() || held.contains(&terminal_output) {
            (terminal_output, None)
        } else if let Some((output, norm)) = normalized_tail(graph, model, terminal_node, &selected)
        {
            (output, Some(norm))
        } else {
            continue;
        };
        let mut current = terminal_node.inputs[0].as_str();
        let mut blocks = Vec::new();
        let mut named = BTreeSet::from([source_terminal]);
        if let Some(b) = &branch {
            named.extend(&b.named);
        }
        if let Some(norm) = &normalization {
            named.extend(&norm.named);
        }
        let mut visited = BTreeSet::new();
        while let Some((next, names)) = block(graph, model, current) {
            if !visited.insert(next.output) {
                return Err("cyclic residual chain source".into());
            }
            let activation = producer(model, current, "Relu").unwrap();
            let add = producer(model, &activation.inputs[0], "Add").unwrap();
            current = &add.inputs[0];
            named.extend(names);
            blocks.push(next);
        }
        if blocks.len() < 2 {
            continue;
        }
        blocks.reverse();
        let root = blocks[0].root;
        if matches!(
            graph.plans[root].expression,
            ScalarExpr::Input(_)
                | ScalarExpr::Constant(_)
                | ScalarExpr::Load { .. }
                | ScalarExpr::Cast { .. }
        ) {
            continue;
        }
        if blocks.windows(2).any(|b| b[0].output != b[1].root) {
            continue;
        }
        let terminal_input = blocks.last().unwrap().output;
        let ty = &graph.plans[terminal_input].ty;
        let weight = graph.names[&terminal_node.inputs[1]];
        let bias = graph.names[&terminal_node.inputs[2]];
        let weights = &graph.plans[weight].ty.shape;
        if weights.len() != 2 || weights[1] != ty.shape[1] {
            continue;
        }
        let n = weights[0];
        if !constant(graph, weight, &[n, ty.shape[1]])
            || !constant(graph, bias, &[n])
            || graph.plans[output].ty.dtype != 1
            || graph.plans[output].ty.shape
                != [
                    1,
                    branch
                        .as_ref()
                        .map_or(n, |b| b.heads.iter().map(|h| h.channels).sum()),
                ]
        {
            continue;
        }
        let Ok(terminal) = MatrixLayout::new(
            &ty.shape,
            &[
                ty.shape[1],
                if branch.is_some() {
                    n.next_multiple_of(4)
                } else {
                    n
                },
            ],
        ) else {
            continue;
        };
        // Walk actual lowered dependencies, stopping at the external computed
        // root and initialized constants. Unknown source plans are not owned.
        let mut owned = BTreeSet::new();
        let projection = normalization.as_ref().and_then(|n| n.projection.as_ref());
        let mut pending = vec![output];
        if let Some(p) = projection {
            pending.push(p.input);
        }
        let mut supported = true;
        while let Some(id) = pending.pop() {
            if id == root
                || projection.is_some_and(|p| id == p.history)
                || matches!(graph.plans[id].expression, ScalarExpr::Constant(_))
                || !owned.insert(id)
            {
                continue;
            }
            if graph.names.values().any(|v| *v == id) && !named.contains(&id) {
                supported = false;
                break;
            }
            let Some(inputs) = calls.get(&id) else {
                supported = false;
                break;
            };
            pending.extend(inputs.iter().copied());
        }
        // The chain owns BOTH uses of each packed residual edge. Any live
        // outside source/scheduled reader or held intermediate changes the
        // original materialization and cannot share this selection.
        if !supported
            || !named.is_subset(&owned)
            || owned.iter().any(|id| *id != output && held.contains(id))
            || schedule.calls.iter().any(|c| {
                !owned.contains(&c.buffers[0])
                    && c.buffers[1..].iter().any(|id| {
                        *id == root
                            || owned.contains(id)
                                && !normalization.as_ref().is_some_and(|n| {
                                    n.projection.as_ref().is_some_and(|p| {
                                        *id == p.input && c.buffers[0] == p.matrix_output
                                    }) || *id == output
                                        && n.packed
                                        && selected.iter().any(|next| {
                                            next.root == output
                                                && next.owned.contains(&c.buffers[0])
                                        })
                                })
                    })
            })
            || selected.iter().any(|c| !c.owned.is_disjoint(&owned))
        {
            continue;
        }
        selected.push(ChainClosure {
            root,
            output,
            terminal_output,
            normalization,
            branch,
            blocks,
            terminal,
            terminal_weight: weight,
            terminal_bias: bias,
            owned,
        });
    }
    selected.reverse();
    Ok(selected)
}

/// Stage only a complete previously selected closed chain. Packed residual
/// edges remain private and live through both consumers; no decode/repack or
/// overlapping in-place update is introduced between blocks.
pub fn stage(
    selected: &[ChainClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let staged = insert(selected, graph, schedule, initialized, scratch)?;
    super::gemm::render(&staged, schedule)
}

// In a combined matrix planner all insertions must precede rendering, because
// entrypoints encode the final schedule index. Keep the insertion phase separate.
pub(super) fn insert(
    selected: &[ChainClosure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
    scratch: &mut BTreeSet<usize>,
) -> Result<Vec<Staged>, String> {
    let mut staged = Vec::new();
    let mut packed_edges = BTreeMap::new();
    for chain in selected {
        let first = &chain
            .blocks
            .first()
            .ok_or("selected chain has no blocks")?
            .first;
        let mut pipeline = Vec::new();
        let mut root = if let Some(packed) = packed_edges.remove(&chain.root) {
            packed
        } else {
            let root = buffer(schedule, first.input_elements * 2, false);
            scratch.insert(root);
            pipeline.push(Call {
                copy: false,
                buffers: vec![root, chain.root],
            });
            staged.push(Staged {
                output: root,
                layout: first.clone(),
                kind: StageKind::Pack,
                relu_half: false,
            });
            root
        };
        for block in &chain.blocks {
            if schedule.buffers[root].bytes != block.first.input_elements * 2 {
                return Err("selected chain packed input capacity changed".into());
            }
            let hidden = buffer(schedule, block.second.input_elements * 2, false);
            let packed = MatrixLayout::new(
                &[1, block.second.output_channels],
                &[block.second.output_channels, 4],
            )?;
            let next = buffer(schedule, packed.input_elements * 2, false);
            scratch.extend([hidden, next]);
            let mut weights = Vec::new();
            for (&id, layout) in block.weights.iter().zip([&block.first, &block.second]) {
                let raw = pack_weight(graph, id, layout)?;
                let weight = buffer(schedule, raw.len(), true);
                initialized.insert(weight, raw);
                weights.push(weight);
            }
            pipeline.extend([
                Call {
                    copy: false,
                    buffers: vec![hidden, root, weights[0], block.biases[0]],
                },
                Call {
                    copy: false,
                    buffers: vec![next, root, hidden, weights[1], block.biases[1]],
                },
            ]);
            staged.extend([
                Staged {
                    output: hidden,
                    layout: block.first.clone(),
                    kind: StageKind::Producer,
                    relu_half: false,
                },
                Staged {
                    output: next,
                    layout: block.second.clone(),
                    kind: StageKind::Residual,
                    relu_half: true,
                },
            ]);
            root = next;
        }
        if schedule.buffers[root].bytes != chain.terminal.input_elements * 2 {
            return Err("selected chain terminal packed input capacity changed".into());
        }
        if let Some(tail) = &chain.branch {
            let mut buffers = vec![chain.output, root];
            for head in &tail.heads {
                let raw = super::branch::pack_weight(graph, head, chain.terminal.input_channels)?;
                let weight = buffer(schedule, raw.len(), true);
                initialized.insert(weight, raw);
                buffers.extend([weight, head.bias]);
            }
            pipeline.push(Call {
                copy: false,
                buffers,
            });
            staged.push(Staged {
                output: chain.output,
                layout: chain.terminal.clone(),
                kind: StageKind::Heads,
                relu_half: false,
            });
        } else {
            let raw = pack_weight(graph, chain.terminal_weight, &chain.terminal)?;
            let weight = buffer(schedule, raw.len(), true);
            initialized.insert(weight, raw);
            pipeline.push(Call {
                copy: false,
                buffers: vec![chain.terminal_output, root, weight, chain.terminal_bias],
            });
            staged.push(Staged {
                output: chain.terminal_output,
                layout: chain.terminal.clone(),
                kind: StageKind::Terminal,
                relu_half: false,
            });
        }
        if let Some(norm) = &chain.normalization {
            pipeline.push(Call {
                copy: false,
                buffers: vec![norm.scalar, chain.terminal_output],
            });
            staged.push(Staged {
                output: norm.scalar,
                layout: chain.terminal.clone(),
                kind: StageKind::Norm,
                relu_half: false,
            });
            if !norm.indexed {
                let divided = if norm.packed {
                    let layout = MatrixLayout::new(
                        &[1, chain.terminal.output_channels],
                        &[chain.terminal.output_channels, 4],
                    )?;
                    let id = buffer(schedule, layout.input_elements * 2, false);
                    scratch.insert(id);
                    packed_edges.insert(chain.output, id);
                    id
                } else {
                    chain.output
                };
                pipeline.push(Call {
                    copy: false,
                    buffers: vec![divided, chain.terminal_output, norm.scalar],
                });
                staged.push(Staged {
                    output: divided,
                    layout: chain.terminal.clone(),
                    kind: if norm.packed {
                        StageKind::PackedDivide
                    } else {
                        StageKind::Divide
                    },
                    relu_half: false,
                });
            }
        }
        let old = schedule
            .calls
            .iter()
            .position(|call| call.buffers[0] == chain.output)
            .ok_or("selected chain terminal call disappeared")?;
        let at = schedule.calls[..old]
            .iter()
            .filter(|call| !chain.owned.contains(&call.buffers[0]))
            .count();
        schedule
            .calls
            .retain(|call| !chain.owned.contains(&call.buffers[0]));
        schedule.calls.splice(at..at, pipeline);
    }
    if !packed_edges.is_empty() {
        return Err("selected normalized packed edge has no closed downstream chain".into());
    }
    Ok(staged)
}
