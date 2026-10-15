//! Admission for CPU:LLVM logical half arithmetic versus physical half buffers.
//! Original CPU emulation keeps half arithmetic in float until storage. Serial
//! native plans cannot introduce an extra half store before math/float export.
use super::{
    lower::{LoweredGraph, ScalarExpr},
    schedule,
};
fn identity_half_view(expression: &ScalarExpr, graph: &LoweredGraph) -> Option<usize> {
    match expression {
        ScalarExpr::Load { plan, .. } if graph.plans[*plan].ty.dtype == 10 => Some(*plan),
        ScalarExpr::Cast { dtype: 10, value } => match value.as_ref() {
            ScalarExpr::Load { plan, .. } if graph.plans[*plan].ty.dtype == 10 => Some(*plan),
            _ => None,
        },
        _ => None,
    }
}
fn unsafe_read(
    expression: &ScalarExpr,
    graph: &LoweredGraph,
    exact: &[bool],
    quantized: bool,
) -> bool {
    let load = |id: usize| !quantized && graph.plans[id].ty.dtype == 10 && !exact[id];
    match expression {
        ScalarExpr::Load { plan, .. } | ScalarExpr::PaddedLoad { plan, .. } => load(*plan),
        ScalarExpr::GatherLoad { plan, axes, .. } => {
            load(*plan)
                || axes
                    .iter()
                    .any(|(v, _, _)| unsafe_read(v, graph, exact, false))
        }
        ScalarExpr::StorageRound { dtype: 10, value } => unsafe_read(value, graph, exact, true),
        ScalarExpr::Cast { dtype, value } => {
            unsafe_read(value, graph, exact, quantized && matches!(dtype, 1 | 10))
        }
        ScalarExpr::StorageRound { value, .. }
        | ScalarExpr::Reciprocal { value, .. }
        | ScalarExpr::Sqrt { value, .. }
        | ScalarExpr::Exp2 { value, .. }
        | ScalarExpr::Sign { value, .. }
        | ScalarExpr::Trunc { value, .. }
        | ScalarExpr::ReduceSum { value, .. }
        | ScalarExpr::ReduceMax { value, .. }
        | ScalarExpr::GreaterZero(value)
        | ScalarExpr::BoolNot(value) => unsafe_read(value, graph, exact, false),
        ScalarExpr::Binary { left, right, .. } => {
            unsafe_read(left, graph, exact, false) || unsafe_read(right, graph, exact, false)
        }
        ScalarExpr::MulAcc { a, b, c, .. } => [a, b, c]
            .iter()
            .any(|v| unsafe_read(v, graph, exact, false)),
        ScalarExpr::Select { condition, yes, no } => [condition, yes, no]
            .iter()
            .any(|v| unsafe_read(v, graph, exact, false)),
        ScalarExpr::Input(_)
        | ScalarExpr::Constant(_)
        | ScalarExpr::Literal(_)
        | ScalarExpr::FloatLiteral { .. }
        | ScalarExpr::IndexLess { .. } => false,
    }
}
/// Accept physical F16 bindings/constants, exact uint8 conversions and direct
/// explicit IMAGE storage boundaries. Defer unproved logical-half materializing
/// paths rather than publishing rounded results for float exports or later ALU.
pub(crate) fn admit(graph: &LoweredGraph) -> Result<(), String> {
    let live = schedule::schedule_plans(graph, "CPU", true)?.live;
    let mut exact = vec![false; graph.plans.len()];
    for (id, plan) in graph.plans.iter().enumerate() {
        if plan.ty.dtype == 10 {
            exact[id] = match &plan.expression {
                ScalarExpr::Input(_)
                | ScalarExpr::Constant(_)
                | ScalarExpr::StorageRound { dtype: 10, .. } => true,
                ScalarExpr::Cast { dtype: 10, value } => match value.as_ref() {
                    ScalarExpr::Load { plan: source, .. } if graph.plans[*source].ty.dtype == 2 => {
                        true
                    }
                    _ => identity_half_view(&plan.expression, graph)
                        .is_some_and(|source| exact[source]),
                },
                _ => {
                    identity_half_view(&plan.expression, graph).is_some_and(|source| exact[source])
                }
            };
        }
        if live.contains(&id)
            && !(plan.ty.dtype == 10 && identity_half_view(&plan.expression, graph).is_some())
            && unsafe_read(&plan.expression, graph, &exact, false)
        {
            return Err("CPU logical-half materialization before arithmetic or float export is not implemented".into());
        }
    }
    Ok(())
}
