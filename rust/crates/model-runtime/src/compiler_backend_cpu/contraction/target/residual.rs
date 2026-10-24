//! Existing admitted scaled Conv residual: preserve its explicit half skip and
//! source Add order while delegating contraction to the configured LLVM target.
use crate::compiler_onnx::{
    lower::{broadcast_index, IndexExpr, LoweredGraph, ScalarExpr},
    schedule::Schedule,
    Model,
};
use std::{collections::BTreeMap, fmt::Write};
type PipelineCall = crate::compiler_backend_cpu::matrix::graph::PipelineCall;
fn load(value: &ScalarExpr) -> Option<(usize, &IndexExpr)> {
    match value {
        ScalarExpr::Load { plan, index } => Some((*plan, index)),
        _ => None,
    }
}
pub(super) fn scheduled(
    graph: &LoweredGraph,
    model: &Model,
    schedule: &Schedule,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let mut selected = BTreeMap::new();
    for (call_index, call) in schedule.calls.iter().enumerate() {
        let output = call.buffers[0];
        if !graph.cpu_contractions.contains(&output) || call.buffers.len() != 4 {
            continue;
        }
        let target = &graph.plans[output];
        let ScalarExpr::MulAcc { dtype: 1, a, b, c } = &target.expression else {
            continue;
        };
        let ScalarExpr::StorageRound { dtype: 10, value } = c.as_ref() else {
            continue;
        };
        let Some((skip, skip_index)) = load(value) else {
            continue;
        };
        let Some((pre, pre_index)) = load(a) else {
            continue;
        };
        let Some((gamma, gamma_index)) = load(b) else {
            continue;
        };
        let shape = &target.ty.shape;
        if target.ty.dtype != 1
            || shape.len() != 4
            || shape[0] != 1
            || !shape[1].is_multiple_of(4)
            || graph.plans[pre].ty != target.ty
            || graph.plans[skip].ty != target.ty
            || graph.plans[gamma].ty.dtype != 10
            || graph.plans[gamma].ty.shape != [shape[1], 1, 1]
            || !matches!(graph.plans[gamma].expression, ScalarExpr::Constant(_))
            || pre_index != &broadcast_index(shape, shape)
            || skip_index != &broadcast_index(shape, shape)
            || gamma_index != &broadcast_index(&graph.plans[gamma].ty.shape, shape)
        {
            continue;
        }
        let Some(add) = model.graph.nodes.iter().find(|n| {
            n.op == "Add" && n.outputs.len() == 1 && graph.names.get(&n.outputs[0]) == Some(&output)
        }) else {
            continue;
        };
        if add.inputs.len() != 2 {
            continue;
        }
        let Some(skip_position) = add
            .inputs
            .iter()
            .position(|name| graph.names.get(name) == Some(&skip))
        else {
            continue;
        };
        let count = shape.iter().try_fold(1usize, |n, &d| n.checked_mul(d));
        let Some(_) = count.filter(|&n| n > 0 && n <= i32::MAX as usize / 4) else {
            return Err("configured residual exceeds bounded LLVM index/storage range".into());
        };
        let spatial = shape[2]
            .checked_mul(shape[3])
            .ok_or("residual spatial overflow")?;
        if spatial == 0 {
            return Err("empty configured residual".into());
        }
        let slot = |id| {
            call.buffers
                .iter()
                .position(|&v| v == id)
                .ok_or("configured residual missing scheduled input")
        };
        let (pre_slot, gamma_slot, skip_slot) = (slot(pre)?, slot(gamma)?, slot(skip)?);
        let mut ir = crate::compiler_backend_cpu::image::Ir::default();
        let parameters = (0..4).map(|i| format!("%data{i}")).collect::<Vec<_>>();
        ir.parameters(
            &parameters
                .iter()
                .enumerate()
                .map(|(i, s)| (s.as_str(), 6, i as i64))
                .collect::<Vec<_>>(),
        );
        let types = (0..4)
            .map(|slot| if slot == gamma_slot { "i16" } else { "float" })
            .collect::<Vec<_>>();
        let function = format!("native_residual_{call_index}");
        writeln!(
            ir.text,
            "define void @{function}({}) #0 {{",
            types
                .iter()
                .enumerate()
                .map(|(i, ty)| format!("{ty}* noalias align 32 %data{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        // The scalar output is NCHW; preserve source axes and its final
        // contiguous-axis upcast instead of flattening unrelated dimensions.
        let lanes = if shape[3].is_multiple_of(4) { 4 } else { 1 };
        let channel = ir.begin_indexed_loop("channel", shape[1], vec![0, 0]);
        let gamma = ir.load_half(&format!("%data{gamma_slot}"), &channel);
        let row = if shape[2] == 1 {
            "0".to_owned()
        } else {
            ir.begin_indexed_loop("row", shape[2], vec![1, 0])
        };
        let groups = shape[3] / lanes;
        let column = if groups == 1 {
            "0".to_owned()
        } else {
            ir.begin_indexed_loop("column", groups, vec![2, 0])
        };
        let base = ir.op(format!("mul i32 {channel}, {spatial}"));
        let row = ir.op(format!("mul i32 {row}, {}", shape[3]));
        let base = ir.op(format!("add nsw i32 {base}, {row}"));
        let column = ir.op(format!("mul i32 {column}, {lanes}"));
        let base = ir.op(format!("add nsw i32 {base}, {column}"));
        let mut sums = Vec::new();
        for lane in 0..lanes {
            let index = ir.op(format!("add nsw i32 {base}, {lane}"));
            let pointer = ir.op(format!(
                "getelementptr inbounds float, float* %data{pre_slot}, i32 {index}"
            ));
            let pre = ir.op(format!("load float, float* {pointer}"));
            let pointer = ir.op(format!(
                "getelementptr inbounds float, float* %data{skip_slot}, i32 {index}"
            ));
            let skip = ir.op(format!("load float, float* {pointer}"));
            let half = ir.store_half(&skip);
            let skip = ir.decode_half(&half);
            let product = ir.op(format!("fmul nsz arcp contract afn float {pre}, {gamma}"));
            let (left, right) = if skip_position == 0 {
                (&skip, &product)
            } else {
                (&product, &skip)
            };
            sums.push(ir.op(format!("fadd nsz arcp contract afn float {left}, {right}")));
        }
        let pointer = ir.op(format!(
            "getelementptr inbounds float, float* %data0, i32 {base}"
        ));
        if lanes == 4 {
            ir.store_float4(&pointer, &sums);
        } else {
            ir.store_float(&pointer, &sums[0]);
        }
        if groups != 1 {
            ir.end_loop("column");
        }
        if shape[2] != 1 {
            ir.end_loop("row");
        }
        ir.end_loop("channel");
        ir.line("ret void");
        ir.text.push_str("}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(
            ir.text,
            "define void @op_kernel_{call_index}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        for (slot, ty) in types.iter().enumerate() {
            writeln!(ir.text, "  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %arg{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        writeln!(
            ir.text,
            "  call void @{function}({})\n  ret void\n}}",
            types
                .iter()
                .enumerate()
                .map(|(slot, ty)| format!("{ty}* %arg{slot}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        selected.insert(
            call_index,
            PipelineCall {
                source: ir.text,
                bytes: call
                    .buffers
                    .iter()
                    .map(|&id| schedule.buffers[id].bytes)
                    .collect(),
                workers: 1,
                scalars: 0,
            },
        );
    }
    Ok(selected)
}
