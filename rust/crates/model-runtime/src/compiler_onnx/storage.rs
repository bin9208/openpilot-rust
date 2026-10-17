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
// Identity and same-dtype Cast disappear in the original graph before its
// moved-AFTER fold. Keep their native plan storage observable, but track their
// identity in both directions before admitting arithmetic consumers.
fn view_parent(graph: &LoweredGraph, id: usize) -> Option<usize> {
    let plan = &graph.plans[id];
    match &plan.expression {
        ScalarExpr::Load { plan: parent, .. } => Some(*parent),
        ScalarExpr::Cast { dtype, value } if *dtype == plan.ty.dtype => match value.as_ref() {
            ScalarExpr::Load { plan: parent, .. }
                if graph.plans.get(*parent)?.ty.dtype == *dtype =>
            {
                Some(*parent)
            }
            _ => None,
        },
        _ => None,
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
    // Original Tensor.cat pads before its addition. That PAD is a barrier to
    // moved-AFTER IMAGE alias reuse; lowered Select loads are not direct ALU
    // reads of the packed source. Keep this source distinction explicit.
    let concat_barriers = model
        .graph
        .nodes
        .iter()
        .filter(|node| node.op == "Concat")
        .flat_map(|node| node.outputs.iter())
        .filter_map(|name| graph.names.get(name).copied())
        .collect::<BTreeSet<_>>();
    let views = (0..graph.plans.len())
        .map(|id| view_parent(graph, id))
        .collect::<Vec<_>>();
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
        if views[id].is_some() {
            let mut source = id;
            unsupported_views.insert(source);
            while let Some(parent) = views[source] {
                if parent >= source {
                    return Err("invalid inverse movement alias order".into());
                }
                source = parent;
                unsupported_views.insert(source);
            }
            continue;
        }
        aliases.insert(id);
    }
    // IMAGE matmul is lowered through packed convolution by the original. Its
    // shared computed operands need a separate packing/inverse-map proof. Keep
    // isolated matrix execution available while rejecting unproved ALU reuse.
    for node in model
        .graph
        .nodes
        .iter()
        .filter(|node| matches!(node.op.as_str(), "MatMul" | "Gemm"))
    {
        if !node
            .outputs
            .iter()
            .filter_map(|name| graph.names.get(name))
            .any(|id| live.contains(id))
        {
            continue;
        }
        for key in ["alpha", "beta"] {
            if let Some(attribute) = node.attributes.get(key) {
                if !matches!(attribute,Attribute::Float(value) if *value==1.0)
                    && !matches!(attribute, Attribute::Integer(1))
                {
                    return Err("CPU IMAGE nonunit Gemm scaling is not implemented".into());
                }
            }
        }
        let weight = node
            .inputs
            .get(1)
            .and_then(|name| graph.names.get(name))
            .ok_or("matrix weight has no storage plan")?;
        if !matches!(graph.plans[*weight].expression, ScalarExpr::Constant(_)) {
            return Err("CPU IMAGE dynamic matrix weight storage alias is not implemented".into());
        }
        let input = node
            .inputs
            .first()
            .and_then(|name| graph.names.get(name))
            .ok_or("matrix input has no storage plan")?;
        for id in [*input, *weight] {
            let ty = &graph.plans[id].ty;
            if ty.shape.len() != 2 || !matches!(ty.dtype, 1 | 10) {
                return Err("CPU IMAGE matrix requires f32/half rank-two operands".into());
            }
        }
        let mut source = *input;
        unsupported_views.insert(source);
        while let Some(parent) = views[source] {
            if parent >= source {
                return Err("invalid inverse movement alias order".into());
            }
            source = parent;
            unsupported_views.insert(source);
        }
    }
    // A view downstream of a packed root can hide that root from the ALU load.
    // Propagate the identity closure in topological order; conservatively reject
    // such reuse until inverse view maps are proved, including otherwise pure
    // same-dtype Casts. Held view outputs still copy the original raw storage.
    let mut identities = aliases
        .union(&unsupported_views)
        .copied()
        .collect::<BTreeSet<_>>();
    for (id, parent) in views.iter().enumerate() {
        if parent.is_some_and(|parent| identities.contains(&parent)) {
            identities.insert(id);
            unsupported_views.insert(id);
        }
    }
    // Mixed Concat proof covers physical/virtual half storage, not inverse
    // IMAGE matrix movement identities. Keep half operand reuse fenced even
    // though the ordinary f32 Concat PAD barrier is independently established.
    for node in model.graph.nodes.iter().filter(|node| node.op == "Concat") {
        if !node
            .outputs
            .iter()
            .filter_map(|name| graph.names.get(name))
            .any(|id| live.contains(id))
        {
            continue;
        }
        if node
            .inputs
            .iter()
            .filter_map(|name| graph.names.get(name))
            .any(|id| graph.plans[*id].ty.dtype == 10 && unsupported_views.contains(id))
        {
            return Err("CPU IMAGE mixed half Concat operand alias is not implemented".into());
        }
    }
    for (id, plan) in graph.plans.iter().enumerate() {
        if live.contains(&id) && views[id].is_none() && !concat_barriers.contains(&id) {
            let mut probe = plan.expression.clone();
            rewrite(&mut probe, &unsupported_views, false);
            if probe != plan.expression {
                return Err(
                    "CPU IMAGE shared view storage requires an inverse movement alias map".into(),
                );
            }
        }
    }
    for (id, plan) in graph.plans.iter_mut().enumerate() {
        if views[id].is_none() && !concat_barriers.contains(&id) {
            rewrite(&mut plan.expression, &aliases, false);
        }
    }
    Ok(())
}

