//! Affine layout and scalar/vector DAG for the independently captured pair.
use super::PipelineCall;
use crate::compiler_backend_cpu::image::Ir;
use std::fmt::Write;

#[derive(Clone, Copy)]
pub(super) enum Stage {
    Pack,
    Producer,
    Fused,
    Held,
}
#[derive(Clone)]
struct Axis {
    size: usize,
    key: Vec<i64>,
    // Dense output, packed input, packed weight, logical channel constants.
    strides: [usize; 4],
}
#[derive(Clone)]
pub(super) struct Layout {
    channels: usize,
    height: usize,
    width: usize,
    row_stride: usize,
    weight_stride: usize,
    axes: Vec<Axis>,
    upcast: Option<(usize, usize)>,
}
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
impl Layout {
    pub(super) fn new([channels, height, width]: [usize; 3]) -> Result<Self, String> {
        let count = channels
            .checked_mul(height)
            .and_then(|n| n.checked_mul(width));
        if channels == 0
            || !channels.is_multiple_of(4)
            || height == 0
            || width == 0
            || count.is_none_or(|n| n >= 2 * (128 << 10))
        {
            return Err(
                "parallel depthwise profile needs aligned channels and an unthreaded source domain"
                    .into(),
            );
        }
        let count = count.unwrap();
        let align = if height == 1 || count < 65536 {
            32
        } else {
            256
        };
        let row_stride = width.next_multiple_of(align / gcd(channels, align)) * channels;
        let weight_align = if channels / 4 == 1 || channels < 65536 {
            32
        } else {
            256
        };
        let weight_stride = 4 * (weight_align / gcd(4, weight_align));
        let mut axes = Vec::new();
        if channels / 4 > 1 {
            axes.push(Axis {
                size: channels / 4,
                key: vec![0, 0],
                strides: [4 * height * width, 4, weight_stride, 4],
            });
        }
        axes.push(Axis {
            size: 4,
            key: vec![0, i64::from(channels / 4 > 1)],
            strides: [height * width, 1, 1, 1],
        });
        let mut id = 1;
        if height > 1 {
            axes.push(Axis {
                size: height,
                key: vec![id, 0],
                strides: [width, row_stride, 0, 0],
            });
            id += 1;
        }
        if width > 1 {
            // Original adjacent affine ranges merge only when every buffer's
            // strides agree, including alignment padding in the packed input.
            if height > 1 && row_stride == width * channels {
                let last = axes.last_mut().unwrap();
                last.size *= width;
                last.strides = [1, channels, 0, 0];
            } else {
                axes.push(Axis {
                    size: width,
                    key: vec![id, 0],
                    strides: [1, channels, 0, 0],
                });
            }
        }
        let mut upcast = None;
        // With no reduction or masked output ranges, the first affine choice
        // can expand an input/output spatial range invariant in constants. No
        // second range is invariant after that same constant access is upcast.
        if count >= 1024 {
            let mut choices = Vec::new();
            for (axis, a) in axes.iter().enumerate() {
                if a.strides[2..] != [0, 0] {
                    continue;
                }
                for factor in [3, 4] {
                    if a.size.is_multiple_of(factor) {
                        choices.push((
                            a.strides.iter().filter(|&&v| v != 0).count(),
                            a.strides.iter().sum::<usize>(),
                            axis,
                            factor,
                        ));
                    }
                }
            }
            if let Some((_, _, axis, factor)) = choices.into_iter().min() {
                upcast = Some((axis, factor));
            }
        }
        if upcast.is_none() {
            let axis = axes.len() - 1;
            if axes[axis].size.is_multiple_of(4) {
                upcast = Some((axis, 4));
            }
        }
        Ok(Self {
            channels,
            height,
            width,
            row_stride,
            weight_stride,
            axes,
            upcast,
        })
    }
    pub(super) fn input_bytes(&self) -> usize {
        self.height * self.row_stride * 2
    }
    fn output_bytes(&self) -> usize {
        self.channels * self.height * self.width * 4
    }
    fn weight_bytes(&self) -> usize {
        self.channels / 4 * self.weight_stride * 2
    }
    pub(super) fn pack_weights(&self, raw: &[u8]) -> Result<Vec<u8>, String> {
        if raw.len() != self.channels * 2 {
            return Err("parallel depthwise weight capacity changed".into());
        }
        let mut packed = vec![0; self.weight_bytes()];
        for (channel, word) in raw.chunks_exact(2).enumerate() {
            let word = u16::from_le_bytes(word.try_into().unwrap());
            let word = if word & 0x7c00 == 0 {
                word & 0x8000
            } else {
                word
            };
            let at = (channel / 4 * self.weight_stride + channel % 4) * 2;
            packed[at..at + 2].copy_from_slice(&word.to_le_bytes());
        }
        Ok(packed)
    }
    fn index(&self, ir: &mut Ir, values: &[String], lane: usize, buffer: usize) -> String {
        let mut result = "0".to_owned();
        let mut order = (0..self.axes.len()).collect::<Vec<_>>();
        if buffer == 1 {
            let spatial = order
                .iter()
                .position(|&i| self.axes[i].strides[3] == 0)
                .unwrap_or(order.len());
            order[spatial..].reverse();
        }
        for axis in order {
            let a = &self.axes[axis];
            if a.strides[buffer] == 0 {
                continue;
            }
            let coordinate = if let Some((_, factor)) = self.upcast.filter(|(u, _)| *u == axis) {
                let base = ir.op(format!("mul i32 {}, {factor}", values[axis]));
                ir.op(format!("add nsw i32 {base}, {lane}"))
            } else {
                values[axis].clone()
            };
            let term = ir.op(format!("mul i32 {coordinate}, {}", a.strides[buffer]));
            result = if result == "0" {
                term
            } else {
                ir.op(format!("add nsw i32 {result}, {term}"))
            };
        }
        result
    }
    pub(super) fn artifact(
        &self,
        stage: Stage,
        function: &str,
        entry: &str,
    ) -> Result<PipelineCall, String> {
        if matches!(stage, Stage::Pack) {
            return crate::compiler_backend_cpu::convolution::input_artifact(
                self.channels,
                [self.height, self.width],
                self.row_stride,
                self.channels,
                self.input_bytes(),
                entry,
            );
        }
        let (types, bytes) = match stage {
            Stage::Producer => (
                vec!["float", "i16", "i16", "i16"],
                vec![
                    self.output_bytes(),
                    self.input_bytes(),
                    self.weight_bytes(),
                    self.channels * 2,
                ],
            ),
            Stage::Fused => (
                vec!["float", "i16", "i16", "i16", "i16", "i16"],
                vec![
                    self.output_bytes(),
                    self.input_bytes(),
                    self.weight_bytes(),
                    self.channels * 2,
                    self.channels * 2,
                    self.weight_bytes(),
                ],
            ),
            Stage::Held => (
                vec!["float", "float", "i16", "i16", "i16", "i16"],
                vec![
                    self.output_bytes(),
                    self.output_bytes(),
                    self.channels * 2,
                    self.input_bytes(),
                    self.channels * 2,
                    self.weight_bytes(),
                ],
            ),
            Stage::Pack => unreachable!(),
        };
        let mut ir = Ir::default();
        let names = (0..types.len())
            .map(|i| format!("%data{i}"))
            .collect::<Vec<_>>();
        ir.parameters(
            &names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.as_str(), 6, i as i64))
                .collect::<Vec<_>>(),
        );
        writeln!(
            ir.text,
            "define void @{function}({}) #0 {{",
            types
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{t}* noalias align 32 %data{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        let factor = self.upcast.map_or(1, |(_, n)| n);
        let mut values = vec!["0".to_owned(); self.axes.len()];
        let mut opened = Vec::new();
        let channel_end = self
            .axes
            .iter()
            .position(|a| a.strides[3] == 0)
            .unwrap_or(self.axes.len());
        let open = |ir: &mut Ir, values: &mut [String], opened: &mut Vec<String>, axis: usize| {
            let size = self.axes[axis].size
                / if self.upcast.is_some_and(|(a, _)| a == axis) {
                    factor
                } else {
                    1
                };
            if size > 1 {
                let name = format!("parallel_axis_{axis}");
                values[axis] = ir.begin_indexed_loop(&name, size, self.axes[axis].key.clone());
                opened.push(name);
            }
        };
        for axis in 0..channel_end {
            open(&mut ir, &mut values, &mut opened, axis);
        }
        let mut coefficients = Vec::new();
        for lane in 0..factor {
            let w = self.index(&mut ir, &values, lane, 2);
            let c = self.index(&mut ir, &values, lane, 3);
            let fields = match stage {
                Stage::Producer => vec![(2, w), (3, c)],
                Stage::Fused => vec![(2, w.clone()), (3, c.clone()), (4, c), (5, w)],
                Stage::Held => vec![(2, c.clone()), (4, c), (5, w)],
                Stage::Pack => unreachable!(),
            };
            coefficients.push(
                fields
                    .into_iter()
                    .map(|(slot, at)| ir.load_half(&format!("%data{slot}"), &at))
                    .collect::<Vec<_>>(),
            );
        }
        for axis in channel_end..self.axes.len() {
            open(&mut ir, &mut values, &mut opened, axis);
        }
        let input_slot = if matches!(stage, Stage::Held) { 3 } else { 1 };
        let base = self.index(&mut ir, &values, 0, 1);
        let input = if factor == 4 {
            ir.half_values(
                &format!("%data{input_slot}"),
                &base,
                4,
                self.axes[self.upcast.unwrap().0].strides[1],
                true,
            )
        } else {
            (0..factor)
                .map(|lane| {
                    let at = self.index(&mut ir, &values, lane, 1);
                    ir.load_half(&format!("%data{input_slot}"), &at)
                })
                .collect()
        };
        let output_base = self.index(&mut ir, &values, 0, 0);
        let held = if matches!(stage, Stage::Held) {
            if factor == 4 && self.axes[self.upcast.unwrap().0].strides[0] == 1 {
                ir.float_values("%data1", &output_base, 4, None)
            } else {
                (0..factor)
                    .map(|lane| {
                        let at = self.index(&mut ir, &values, lane, 0);
                        let p = ir.op(format!(
                            "getelementptr inbounds float, float* %data1, i32 {at}"
                        ));
                        ir.op(format!("load float, float* {p}"))
                    })
                    .collect()
            }
        } else {
            Vec::new()
        };
        let mut sums = Vec::new();
        for lane in 0..factor {
            let c = &coefficients[lane];
            let pre = if matches!(stage, Stage::Held) {
                held[lane].clone()
            } else {
                let product = ir.op(format!(
                    "fmul nsz arcp contract afn float {}, {}",
                    input[lane], c[0]
                ));
                ir.op(format!(
                    "fadd nsz arcp contract afn float {product}, {}",
                    c[1]
                ))
            };
            let value = if matches!(stage, Stage::Producer) {
                pre
            } else {
                let (gamma, bias, weight) = if matches!(stage, Stage::Held) {
                    (&c[0], &c[1], &c[2])
                } else {
                    (&c[2], &c[1], &c[3])
                };
                let skip = ir.op(format!(
                    "fmul nsz arcp contract afn float {}, {weight}",
                    input[lane]
                ));
                let skip = ir.op(format!("fadd nsz arcp contract afn float {skip}, {bias}"));
                let product = ir.op(format!("fmul nsz arcp contract afn float {pre}, {gamma}"));
                ir.op(format!(
                    "fadd nsz arcp contract afn float {product}, {skip}"
                ))
            };
            sums.push(value);
        }
        // LLVM source rendering groups a contiguous four-lane upcast into a
        // float4 store, but leaves a factor-three upcast as three scalar stores.
        if factor == 4 && self.axes[self.upcast.unwrap().0].strides[0] == 1 {
            let p = ir.op(format!(
                "getelementptr inbounds float, float* %data0, i32 {output_base}"
            ));
            ir.store_float_vector(&p, &sums);
        } else {
            for (lane, value) in sums.iter().enumerate() {
                let at = self.index(&mut ir, &values, lane, 0);
                let p = ir.op(format!(
                    "getelementptr inbounds float, float* %data0, i32 {at}"
                ));
                ir.store_float(&p, value);
            }
        }
        for name in opened.iter().rev() {
            ir.end_loop(name);
        }
        ir.line("ret void");
        writeln!(ir.text, "}}\nattributes #0 = {{ noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }}\ndefine void @{entry}(i8** %buffers, i32* %scalars) {{").unwrap();
        for (slot, ty) in types.iter().enumerate() {
            writeln!(ir.text, "  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        writeln!(
            ir.text,
            "  call void @{function}({})\n  ret void\n}}",
            types
                .iter()
                .enumerate()
                .map(|(i, t)| format!("{t}* %ptr{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .unwrap();
        Ok(PipelineCall {
            source: ir.text,
            bytes,
            workers: 1,
            scalars: 0,
        })
    }
}
