//! CPU late multiply/add contraction before schedule and memory planning.
pub(super) mod parallel;
pub(super) mod target;
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    scalar::Binary,
    schedule,
};
fn movement_root(graph: &LoweredGraph, mut id: usize) -> usize {
    while graph.cpu_divisor_views.contains(&id) {
        let parent = match &graph.plans[id].expression {
            ScalarExpr::Load { plan, .. } => Some(*plan),
            ScalarExpr::Cast { value, .. } => match value.as_ref() {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                _ => None,
            },
            _ => None,
        };
        match parent {
            Some(parent) if parent < id => id = parent,
            _ => break,
        }
    }
    id
}
fn supported_binding(
    graph: &LoweredGraph,
    model: &crate::compiler_onnx::Model,
    id: usize,
    output: usize,
) -> bool {
    let mut at = id;
    while graph.cpu_divisor_views.contains(&at) {
        let parent = match &graph.plans[at].expression {
            ScalarExpr::Load { plan, .. } => Some(*plan),
            ScalarExpr::Cast { value, .. } => match value.as_ref() {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                _ => None,
            },
            _ => None,
        };
        let Some(parent) = parent.filter(|p| *p < at) else {
            return false;
        };
        if graph.plans[at].ty.shape.iter().product::<usize>()
            > graph.plans[parent].ty.shape.iter().product::<usize>()
        {
            return false;
        }
        at = parent;
    }
    let root = movement_root(graph, id);
    match &graph.plans[root].expression {
        ScalarExpr::Input(_) => {
            graph.plans[root].ty.dtype == 1
                && graph.plans[id].ty.dtype == 1
                && graph.plans[root].ty.shape.iter().product::<usize>()
                    == graph.plans[output].ty.shape.iter().product::<usize>()
        }
        ScalarExpr::Constant(bytes) => {
            let width = if graph.plans[root].ty.dtype == 10 {
                2
            } else if graph.plans[root].ty.dtype == 1 {
                4
            } else {
                return false;
            };
            let mut first = None;
            let mut uniform = true;
            for chunk in bytes.chunks_exact(width) {
                let Ok(value) =
                    crate::compiler_onnx::scalar::Scalar::read(graph.plans[root].ty.dtype, chunk)
                        .and_then(|v| v.float())
                else {
                    return false;
                };
                if !value.is_finite() {
                    return false;
                }
                if let Some(first) = first {
                    uniform &= value == first;
                } else {
                    first = Some(value);
                }
            }
            first.is_some_and(|value| !(uniform && matches!(value, 0.0 | 1.0 | -1.0)))
        }
        _ => {
            root == id
                && model.graph.nodes.iter().any(|node| {
                    node.op == "Conv"
                        && node
                            .outputs
                            .iter()
                            .any(|name| graph.names.get(name) == Some(&id))
                        && node
                            .inputs
                            .get(1)
                            .and_then(|name| graph.names.get(name))
                            .is_some_and(|weight| {
                                matches!(
                                    graph.plans[movement_root(graph, *weight)].expression,
                                    ScalarExpr::Constant(_)
                                ) && supported_binding(graph, model, *weight, output)
                            })
                })
        }
    }
}
pub(super) fn apply(
    graph: &mut LoweredGraph,
    model: &crate::compiler_onnx::Model,
    kernel_owned: &std::collections::BTreeSet<usize>,
) -> Result<(), String> {
    let schedule = schedule::schedule_plans(graph, "CPU", true)?;
    let mut uses = vec![0usize; graph.plans.len()];
    for call in &schedule.calls {
        for &id in &call.buffers[1..] {
            uses[id] += 1;
        }
    }
    let mut held = graph
        .outputs
        .values()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for id in (0..graph.plans.len()).rev() {
        if !held.contains(&id) {
            continue;
        }
        match &graph.plans[id].expression {
            ScalarExpr::Load { plan, .. }
                if graph.plans[*plan].ty.dtype == graph.plans[id].ty.dtype =>
            {
                held.insert(*plan);
            }
            ScalarExpr::Cast { dtype, value } if *dtype == graph.plans[id].ty.dtype => {
                if let ScalarExpr::Load { plan, .. } = value.as_ref() {
                    if graph.plans[*plan].ty.dtype == *dtype {
                        held.insert(*plan);
                    }
                }
            }
            _ => {}
        }
    }
    // A selected LLVM closure owns its complete arithmetic/DAG schedule.
    // Serial C contraction must neither rewrite nor reject those owned plans.
    for id in 0..graph.plans.len() {
        if !schedule.live.contains(&id)
            || graph.plans[id].ty.dtype != 1
            || kernel_owned.contains(&id)
        {
            continue;
        }
        let ScalarExpr::Binary {
            kind: Binary::Add,
            dtype: 1,
            left,
            right,
        } = &graph.plans[id].expression
        else {
            continue;
        };
        let load_plan = |value: &ScalarExpr| match value {
            ScalarExpr::Load { plan, .. } => Some(*plan),
            ScalarExpr::StorageRound { dtype: 10, value } => match value.as_ref() {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                _ => None,
            },
            _ => None,
        };
        if let (Some(residual), Some(convolution)) = (load_plan(left), load_plan(right)) {
            if model.graph.nodes.iter().any(|node| {
                node.op == "Conv"
                    && node.inputs.first().and_then(|name| graph.names.get(name)) == Some(&residual)
                    && node
                        .outputs
                        .iter()
                        .any(|name| graph.names.get(name) == Some(&convolution))
            }) {
                return Err("CPU swapped IMAGE residual requires source fusion scheduling".into());
            }
        }
        // Original Conv residual arithmetic is in the final accumulator kernel,
        // so the convolution operand retains first-operand NaN priority even
        // when the residual's IMAGE buffer is reused or externally held.
        let residual_plan = match right.as_ref() {
            ScalarExpr::Load { plan, .. } => Some(*plan),
            ScalarExpr::StorageRound { dtype: 10, value } => match value.as_ref() {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                _ => None,
            },
            _ => None,
        };
        if let (ScalarExpr::Load { plan: conv, .. }, Some(residual)) =
            (left.as_ref(), residual_plan)
        {
            if model.graph.nodes.iter().any(|node| {
                node.op == "Conv"
                    && node.inputs.first().and_then(|name| graph.names.get(name)) == Some(&residual)
                    && node
                        .outputs
                        .iter()
                        .any(|name| graph.names.get(name) == Some(conv))
            }) {
                if let ScalarExpr::Binary {
                    kind: Binary::Mul,
                    dtype: 1,
                    left: a,
                    right: b,
                } = &graph.plans[residual].expression
                {
                    let direct = |value: &ScalarExpr| match value {
                        ScalarExpr::Load { plan, .. } => {
                            matches!(
                                graph.plans[movement_root(graph, *plan)].expression,
                                ScalarExpr::Input(_) | ScalarExpr::Constant(_)
                            ) && supported_binding(graph, model, *plan, id)
                        }
                        _ => false,
                    };
                    if !direct(a) || !direct(b) || !supported_binding(graph, model, *conv, id) {
                        return Err("CPU computed or folded IMAGE residual requires source fusion scheduling".into());
                    }
                    graph.cpu_ordered_image_adds.insert(id);
                }
            }
        }
        let mut replacement = None;
        for (product, addend) in [(left, right), (right, left)] {
            let ScalarExpr::Load { plan, index } = product.as_ref() else {
                continue;
            };
            let mut root = *plan;
            while root < id {
                let parent = match &graph.plans[root].expression {
                    ScalarExpr::Load { plan, .. } => Some(*plan),
                    ScalarExpr::Cast { dtype: 1, value } => match value.as_ref() {
                        ScalarExpr::Load { plan, .. } => Some(*plan),
                        _ => None,
                    },
                    _ => None,
                };
                match parent {
                    Some(parent) if parent < root => root = parent,
                    _ => break,
                }
            }
            if root != *plan
                && matches!(&graph.plans[root].expression,ScalarExpr::Binary{kind:Binary::Mul,dtype:1,left,right} if matches!(left.as_ref(),ScalarExpr::Load{..})&&matches!(right.as_ref(),ScalarExpr::Load{..}))
            {
                return Err("CPU moved multiply/add requires source fusion scheduling".into());
            }
            if *plan >= id
                || graph.plans[*plan].ty != graph.plans[id].ty
                || *index
                    != crate::compiler_onnx::lower::broadcast_index(
                        &graph.plans[id].ty.shape,
                        &graph.plans[id].ty.shape,
                    )
            {
                continue;
            }
            let ScalarExpr::Binary {
                kind: Binary::Mul,
                dtype: 1,
                left: a,
                right: b,
            } = &graph.plans[*plan].expression
            else {
                continue;
            };
            // Reciprocal, cast and storage-round expressions have separate observable
            // lowering rules. This unit contracts only direct buffer operands.
            if !matches!(a.as_ref(), ScalarExpr::Load { .. })
                || !matches!(b.as_ref(), ScalarExpr::Load { .. })
            {
                continue;
            }
            // A direct IMAGE convolution consumes this source before its residual
            // Add. Original unheld epilogues recompute the multiply; a held
            // source is loaded instead. Other shared consumers remain pending.
            let image_residual = if let ScalarExpr::Load {
                plan: convolution, ..
            } = addend.as_ref()
            {
                addend.as_ref() == left.as_ref()
                    && uses[*plan] == 2
                    && model.graph.nodes.iter().any(|node| {
                        node.op == "Conv"
                            && node.inputs.first().and_then(|name| graph.names.get(name))
                                == Some(plan)
                            && node
                                .outputs
                                .iter()
                                .any(|name| graph.names.get(name) == Some(convolution))
                    })
            } else {
                false
            };
            if held.contains(plan) && image_residual {
                graph.cpu_ordered_image_adds.insert(id);
                continue;
            }
            if held.contains(plan) {
                return Err("CPU held multiply/add requires source fusion scheduling".into());
            }
            if uses[*plan] != 1 && !image_residual {
                return Err(
                    "CPU shared multiply contraction requires source fusion scheduling".into(),
                );
            }
            let ScalarExpr::Load { plan: a_plan, .. } = a.as_ref() else {
                unreachable!()
            };
            let ScalarExpr::Load { plan: b_plan, .. } = b.as_ref() else {
                unreachable!()
            };
            let addend_plan = match addend.as_ref() {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                ScalarExpr::StorageRound { value, .. } => match value.as_ref() {
                    ScalarExpr::Load { plan, .. } => Some(*plan),
                    _ => None,
                },
                _ => None,
            };
            let a_root = movement_root(graph, *a_plan);
            let b_root = movement_root(graph, *b_plan);
            let c_root = addend_plan.map(|p| movement_root(graph, p));
            let quantized_c_supported =
                matches!(addend.as_ref(), ScalarExpr::StorageRound { dtype: 10, .. })
                    && addend_plan.is_some_and(|p| {
                        supported_binding(graph, model, p, id)
                            || graph.cpu_contractions.contains(&p)
                            || graph.cpu_ordered_image_adds.contains(&p)
                    });
            if !supported_binding(graph, model, *a_plan, id)
                || !supported_binding(graph, model, *b_plan, id)
                || match addend.as_ref() {
                    ScalarExpr::StorageRound { dtype: 10, .. } => !quantized_c_supported,
                    _ => !addend_plan.is_some_and(|p| supported_binding(graph, model, p, id)),
                }
            {
                return Err(
                    "CPU computed or folded multiply/add requires source fusion scheduling".into(),
                );
            }
            if a_root == b_root || c_root.is_some_and(|p| p == a_root || p == b_root) {
                return Err("CPU aliased multiply/add requires source fusion scheduling".into());
            }
            let conv = |plan: usize| {
                model.graph.nodes.iter().any(|node| {
                    node.op == "Conv"
                        && node
                            .outputs
                            .iter()
                            .any(|name| graph.names.get(name) == Some(&plan))
                })
            };
            let has_conv = conv(a_root) || conv(b_root) || c_root.is_some_and(conv);
            let scaled_conv_residual = quantized_c_supported
                && ((conv(a_root)
                    && matches!(graph.plans[b_root].expression, ScalarExpr::Constant(_)))
                    || (conv(b_root)
                        && matches!(graph.plans[a_root].expression, ScalarExpr::Constant(_))));
            if has_conv && !image_residual && !scaled_conv_residual {
                return Err(
                    "CPU computed convolution multiply/add requires source fusion scheduling"
                        .into(),
                );
            }
            if a_plan == b_plan
                || addend_plan.is_some_and(|p| p == *a_plan || p == *b_plan || p == *plan)
            {
                return Err("CPU aliased multiply/add requires source fusion scheduling".into());
            }
            replacement = Some(ScalarExpr::MulAcc {
                dtype: 1,
                a: a.clone(),
                b: b.clone(),
                c: addend.clone(),
            });
            break;
        }
        if let Some(expression) = replacement {
            graph.plans[id].expression = expression;
            graph.cpu_contractions.insert(id);
        }
    }
    Ok(())
}
