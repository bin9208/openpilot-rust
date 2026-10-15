//! CPU IMAGE storage reuse, derived from tinygrad schedule/rangeify.py's
//! found_after/pm_fold_moved_after (MIT). This changes ALU consumers only;
//! externally held inputs and observable tensor outputs keep their own storage.
use super::{
    lower::{LoweredGraph, ScalarExpr},
    Attribute, Model,
};
use std::collections::BTreeSet;
fn rewrite(expression: &mut ScalarExpr, aliases: &BTreeSet<usize>, alu: bool) {
    match expression {
        ScalarExpr::Load { plan, .. } if alu && aliases.contains(plan) => {
            *expression = ScalarExpr::StorageRound {
                dtype: 10,
                value: Box::new(expression.clone()),
            };
        }
        ScalarExpr::Binary { left, right, .. } => {
            rewrite(left, aliases, true);
            rewrite(right, aliases, true);
        }
        ScalarExpr::MulAcc { a, b, c, .. } => {
            rewrite(a, aliases, true);
            rewrite(b, aliases, true);
            rewrite(c, aliases, true);
        }
        ScalarExpr::Cast { value, .. }
        | ScalarExpr::Reciprocal { value, .. }
        | ScalarExpr::Sqrt { value, .. }
        | ScalarExpr::Exp2 { value, .. }
        | ScalarExpr::Sign { value, .. }
        | ScalarExpr::Trunc { value, .. }
        | ScalarExpr::GreaterZero(value)
        | ScalarExpr::BoolNot(value) => rewrite(value, aliases, true),
        ScalarExpr::StorageRound { .. } => {}
        ScalarExpr::ReduceSum { value, .. } | ScalarExpr::ReduceMax { value, .. } => {
            rewrite(value, aliases, false)
        }
        ScalarExpr::Select { condition, yes, no } => {
            rewrite(condition, aliases, true);
            rewrite(yes, aliases, true);
            rewrite(no, aliases, true);
        }
        ScalarExpr::GatherLoad { axes, .. } => {
            for (value, _, _) in axes {
                rewrite(value, aliases, false);
            }
        }
        _ => {}
    }
}
/// Reuse only computed f32 tensors with invertible full-channel IMAGE packing.
/// Channel zero-padding is an ordinary PAD and is not stripped by the original
/// fold; alignment's Invalid padding is stripped. Negative crop is noninvertible.
/// Existing explicit StorageRound staging remains intact and inputs/constants
/// never acquire a different externally visible storage identity.
pub fn apply_cpu_image_aliases(graph: &mut LoweredGraph, model: &Model) -> Result<(), String> {
    let mut aliases = BTreeSet::new();
    let mut unsupported_views = BTreeSet::new();
    let live = super::schedule::schedule_plans(graph, "CPU", true)?.live;
    for node in model.graph.nodes.iter().filter(|node| node.op == "Conv") {
        if !node
            .outputs
            .iter()
            .filter_map(|name| graph.names.get(name))
            .any(|id| live.contains(id))
        {
            continue;
        }
        let weight = node
            .inputs
            .get(1)
            .and_then(|name| graph.names.get(name))
            .ok_or("Conv missing weight storage plan")?;
        if !matches!(graph.plans[*weight].expression, ScalarExpr::Constant(_)) {
            return Err("CPU IMAGE dynamic weight storage alias is not implemented".into());
        }
        let name = node
            .inputs
            .first()
            .ok_or("Conv missing input for storage planning")?;
        let id = *graph
            .names
            .get(name)
            .ok_or("Conv storage input has no plan")?;
        let plan = &graph.plans[id];
        if plan.ty.dtype != 1
            || plan.ty.shape.len() != 4
            || matches!(
                plan.expression,
                ScalarExpr::Input(_) | ScalarExpr::Constant(_)
            )
        {
            continue;
        }
        let groups = match node.attributes.get("group") {
            Some(Attribute::Integer(v)) => {
                usize::try_from(*v).map_err(|_| "invalid Conv storage group")?
            }
            None => 1,
            _ => return Err("invalid Conv storage group".into()),
        };
        if groups == 0 || !plan.ty.shape[1].is_multiple_of(groups) {
            return Err("invalid Conv storage channels".into());
        }
        let cin = plan.ty.shape[1] / groups;
        if !(cin.is_multiple_of(4) || cin == 1 && groups.is_multiple_of(4)) {
            continue;
        }
        let explicit_padding = match node.attributes.get("auto_pad") {
            None => true,
            Some(Attribute::String(value)) => value == "NOTSET",
            _ => return Err("invalid Conv auto_pad storage policy".into()),
        };
        if explicit_padding
            && matches!(node.attributes.get("pads"),Some(Attribute::Integers(pads))if pads.iter().any(|v|*v<0))
        {
            continue;
        }
        // Movement sources require an inverse index map to the underlying computed
        // value. Do not silently treat a noninvertible view as packed value storage.
        if matches!(plan.expression, ScalarExpr::Load { .. }) {
            let mut source = id;
            unsupported_views.insert(source);
            while let ScalarExpr::Load { plan: parent, .. } = &graph.plans[source].expression {
                if *parent >= source {
                    return Err("invalid inverse movement alias order".into());
                }
                source = *parent;
                unsupported_views.insert(source);
            }
            continue;
        }
        aliases.insert(id);
    }
    for (id, plan) in graph.plans.iter().enumerate() {
        if live.contains(&id) {
            let mut probe = plan.expression.clone();
            rewrite(&mut probe, &unsupported_views, false);
            if probe != plan.expression {
                return Err(
                    "CPU IMAGE shared view storage requires an inverse movement alias map".into(),
                );
            }
        }
    }
    for plan in &mut graph.plans {
        rewrite(&mut plan.expression, &aliases, false);
    }
    Ok(())
}
