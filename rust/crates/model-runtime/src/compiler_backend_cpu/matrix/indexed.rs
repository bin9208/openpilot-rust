//! Fused normalized feature indexing and image_dot input packing. Tensor.cat's
//! masked ADD and original division/upcast DAG are observable for zero/NaN bits;
//! separate f32 Div and bit-preserving Select are not interchangeable here.
use super::{graph::PipelineCall, MatrixLayout};
use crate::compiler_backend_cpu::image::Ir;
use std::fmt::Write;

fn upcasts(channels: usize, rows: usize, stride: usize) -> (usize, usize) {
    let mut factors = [1usize, 1];
    let mut sizes = [rows, channels];
    let strides = [[stride, 1, channels, 0, 0], [1, 0, 1, 1, 0]];
    // Only row ranges occur in the source Concat/indirect-load validity gates.
    if rows <= 7 {
        factors[0] = rows;
        sizes[0] = 1;
    }
    let mut used = [false; 2];
    while sizes[0] * sizes[1] >= 1024 && factors.iter().product::<usize>() < 32 {
        let mut choices = Vec::new();
        for axis in 0..2 {
            for factor in [3, 4] {
                if used[axis] || !sizes[axis].is_multiple_of(factor) {
                    continue;
                }
                if (0..5).any(|buffer| {
                    strides[axis][buffer] == 0
                        && (0..2)
                            .filter(|&a| factors[a] > 1)
                            .all(|a| strides[a][buffer] != 0)
                }) {
                    choices.push((
                        strides[axis].iter().filter(|&&v| v != 0).count(),
                        strides[axis].iter().sum::<usize>(),
                        axis,
                        factor,
                    ));
                }
            }
        }
        let Some((_, _, axis, factor)) = choices.into_iter().min() else {
            break;
        };
        factors[axis] *= factor;
        sizes[axis] /= factor;
        used[axis] = true;
    }
    if factors == [1, 1] && channels.is_multiple_of(4) {
        factors[1] = 4;
    }
    (factors[0], factors[1])
}
fn wrap(ir: &mut Ir, row: &str, history: usize) -> String {
    if history.is_power_of_two() {
        return ir.op(format!("and i32 {row}, {}", history - 1));
    }
    let rem = ir.op(format!("srem i32 {row}, {history}"));
    let neg = ir.op(format!("icmp slt i32 {row}, 0"));
    let nonzero = ir.op(format!("icmp ne i32 {rem}, 0"));
    let neg = ir.op(format!("icmp ne i1 {neg}, 0"));
    let adjust = ir.op(format!("and i1 {nonzero}, {neg}"));
    let adjust = ir.op(format!("select i1 {adjust}, i32 {history}, i32 0"));
    ir.op(format!("add nsw i32 {rem}, {adjust}"))
}
pub fn artifact(
    channels: usize,
    history: usize,
    window: usize,
    function: &str,
    entry: &str,
) -> Result<PipelineCall, String> {
    if history == 0
        || window == 0
        || window > history.checked_add(1).ok_or("feature history overflow")?
        || history > 1 << 20
        || !super::render::identifier(function)
        || !super::render::identifier(entry)
        || function == entry
    {
        return Err("invalid indexed feature packing profile".into());
    }
    let history_bytes = history
        .checked_mul(channels)
        .filter(|&n| n <= i32::MAX as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("feature history storage exceeds signed32 domain")?;
    let layout = MatrixLayout::new(&[1, window, channels], &[channels, 4])?;
    let (ru, cu) = upcasts(channels, window, layout.input_row_stride);
    let mut ir = Ir::default();
    ir.parameters(&[
        ("%out", 6, 0),
        ("%indices", 6, 1),
        ("%history", 6, 2),
        ("%terminal", 6, 3),
        ("%norm", 6, 4),
    ]);
    writeln!(ir.text,"define void @{function}(i16* noalias align 32 %out, i64* noalias align 32 %indices, float* noalias align 32 %history, float* noalias align 32 %terminal, float* noalias align 32 %norm) #0 {{").unwrap();
    let p = ir.op("getelementptr inbounds float, float* %norm, i32 0");
    let norm = ir.op(format!("load float, float* {p}"));
    let row_group = if window > ru {
        ir.begin_indexed_loop("indexed_rows", window / ru, vec![1, 0])
    } else {
        "0".into()
    };
    let row_base = ir.op(format!("mul i32 {row_group}, {ru}"));
    let mut descriptors = Vec::new();
    for row in 0..ru {
        let output_row = ir.op(format!("add nsw i32 {row_base}, {row}"));
        let p = ir.op(format!(
            "getelementptr inbounds i64, i64* %indices, i32 {output_row}"
        ));
        let index = ir.op(format!("load i64, i64* {p}"));
        let normalized = ir.op(format!("add nsw i64 {index}, {}", history + 1));
        let neg = ir.op(format!("icmp slt i64 {index}, 0"));
        let normalized = ir.op(format!("select i1 {neg}, i64 {normalized}, i64 {index}"));
        let narrowed = ir.op(format!("trunc i64 {normalized} to i32"));
        let wrapped = wrap(&mut ir, &narrowed, history);
        let from = ir.op(format!("mul i32 {wrapped}, {channels}"));
        let in_history = ir.op(format!("icmp slt i64 {normalized}, {history}"));
        let below = ir.op(format!("icmp slt i64 {normalized}, {}", history + 1));
        let above = ir.op(format!("icmp slt i64 -1, {normalized}"));
        let valid = ir.op(format!("and i1 {above}, {below}"));
        let load_valid = ir.op(format!("and i1 {in_history}, {valid}"));
        descriptors.push((output_row, from, in_history, valid, load_valid));
    }
    let column = if channels > cu {
        ir.begin_indexed_loop(
            "indexed_channels",
            channels / cu,
            if cu > 1 {
                vec![2, 0]
            } else if layout.input_channels >= 8 && (layout.input_channels / 4).is_power_of_two() {
                vec![2, 1, 0]
            } else {
                vec![3, 0]
            },
        )
    } else {
        "0".into()
    };
    let channel_base = ir.op(format!("mul i32 {column}, {cu}"));
    let mut features = Vec::new();
    for (_, from, _, _, load_valid) in &descriptors {
        let from = ir.op(format!("add nsw i32 {from}, {channel_base}"));
        features.extend(ir.float_values("%history", &from, cu, Some(load_valid)));
    }
    let terminal = ir.float_values("%terminal", &channel_base, cu, None);
    let divided = terminal
        .iter()
        .map(|x| ir.op(format!("fdiv nsz arcp contract afn float {x}, {norm}")))
        .collect::<Vec<_>>();
    for (row, (output_row, _, in_history, valid, _)) in descriptors.iter().enumerate() {
        for (lane, divided) in divided.iter().enumerate() {
            let current = ir.op(format!(
                "select i1 {in_history}, float 0.0, float {divided}"
            ));
            let value = ir.op(format!(
                "fadd nsz arcp contract afn float {}, {current}",
                features[row * cu + lane]
            ));
            let half = ir.masked_plain_half(&value, valid);
            let channel = ir.op(format!("add nsw i32 {channel_base}, {lane}"));
            let output = ir.op(format!("mul i32 {output_row}, {}", layout.input_row_stride));
            let output = ir.op(format!("add nsw i32 {output}, {channel}"));
            let p = ir.op(format!(
                "getelementptr inbounds i16, i16* %out, i32 {output}"
            ));
            ir.store_i16(&p, &half);
        }
    }
    if channels > cu {
        ir.end_loop("indexed_channels");
    }
    if window > ru {
        ir.end_loop("indexed_rows");
    }
    ir.line("ret void");
    writeln!(ir.text,"}}\nattributes #0 = {{ noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }}\ndefine void @{entry}(i8** %buffers, i32* %scalars) {{").unwrap();
    let mut args = Vec::new();
    for (slot, ty) in ["i16", "i64", "float", "float", "float"]
        .into_iter()
        .enumerate()
    {
        writeln!(ir.text,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        args.push(format!("{ty}* %ptr{slot}"));
    }
    writeln!(
        ir.text,
        "  call void @{function}({})\n  ret void\n}}",
        args.join(", ")
    )
    .unwrap();
    Ok(PipelineCall {
        source: ir.text,
        bytes: vec![
            layout.input_elements * 2,
            window * 8,
            history_bytes,
            channels * 4,
            4,
        ],
        workers: 1,
        scalars: 0,
    })
}
