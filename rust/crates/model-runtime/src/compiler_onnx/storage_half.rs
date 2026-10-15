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
fn admit(graph: &LoweredGraph, held: &[bool]) -> Result<(), String> {
    let live = schedule::schedule_plans(graph, "CPU", true)?.live;
    let mut exact = vec![false; graph.plans.len()];
    for (id, plan) in graph.plans.iter().enumerate() {
        if plan.ty.dtype == 10 {
            exact[id] = held[id] && virtual_expression(&plan.expression)
                || match &plan.expression {
                    ScalarExpr::Input(_)
                    | ScalarExpr::Constant(_)
                    | ScalarExpr::StorageRound { dtype: 10, .. } => true,
                    ScalarExpr::Cast { dtype: 10, value } => match value.as_ref() {
                        ScalarExpr::Load { plan: source, .. }
                            if graph.plans[*source].ty.dtype == 2 =>
                        {
                            true
                        }
                        _ => identity_half_view(&plan.expression, graph)
                            .is_some_and(|source| exact[source]),
                    },
                    _ => identity_half_view(&plan.expression, graph)
                        .is_some_and(|source| exact[source]),
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

// Only these expression families have independent CPU logical-half captures.
// General reductions and transcendental half operations
// do not acquire a new admission path through this storage projection.
fn virtual_expression(value: &ScalarExpr) -> bool {
    use super::scalar::Binary;
    match value {
        ScalarExpr::Load { .. } | ScalarExpr::Literal(_) | ScalarExpr::FloatLiteral { .. } => true,
        ScalarExpr::Cast {
            dtype: 1 | 10,
            value,
        }
        | ScalarExpr::Reciprocal { dtype: 10, value } => virtual_expression(value),
        ScalarExpr::Select { condition, yes, no }
            if matches!(condition.as_ref(), ScalarExpr::IndexLess { .. }) =>
        {
            virtual_expression(yes) && virtual_expression(no)
        }
        ScalarExpr::Binary {
            kind: Binary::Add | Binary::Sub | Binary::Mul,
            dtype: 10,
            left,
            right,
        } => virtual_expression(left) && virtual_expression(right),
        _ => false,
    }
}
/// A CPU storage projection, separate from the logical graph used by ONNX
/// validation and IMAGE alias analysis. The expression dtypes remain logical:
/// half casts clamp, whereas the projected buffer dtype controls conversion at
/// actual loads/stores. Inputs, constants, selected half outputs and their view
/// ancestors retain physical half storage.
pub(crate) fn physical_graph(logical: &LoweredGraph) -> Result<LoweredGraph, String> {
    let mut held = vec![false; logical.plans.len()];
    for &id in logical.outputs.values() {
        held[id] = logical.plans[id].ty.dtype == 10;
    }
    for id in (0..logical.plans.len()).rev() {
        if held[id] {
            if let Some(source) = identity_half_view(&logical.plans[id].expression, logical) {
                held[source] = true;
            }
        }
    }
    let mut physical = logical.clone();
    for (id, plan) in logical.plans.iter().enumerate() {
        if plan.ty.dtype == 10 && !held[id] && virtual_expression(&plan.expression) {
            physical.plans[id].ty.dtype = 1;
        }
    }
    // CPU's physical-half PAD+ADD elimination depends on optimized movement
    // context. Its -0/sNaN effects are not reproduced by a universal +0. Admit
    // exact uint8-derived arms and unheld virtual arms; reject ambiguous half
    // storage contexts before rendering rather than guessing their arithmetic.
    fn concat_sources(value: &ScalarExpr, accepted: &[bool]) -> bool {
        match value {
            ScalarExpr::Select { condition, yes, no }
                if matches!(condition.as_ref(), ScalarExpr::IndexLess { .. }) =>
            {
                concat_sources(yes, accepted) && concat_sources(no, accepted)
            }
            ScalarExpr::Load { plan, .. } => accepted[*plan],
            _ => false,
        }
    }
    let mut physical_half = vec![false; logical.plans.len()];
    let mut exact_u8 = vec![false; logical.plans.len()];
    for (id, plan) in logical.plans.iter().enumerate() {
        let view = identity_half_view(&plan.expression, logical);
        physical_half[id] = physical.plans[id].ty.dtype == 10
            || plan.ty.dtype == 10 && view.is_some_and(|source| physical_half[source]);
        if plan.ty.dtype == 10 {
            exact_u8[id] = view.is_some_and(|source| exact_u8[source])
                || match &plan.expression {
                    ScalarExpr::Cast { dtype: 10, value } => matches!(value.as_ref(),
                        ScalarExpr::Load { plan: source, .. } if logical.plans[*source].ty.dtype == 2),
                    ScalarExpr::Select { .. } => concat_sources(&plan.expression, &exact_u8),
                    _ => false,
                };
        }
    }
    let safe_arms = physical_half
        .iter()
        .zip(&exact_u8)
        .map(|(physical, exact)| !physical || *exact)
        .collect::<Vec<_>>();
    for (id, plan) in logical.plans.iter().enumerate() {
        if plan.ty.dtype == 10
            && matches!(plan.expression, ScalarExpr::Select { .. })
            && (!concat_sources(&plan.expression, &safe_arms)
                || physical.plans[id].ty.dtype == 10 && !exact_u8[id])
        {
            return Err("CPU physical-half Concat movement context is not implemented".into());
        }
    }
    // Preserve the existing rejection for unproved physical-half edges. This
    // projection is not permission to compile arbitrary half scalar IR.
    admit(&physical, &held)?;
    Ok(physical)
}