/// Fence the independently observed CPU Conv/Gelu/Mean fusion discrepancy.
/// A separately materialized Gelu output has its own verified mean contract;
/// otherwise an observable f32 path must cross explicit storage conversion.
/// This is conservative admission, not an implementation of the fused schedule
/// or an analytic proof that quantization masks differences for every input.
pub fn admit_cpu_observable_fusion(graph: &LoweredGraph, model: &Model) -> Result<(), String> {
    let mut held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    for id in (0..graph.plans.len()).rev() {
        if held.contains(&id) {
            if let Some(parent) = view_parent(graph, id) {
                held.insert(parent);
            }
        }
    }
    let convs = model
        .graph
        .nodes
        .iter()
        .filter(|node| node.op == "Conv")
        .flat_map(|node| node.outputs.iter())
        .filter_map(|name| graph.names.get(name))
        .copied()
        .collect::<BTreeSet<_>>();
    // Original symbolic passes erase identity ALU as well as static views.
    // Search unquantized scalar ancestry rather than requiring adjacent source
    // operator spellings; real interposed f32 ALU is likewise not proved fused.
    let unproved_gelus = model
        .graph
        .nodes
        .iter()
        .filter(|node| node.op == "Gelu")
        .filter(|node| {
            node.inputs
                .iter()
                .filter_map(|name| graph.names.get(name))
                .any(|id| plan(*id, graph, &convs, &mut BTreeSet::new()))
        })
        .flat_map(|node| node.outputs.iter())
        .filter_map(|name| graph.names.get(name))
        .filter(|id| graph.plans[**id].ty.dtype == 1 && !held.contains(id))
        .copied()
        .collect::<BTreeSet<_>>();
    let risky = model
        .graph
        .nodes
        .iter()
        .filter(|node| node.op == "ReduceMean")
        .filter(|node| {
            node.inputs
                .iter()
                .filter_map(|name| graph.names.get(name))
                .any(|id| plan(*id, graph, &unproved_gelus, &mut BTreeSet::new()))
        })
        .flat_map(|node| node.outputs.iter())
        .filter_map(|name| graph.names.get(name))
        .filter(|id| graph.plans[**id].ty.dtype == 1)
        .copied()
        .collect::<BTreeSet<_>>();
    fn plan(
        id: usize,
        graph: &LoweredGraph,
        risky: &BTreeSet<usize>,
        seen: &mut BTreeSet<usize>,
    ) -> bool {
        if risky.contains(&id) {
            return true;
        }
        if graph.plans[id].ty.dtype == 10 || !seen.insert(id) {
            return false;
        }
        expression(&graph.plans[id].expression, graph, risky, seen)
    }
    fn expression(
        value: &ScalarExpr,
        graph: &LoweredGraph,
        risky: &BTreeSet<usize>,
        seen: &mut BTreeSet<usize>,
    ) -> bool {
        match value {
            ScalarExpr::Load { plan: id, .. } | ScalarExpr::PaddedLoad { plan: id, .. } => {
                plan(*id, graph, risky, seen)
            }
            ScalarExpr::GatherLoad { plan: id, axes, .. } => {
                plan(*id, graph, risky, seen)
                    || axes
                        .iter()
                        .any(|(e, _, _)| expression(e, graph, risky, seen))
            }
            ScalarExpr::StorageRound { dtype: 10, .. } => false,
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
            | ScalarExpr::BoolNot(value) => expression(value, graph, risky, seen),
            ScalarExpr::Binary { left, right, .. } => {
                expression(left, graph, risky, seen) || expression(right, graph, risky, seen)
            }
            ScalarExpr::MulAcc { a, b, c, .. } => {
                [a, b, c].iter().any(|v| expression(v, graph, risky, seen))
            }
            ScalarExpr::Select { condition, yes, no } => [condition, yes, no]
                .iter()
                .any(|v| expression(v, graph, risky, seen)),
            ScalarExpr::Input(_)
            | ScalarExpr::Constant(_)
            | ScalarExpr::Literal(_)
            | ScalarExpr::FloatLiteral { .. }
            | ScalarExpr::IndexLess { .. } => false,
        }
    }
    if graph
        .outputs
        .values()
        .any(|id| graph.plans[*id].ty.dtype == 1 && plan(*id, graph, &risky, &mut BTreeSet::new()))
    {
        return Err("CPU observable Conv/Gelu/ReduceMean fusion is not implemented".into());
    }
    Ok(())
}
