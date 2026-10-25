//! Conservative serial scheduling of typed plans. This preserves materialization
//! boundaries; original graph fusion/image scheduling is a separate pending stage.
use super::{
    lower::{LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
};
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub struct Schedule {
    pub buffers: Vec<BufferSpec>,
    pub calls: Vec<Call>,
    pub live: BTreeSet<usize>,
}
fn dependencies(expression: &ScalarExpr, ids: &mut Vec<usize>) {
    let mut insert = |id| {
        if !ids.contains(&id) {
            ids.push(id);
        }
    };
    match expression {
        ScalarExpr::Load { plan, .. } | ScalarExpr::PaddedLoad { plan, .. } => insert(*plan),
        ScalarExpr::GatherLoad { plan, axes, .. } => {
            insert(*plan);
            for (e, _, _) in axes {
                dependencies(e, ids);
            }
        }
        ScalarExpr::Cast { value, .. }
        | ScalarExpr::StorageRound { value, .. }
        | ScalarExpr::Reciprocal { value, .. }
        | ScalarExpr::Sqrt { value, .. }
        | ScalarExpr::Exp2 { value, .. }
        | ScalarExpr::Sign { value, .. }
        | ScalarExpr::Trunc { value, .. }
        | ScalarExpr::ReduceSum { value, .. }
        | ScalarExpr::ReduceMax { value, .. }
        | ScalarExpr::GreaterZero(value)
        | ScalarExpr::BoolNot(value) => dependencies(value, ids),
        ScalarExpr::Binary { left, right, .. } => {
            dependencies(left, ids);
            dependencies(right, ids);
        }
        ScalarExpr::Select { condition, yes, no } => {
            dependencies(condition, ids);
            dependencies(yes, ids);
            dependencies(no, ids);
        }
        ScalarExpr::MulAcc { a, b, c, .. } => {
            dependencies(a, ids);
            dependencies(b, ids);
            dependencies(c, ids);
        }
        ScalarExpr::Input(_)
        | ScalarExpr::Constant(_)
        | ScalarExpr::Literal(_)
        | ScalarExpr::FloatLiteral { .. }
        | ScalarExpr::IndexLess { .. } => {}
    }
}
/// This initial schedule emits one call per live materialized expression. It
/// prunes dead plans, holds input/constant/output storage, and rejects a malformed
/// dependency graph. It does not claim original kernel fusion equivalence.
pub fn schedule_plans(
    graph: &LoweredGraph,
    device: &str,
    supports_offset: bool,
) -> Result<Schedule, String> {
    let mut live = BTreeSet::new();
    let mut pending = graph.outputs.values().copied().collect::<Vec<_>>();
    if pending.is_empty() {
        return Err("native kernel schedule requires outputs".into());
    }
    while let Some(id) = pending.pop() {
        let plan = graph
            .plans
            .get(id)
            .ok_or("scheduled output/dependency index out of range")?;
        if !live.insert(id) {
            continue;
        }
        let mut ids = Vec::new();
        dependencies(&plan.expression, &mut ids);
        if ids.iter().any(|v| *v >= id) {
            return Err("typed kernel graph is not topological".into());
        }
        pending.extend(ids);
    }
    let mut buffers = Vec::new();
    let mut calls = Vec::new();
    for (id, plan) in graph.plans.iter().enumerate() {
        let count = plan.ty.shape.iter().try_fold(1usize, |n, v| {
            n.checked_mul(*v).ok_or("scheduled tensor count overflow")
        })?;
        let bytes = count
            .checked_mul(super::parse::width(plan.ty.dtype)?)
            .filter(|v| *v <= 4 * 1024 * 1024 * 1024)
            .ok_or("scheduled tensor exceeds4GiB")?;
        let held = matches!(
            plan.expression,
            ScalarExpr::Input(_) | ScalarExpr::Constant(_)
        ) || graph.outputs.values().any(|v| *v == id);
        if let ScalarExpr::Constant(data) = &plan.expression {
            if data.len() != bytes {
                return Err("scheduled constant byte count mismatch".into());
            }
        }
        buffers.push(BufferSpec {
            device: device.into(),
            bytes,
            held,
            supports_offset,
        });
        if live.contains(&id)
            && !matches!(
                plan.expression,
                ScalarExpr::Input(_) | ScalarExpr::Constant(_)
            )
        {
            let mut args = vec![id];
            dependencies(&plan.expression, &mut args);
            calls.push(Call {
                copy: false,
                buffers: args,
            });
        }
    }
    Ok(Schedule {
        buffers,
        calls,
        live,
    })
}
