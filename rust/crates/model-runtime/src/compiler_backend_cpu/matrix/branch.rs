//! A closed Concat of bias-bearing image_dot reductions over one packed row.
//! Concat's masked output chunks share a kernel, but each head retains its own
//! reduction ranges. The final four-channel range alone is unrolled by the
//! retained multi-reduce heuristic (tinygrad codegen/opt/heuristic.py, MIT).
use super::{
    gemm::{constant, gemm, producer},
    graph::PipelineCall,
    MatrixLayout,
};
use crate::compiler_backend_cpu::image::Ir;
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    Attribute, Model, Node,
};
use std::{collections::BTreeSet, fmt::Write};

#[derive(Debug)]
pub struct Head {
    pub channels: usize,
    pub weight: usize,
    pub bias: usize,
}
#[derive(Debug)]
pub struct Tail {
    pub output: usize,
    pub heads: Vec<Head>,
    pub named: BTreeSet<usize>,
}
/// The source tail must be held, and every head must read the same activation.
/// Intermediate binding and additional readers are checked by chain ownership.
pub(super) fn tail(graph: &LoweredGraph, model: &Model, first: &Node) -> Option<Tail> {
    let concat = model.graph.nodes.iter().find(|n| {
        n.op == "Concat"
            && n.inputs.first() == first.outputs.first()
            && n.inputs.len() >= 2
            && n.outputs.len() == 1
            && n.attributes.len() == 1
            && matches!(n.attributes.get("axis"), Some(Attribute::Integer(1)))
    })?;
    let output = graph.names[&concat.outputs[0]];
    if !graph.outputs.values().any(|&id| id == output) {
        return None;
    }
    let input = graph.names[&first.inputs[0]];
    let ty = &graph.plans[input].ty;
    if ty.dtype != 1 || ty.shape.len() != 2 || ty.shape[0] != 1 {
        return None;
    }
    let mut heads = Vec::new();
    let mut named = BTreeSet::from([output]);
    for name in &concat.inputs {
        let node = producer(model, name, "Gemm").filter(|n| gemm(n))?;
        if node.inputs[0] != first.inputs[0] {
            return None;
        }
        let id = graph.names[name];
        let shape = &graph.plans[id].ty.shape;
        if shape.len() != 2 || shape[0] != 1 || graph.plans[id].ty.dtype != 1 {
            return None;
        }
        let channels = shape[1];
        let weight = graph.names[&node.inputs[1]];
        let bias = graph.names[&node.inputs[2]];
        if !constant(graph, weight, &[channels, ty.shape[1]]) || !constant(graph, bias, &[channels])
        {
            return None;
        }
        heads.push(Head {
            channels,
            weight,
            bias,
        });
        named.insert(id);
    }
    let widths = heads.iter().map(|h| h.channels).collect::<Vec<_>>();
    profile(ty.shape[1], &widths).ok()?;
    if graph.plans[output].ty.shape != [1, widths.iter().sum()] || graph.plans[output].ty.dtype != 1
    {
        return None;
    }
    Some(Tail {
        output,
        heads,
        named,
    })
}
fn profile(input: usize, widths: &[usize]) -> Result<(usize, usize), String> {
    if input == 0 || !input.is_multiple_of(4) || widths.len() < 2 || widths.contains(&0) {
        return Err("Concat image_dot requires aligned input and nonempty heads".into());
    }
    let total = widths
        .iter()
        .try_fold(0usize, |sum, &width| {
            sum.checked_add(width).filter(|&n| n <= 1 << 20)
        })
        .ok_or("Concat image_dot output exceeds bounded storage")?;
    let chunk = widths.iter().copied().reduce(super::gcd).unwrap();
    let chunks = total / chunk;
    // Masked-axis upcast is permitted only within the original <=7 rule.
    // Other Concat schedules need separate range normalization, not this ABI.
    if chunks > 7 || input > 1 << 20 {
        return Err("Concat image_dot masked upcast profile is unsupported".into());
    }
    Ok((chunk, chunks))
}
pub(super) fn pack_weight(
    graph: &LoweredGraph,
    head: &Head,
    input: usize,
) -> Result<Vec<u8>, String> {
    let ScalarExpr::Constant(raw) = &graph.plans[head.weight].expression else {
        return Err("Concat weight changed".into());
    };
    let padded = head.channels.next_multiple_of(4);
    let layout = MatrixLayout::new(&[1, input], &[input, padded])?;
    if raw.len() != head.channels * input * 2 {
        return Err("Concat initializer size changed".into());
    }
    let mut logical = vec![0; input * padded * 2];
    for i in 0..input {
        for o in 0..head.channels {
            logical[(i * padded + o) * 2..(i * padded + o + 1) * 2]
                .copy_from_slice(&raw[(o * input + i) * 2..(o * input + i + 1) * 2]);
        }
    }
    let mut packed = vec![0; layout.weight_elements * 2];
    layout.pack_weights_into(&logical, &mut packed)?;
    Ok(packed)
}
fn weight_index(
    ir: &mut Ir,
    output: &str,
    block: &str,
    lane: &str,
    stride: usize,
    interval: std::ops::RangeInclusive<usize>,
) -> String {
    let min = *interval.start();
    let max = *interval.end();
    let group = if min / 4 == max / 4 {
        (min / 4 * stride).to_string()
    } else {
        let group = ir.op(format!("ashr i32 {output}, 2"));
        ir.op(format!("mul i32 {group}, {stride}"))
    };
    let ol = if max < 4 {
        output.to_owned()
    } else {
        ir.op(format!("and i32 {output}, 3"))
    };
    let ol = ir.op(format!("mul i32 {ol}, 4"));
    let rb = ir.op(format!("mul i32 {block}, 16"));
    let offset = ir.op(format!("add nsw i32 {ol}, {rb}"));
    let offset = ir.op(format!("add nsw i32 {offset}, {lane}"));
    if group == "0" {
        offset
    } else {
        ir.op(format!("add nsw i32 {group}, {offset}"))
    }
}
fn pointer(ir: &mut Ir, register: usize, slot: usize) -> String {
    ir.op(format!(
        "getelementptr inbounds float, float* %acc{register}, i32 {slot}"
    ))
}
/// One typed fused artifact. This does not emit independent padded terminal
/// Gemms: masked Concat chunks and all head reductions retain one LLVM DAG.
pub fn artifact(
    input: usize,
    widths: &[usize],
    function: &str,
    entry: &str,
) -> Result<PipelineCall, String> {
    let (chunk, chunks) = profile(input, widths)?;
    if !super::render::identifier(function)
        || !super::render::identifier(entry)
        || function == entry
    {
        return Err("invalid Concat identifiers".into());
    }
    let layouts = widths
        .iter()
        .map(|&n| MatrixLayout::new(&[1, input], &[input, n.next_multiple_of(4)]))
        .collect::<Result<Vec<_>, _>>()?;
    let groups = input / 4;
    let reductions = if groups == 1 { 1 } else { 2 };
    let mut ir = Ir::default();
    let mut params = vec![("%out".to_owned(), 6, 0), ("%input".to_owned(), 6, 1)];
    for h in 0..widths.len() {
        params.extend([
            (format!("%weight{h}"), 6, 2 + 2 * h as i64),
            (format!("%bias{h}"), 6, 3 + 2 * h as i64),
        ]);
    }
    let registers = widths.len() - usize::from(groups == 1);
    for r in 0..registers {
        params.push((format!("%acc{r}"), 3, r as i64));
    }
    ir.parameters(
        &params
            .iter()
            .map(|(n, o, s)| (n.as_str(), *o, *s))
            .collect::<Vec<_>>(),
    );
    write!(
        ir.text,
        "define void @{function}(float* noalias align 32 %out, i16* noalias align 32 %input"
    )
    .unwrap();
    for h in 0..widths.len() {
        write!(
            ir.text,
            ", i16* noalias align 32 %weight{h}, i16* noalias align 32 %bias{h}"
        )
        .unwrap();
    }
    writeln!(ir.text, ") #0 {{").unwrap();
    for r in 0..registers {
        ir.line(format!("%acc{r} = alloca [{chunks} x float]"));
    }
    let invariant = if groups == 1 {
        Some(ir.half_values("%input", "0", 4, 1, true))
    } else {
        None
    };
    let out = ir.begin_indexed_loop(
        "concat_output",
        chunk,
        vec![(reductions * widths.len()) as i64, 1, 0],
    );
    let mut values = Vec::new();
    let mut global_chunk = 0;
    for (h, &width) in widths.iter().enumerate() {
        let last = h + 1 == widths.len();
        let count = width / chunk;
        let register = registers - 1 - h.min(registers - 1);
        let has_loop = !last || groups > 1;
        let mut acc = vec![None; count];
        if has_loop {
            for slot in 0..chunks {
                let p = pointer(&mut ir, register, slot);
                ir.store_float(&p, "0.0");
            }
        }
        let block = if groups > 1 {
            ir.begin_indexed_loop(
                &format!("concat_block{h}"),
                groups,
                vec![(h * reductions) as i64, 1],
            )
        } else {
            "0".into()
        };
        let lane = if !last {
            ir.begin_indexed_loop(
                &format!("concat_lane{h}"),
                4,
                vec![(h * reductions + usize::from(groups > 1)) as i64, 1],
            )
        } else {
            "0".into()
        };
        if has_loop {
            for (i, a) in acc.iter_mut().enumerate() {
                let p = pointer(&mut ir, register, global_chunk + i);
                *a = Some(ir.op(format!("load float, float* {p}")));
            }
        }
        let base = ir.op(format!("mul i32 {block}, 4"));
        let input_values = if last {
            invariant
                .clone()
                .unwrap_or_else(|| ir.half_values("%input", &base, 4, 1, true))
        } else {
            let ix = ir.op(format!("add nsw i32 {base}, {lane}"));
            vec![ir.load_half("%input", &ix)]
        };
        let mut weights = Vec::new();
        for i in 0..count {
            let o = ir.op(format!("add nsw i32 {out}, {}", i * chunk));
            let ix = weight_index(
                &mut ir,
                &o,
                &block,
                &lane,
                layouts[h].weight_output_stride,
                i * chunk..=(i + 1) * chunk - 1,
            );
            weights.push(if last {
                ir.half_values(&format!("%weight{h}"), &ix, 4, 1, true)
            } else {
                vec![ir.load_half(&format!("%weight{h}"), &ix)]
            });
        }
        for (l, x) in input_values.iter().enumerate() {
            for (i, a) in acc.iter_mut().enumerate() {
                let product = ir.op(format!(
                    "fmul nsz arcp contract afn float {x}, {}",
                    weights[i][l]
                ));
                *a = Some(match a.take() {
                    Some(old) => {
                        ir.op(format!("fadd nsz arcp contract afn float {old}, {product}"))
                    }
                    None => product,
                });
            }
        }
        if has_loop {
            for (i, a) in acc.iter().enumerate() {
                let p = pointer(&mut ir, register, global_chunk + i);
                ir.store_float(&p, a.as_ref().unwrap());
            }
        }
        if !last {
            ir.end_loop(&format!("concat_lane{h}"));
        }
        if groups > 1 {
            ir.end_loop(&format!("concat_block{h}"));
        }
        if has_loop {
            for (i, a) in acc.iter_mut().enumerate() {
                let p = pointer(&mut ir, register, global_chunk + i);
                *a = Some(ir.op(format!("load float, float* {p}")));
            }
        }
        for (i, a) in acc.into_iter().enumerate() {
            let local = ir.op(format!("add nsw i32 {out}, {}", i * chunk));
            values.push((global_chunk + i, a.unwrap(), h, local));
        }
        global_chunk += count;
    }
    for (slot, acc, h, local) in values {
        let bias = ir.load_half(&format!("%bias{h}"), &local);
        let value = ir.op(format!("fadd nsz arcp contract afn float {acc}, {bias}"));
        let ix = ir.op(format!("add nsw i32 {out}, {}", slot * chunk));
        let p = ir.op(format!(
            "getelementptr inbounds float, float* %out, i32 {ix}"
        ));
        ir.store_float(&p, &value);
    }
    ir.end_loop("concat_output");
    ir.line("ret void");
    writeln!(ir.text,"}}\nattributes #0 = {{ noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }}\ndefine void @{entry}(i8** %buffers, i32* %scalars) {{").unwrap();
    let mut args = Vec::new();
    for slot in 0..2 + 2 * widths.len() {
        let ty = if slot == 0 { "float" } else { "i16" };
        writeln!(ir.text,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        args.push(format!("{ty}* %ptr{slot}"));
    }
    writeln!(
        ir.text,
        "  call void @{function}({})\n  ret void\n}}",
        args.join(", ")
    )
    .unwrap();
    let mut bytes = vec![
        widths.iter().sum::<usize>() * 4,
        layouts[0].input_elements * 2,
    ];
    for (h, layout) in layouts.iter().enumerate() {
        bytes.extend([layout.weight_elements * 2, widths[h] * 2]);
    }
    Ok(PipelineCall {
        source: ir.text,
        bytes,
        workers: 1,
        scalars: 0,
    })
}
