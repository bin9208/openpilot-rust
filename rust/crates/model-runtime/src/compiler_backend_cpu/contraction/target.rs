//! Target-owned contraction for already admitted, direct f32 expressions.
//! Emits the source MUL/ADD DAG rather than imposing a hardware FMA or a NaN
//! priority rule. The core keeps its original independent codegen boundary:
//! inlining it into the indirect ABI wrapper changes LLVM NaN operand ownership.
//! Scheduling and storage ownership remain with the caller.
mod residual;
use crate::compiler_onnx::{
    lower::{broadcast_index, IndexExpr, LoweredGraph, ScalarExpr},
    schedule::Schedule,
};
use std::{collections::BTreeMap, fmt::Write};

pub(crate) fn scheduled(
    graph: &LoweredGraph,
    model: &crate::compiler_onnx::Model,
    schedule: &Schedule,
) -> Result<BTreeMap<usize, super::super::matrix::graph::PipelineCall>, String> {
    let mut selected = BTreeMap::new();
    for (call_index, call) in schedule.calls.iter().enumerate() {
        let output = call.buffers[0];
        if !graph.cpu_contractions.contains(&output) {
            continue;
        }
        let plan = &graph.plans[output];
        let ScalarExpr::MulAcc { dtype: 1, a, b, c } = &plan.expression else {
            continue;
        };
        let elements = plan
            .ty
            .shape
            .iter()
            .try_fold(1usize, |n, &d| n.checked_mul(d));
        let Some(elements) = elements.filter(|&n| n > 0 && n <= i32::MAX as usize / 4) else {
            return Err("configured contraction exceeds bounded LLVM index/storage range".into());
        };
        let mapping = broadcast_index(&plan.ty.shape, &plan.ty.shape);
        let operands = [a.as_ref(), b.as_ref(), c.as_ref()];
        // Projected half storage, broadcasts and movement maps have distinct
        // source iteration/layout rules. They retain their existing path.
        let mut slots = Vec::new();
        for operand in operands {
            let ScalarExpr::Load { plan: input, index } = operand else {
                break;
            };
            if graph.plans[*input].ty != plan.ty
                || plan.ty.dtype != 1
                || !matches!(index, IndexExpr::Index) && index != &mapping
            {
                break;
            }
            let slot = call.buffers[1..]
                .iter()
                .position(|&id| id == *input)
                .ok_or("configured contraction missing scheduled input")?
                + 1;
            slots.push(slot);
        }
        if slots.len() != 3 || call.buffers.len() != 4 {
            continue;
        }
        let function = format!("native_contraction_{call_index}");
        let mut ir = super::super::image::Ir::default();
        let names = (0..call.buffers.len())
            .map(|slot| format!("%data{slot}"))
            .collect::<Vec<_>>();
        let parameters = names
            .iter()
            .enumerate()
            .map(|(slot, name)| (name.as_str(), 6, slot as i64))
            .collect::<Vec<_>>();
        ir.parameters(&parameters);
        writeln!(
            ir.text,
            "define void @{function}({}) #0 {{",
            names
                .iter()
                .map(|name| format!("float* noalias align 32 {name}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        // Retained hand_coded_optimizations: if no earlier upcast exists,
        // upcast the final contiguous axis by four when it divides exactly.
        // Odd lengths keep the original scalar loop, without a fitted tail.
        let lanes = if elements.is_multiple_of(4) { 4 } else { 1 };
        let groups = elements / lanes;
        let group = if groups == 1 {
            "0".to_owned()
        } else {
            ir.begin_indexed_loop("element", groups, vec![0, 0])
        };
        let index = if lanes == 4 {
            ir.op(format!("mul i32 {group}, 4"))
        } else {
            group
        };
        let mut operands = Vec::new();
        for slot in slots {
            let pointer = ir.op(format!(
                "getelementptr inbounds float, float* %data{slot}, i32 {index}"
            ));
            let values = if lanes == 4 {
                let vector = ir.op(format!("load <4 x float>, <4 x float>* {pointer}"));
                (0..4)
                    .map(|lane| ir.op(format!("extractelement <4 x float> {vector}, i32 {lane}")))
                    .collect::<Vec<_>>()
            } else {
                vec![ir.op(format!("load float, float* {pointer}"))]
            };
            operands.push(values);
        }
        let mut values = Vec::new();
        for ((a, b), c) in operands[0].iter().zip(&operands[1]).zip(&operands[2]) {
            let product = ir.op(format!("fmul nsz arcp contract afn float {}, {}", a, b));
            values.push(ir.op(format!("fadd nsz arcp contract afn float {product}, {}", c)));
        }
        let pointer = ir.op(format!(
            "getelementptr inbounds float, float* %data0, i32 {index}"
        ));
        if lanes == 4 {
            ir.store_float4(&pointer, &values);
        } else {
            ir.store_float(&pointer, &values[0]);
        }
        if groups != 1 {
            ir.end_loop("element");
        }
        ir.line("ret void");
        ir.text.push_str("}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(
            ir.text,
            "define void @op_kernel_{call_index}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        for slot in 0..call.buffers.len() {
            writeln!(ir.text, "  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %arg{slot} = bitcast i8* %raw{slot} to float*").unwrap();
        }
        writeln!(
            ir.text,
            "  call void @{function}({})\n  ret void\n}}",
            (0..call.buffers.len())
                .map(|slot| format!("float* %arg{slot}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        selected.insert(
            call_index,
            super::super::matrix::graph::PipelineCall {
                source: ir.text,
                bytes: vec![elements * 4; call.buffers.len()],
                workers: 1,
                scalars: 0,
            },
        );
    }
    for (index, call) in residual::scheduled(graph, model, schedule)? {
        if selected.insert(index, call).is_some() {
            return Err("overlapping configured contraction profiles".into());
        }
    }
    Ok(selected)
}
