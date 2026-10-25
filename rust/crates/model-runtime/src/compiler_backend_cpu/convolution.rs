//! Packed masked depthwise Conv primitive, not ONNX graph admission.
//! Source: retained tinygrad tensor.image_conv2d, masked-axis upcasting and
//! load/store folding (MIT license retained in tinygrad_repo/LICENSE).
mod graph;
mod pointwise;
use super::image::{half_storage, Ir};
pub(super) use graph::{scheduled, stage};
pub use pointwise::Pointwise;
use std::fmt::Write;

pub struct MaskedDepthwise {
    channels: usize,
    input: [usize; 2],
    kernel: [usize; 2],
    pads: [usize; 4],
    output: [usize; 2],
    row_stride: usize,
    weight_stride: usize,
}
pub struct Artifact {
    pub source: String,
    pub bytes: [usize; 4],
    pub workers: usize,
    pub scalars: usize,
}
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
impl MaskedDepthwise {
    /// Unit-stride, unit-dilation, nonnegative padding. Both masked output axes
    /// must satisfy the original full-upcast heuristic; all reduction axes must
    /// satisfy its full-unroll rule. No observed dimensions or tensor values are
    /// used to select arithmetic or payload behavior.
    pub fn new(input: [usize; 3], kernel: [usize; 2], pads: [usize; 4]) -> Result<Self, String> {
        let [channels, height, width] = input;
        let area = kernel[0]
            .checked_mul(kernel[1])
            .filter(|&n| n > 0 && n <= 32)
            .ok_or("masked depthwise kernel exceeds source full-unroll range")?;
        // heuristic.py expands one reduction axis up to32, then a second
        // only if both extents are <=3. A small total area is not sufficient
        // proof for removing that remaining source reduction loop.
        if kernel[0] > 1 && kernel[1] > 1 && kernel.iter().any(|&n| n > 3) {
            return Err("masked depthwise reduction retains an unproved source loop".into());
        }
        if channels == 0
            || !channels.is_multiple_of(4)
            || height == 0
            || width == 0
            || kernel.contains(&0)
            || pads.iter().any(|&p| p > 32)
        {
            return Err(
                "masked depthwise requires aligned channels and bounded nonempty input/kernel"
                    .into(),
            );
        }
        let mut output = [0; 2];
        for axis in 0..2 {
            if pads[axis] == 0 && pads[axis + 2] == 0 {
                return Err("depthwise axis lacks the source masked full-upcast proof".into());
            }
            output[axis] = input[axis + 1]
                .checked_add(pads[axis])
                .and_then(|n| n.checked_add(pads[axis + 2]))
                .and_then(|n| n.checked_sub(kernel[axis]))
                .and_then(|n| n.checked_add(1))
                .filter(|&n| n > 0 && n <= 7)
                .ok_or("depthwise output exceeds source masked-axis full-upcast range")?;
        }
        if output[0] * output[1] > 49 {
            return Err("masked depthwise upcast product exceeds source heuristic".into());
        }
        let padded_output_width = output[1].next_multiple_of(32 / gcd(channels, 32));
        channels
            .checked_mul(output[0])
            .and_then(|n| n.checked_mul(padded_output_width))
            .and_then(|n| n.checked_mul(area))
            .filter(|&n| n < 2 * (128 << 10))
            .ok_or("masked depthwise source scheduling requires a threaded profile")?;
        let input_elements = channels
            .checked_mul(height)
            .and_then(|n| n.checked_mul(width))
            .filter(|&n| n <= 1 << 24)
            .ok_or("masked depthwise input storage exceeds bound")?;
        let weights = channels
            .checked_mul(kernel[0])
            .and_then(|n| n.checked_mul(kernel[1]))
            .filter(|&n| n <= 1 << 24)
            .ok_or("masked depthwise weight storage exceeds bound")?;
        // tensor.pad_align: half storage uses 32 scalar elements for small
        // tensors or a singleton prefix, otherwise 256 scalar elements.
        let align = if height == 1 || input_elements < 65536 {
            32
        } else {
            256
        };
        let padded_width = width.next_multiple_of(align / gcd(channels, align));
        let row_stride = padded_width
            .checked_mul(channels)
            .ok_or("depthwise row stride overflow")?;
        let weight_align = if channels / 4 == 1 || weights < 65536 {
            32
        } else {
            256
        };
        let weight_row = kernel[1] * 4;
        let padded_height =
            kernel[0].next_multiple_of(weight_align / gcd(weight_row, weight_align));
        let weight_stride = padded_height * weight_row;
        let result = Self {
            channels,
            input: [height, width],
            kernel,
            pads,
            output,
            row_stride,
            weight_stride,
        };
        if result.bytes().iter().any(|&n| n > i32::MAX as usize / 4) {
            return Err("masked depthwise physical storage exceeds bounded LLVM indices".into());
        }
        Ok(result)
    }
    fn bytes(&self) -> [usize; 4] {
        [
            self.channels * self.output[0] * self.output[1] * 4,
            self.input[0] * self.row_stride * 2,
            self.channels / 4 * self.weight_stride * 2,
            self.channels * 2,
        ]
    }
    pub fn pack_input(&self, logical: &[u8]) -> Result<Vec<u8>, String> {
        let spatial = self.input[0] * self.input[1];
        if logical.len() != self.channels * spatial * 4 {
            return Err("masked depthwise logical f32 input byte count differs".into());
        }
        let mut packed = vec![0; self.bytes()[1]];
        for channel in 0..self.channels {
            for y in 0..self.input[0] {
                for x in 0..self.input[1] {
                    let from = (channel * spatial + y * self.input[1] + x) * 4;
                    let value = half_storage(u32::from_le_bytes(
                        logical[from..from + 4].try_into().unwrap(),
                    ));
                    let to = (y * self.row_stride + x * self.channels + channel) * 2;
                    packed[to..to + 2].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        Ok(packed)
    }
    pub fn pack_weights(&self, logical: &[u8]) -> Result<Vec<u8>, String> {
        let area = self.kernel[0] * self.kernel[1];
        if logical.len() != self.channels * area * 2 {
            return Err("masked depthwise logical half weight byte count differs".into());
        }
        let mut packed = vec![0; self.bytes()[2]];
        for channel in 0..self.channels {
            for k in 0..area {
                let from = (channel * area + k) * 2;
                let to = (channel / 4 * self.weight_stride + k * 4 + channel % 4) * 2;
                packed[to..to + 2].copy_from_slice(&logical[from..from + 2]);
            }
        }
        Ok(packed)
    }
    /// Once-only logical-to-packed input producer. This is a separate graph
    /// call, so scratch allocation and its complete write lifetime are explicit.
    pub(super) fn input_artifact(
        &self,
        entry: &str,
    ) -> Result<super::matrix::graph::PipelineCall, String> {
        input_artifact(
            self.channels,
            self.input,
            self.row_stride,
            self.channels,
            self.bytes()[1],
            entry,
        )
    }

    pub fn artifact(&self, function: &str, entry: &str) -> Result<Artifact, String> {
        if !identifier(function) || !identifier(entry) || function == entry {
            return Err("invalid masked depthwise LLVM identifiers".into());
        }
        let mut ir = Ir::default();
        ir.parameters(&[
            ("%data0", 6, 0),
            ("%data1", 6, 1),
            ("%data2", 6, 2),
            ("%data3", 6, 3),
        ]);
        writeln!(ir.text,"define void @{function}(float* noalias align 32 %data0, i16* noalias align 32 %data1, i16* noalias align 32 %data2, i16* noalias align 32 %data3) #0 {{").unwrap();
        let blocks = self.channels / 4;
        let block = if blocks == 1 {
            "0".to_owned()
        } else {
            ir.begin_indexed_loop("channel_block", blocks, vec![2, 0])
        };
        let lane = ir.begin_indexed_loop(
            "channel_lane",
            4,
            if blocks == 1 { vec![2] } else { vec![2, 1] },
        );
        let channel = ir.op(format!("mul i32 {block}, 4"));
        let channel = ir.op(format!("add nsw i32 {channel}, {lane}"));
        let weight_base = ir.op(format!("mul i32 {block}, {}", self.weight_stride));
        let weight_base = ir.op(format!("add nsw i32 {weight_base}, {lane}"));
        let bias = ir.load_half("%data3", &channel);
        let spatial = self.output[0] * self.output[1];
        let mut values = vec![String::new(); spatial];
        // The masked-axis heuristic expands the last spatial axis first. The
        // reduction axes then fully expand; invalid coordinates disappear before
        // LOAD/MUL, so no padded zero product enters the accumulator.
        for x in 0..self.output[1] {
            for y in 0..self.output[0] {
                let mut accumulator = None;
                for ky in 0..self.kernel[0] {
                    for kx in 0..self.kernel[1] {
                        let iy = y as i128 + ky as i128 - self.pads[0] as i128;
                        let ix = x as i128 + kx as i128 - self.pads[1] as i128;
                        if iy < 0
                            || ix < 0
                            || iy >= self.input[0] as i128
                            || ix >= self.input[1] as i128
                        {
                            continue;
                        }
                        let input_index = ir.op(format!(
                            "add nsw i32 {channel}, {}",
                            iy as usize * self.row_stride + ix as usize * self.channels
                        ));
                        let input = ir.load_half("%data1", &input_index);
                        let weight_index = ir.op(format!(
                            "add nsw i32 {weight_base}, {}",
                            (ky * self.kernel[1] + kx) * 4
                        ));
                        let weight = ir.load_half("%data2", &weight_index);
                        let product = ir.op(format!(
                            "fmul nsz arcp contract afn float {input}, {weight}"
                        ));
                        accumulator = Some(match accumulator {
                            Some(previous) => ir.op(format!(
                                "fadd nsz arcp contract afn float {previous}, {product}"
                            )),
                            None => product,
                        });
                    }
                }
                values[y * self.output[1] + x] = if let Some(accumulator) = accumulator {
                    ir.op(format!(
                        "fadd nsz arcp contract afn float {accumulator}, {bias}"
                    ))
                } else {
                    bias.clone()
                };
            }
        }
        let store_lanes = if spatial.is_multiple_of(4) {
            4
        } else if spatial.is_multiple_of(2) {
            2
        } else {
            1
        };
        let base = ir.op(format!("mul i32 {channel}, {spatial}"));
        for offset in (0..spatial).step_by(store_lanes) {
            let index = ir.op(format!("add nsw i32 {base}, {offset}"));
            let pointer = ir.op(format!(
                "getelementptr inbounds float, float* %data0, i32 {index}"
            ));
            if store_lanes == 1 {
                ir.store_float(&pointer, &values[offset]);
            } else {
                ir.store_float_vector(&pointer, &values[offset..offset + store_lanes]);
            }
        }
        ir.end_loop("channel_lane");
        if blocks != 1 {
            ir.end_loop("channel_block");
        }
        ir.line("ret void");
        ir.text.push_str("}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(
            ir.text,
            "define void @{entry}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        for (slot, ty) in ["float", "i16", "i16", "i16"].iter().enumerate() {
            writeln!(ir.text,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %arg{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        writeln!(ir.text,"  call void @{function}(float* %arg0, i16* %arg1, i16* %arg2, i16* %arg3)\n  ret void\n}}").unwrap();
        Ok(Artifact {
            source: ir.text,
            bytes: self.bytes(),
            workers: 1,
            scalars: 0,
        })
    }
}

pub(super) fn input_artifact(
    channels: usize,
    input: [usize; 2],
    row_stride: usize,
    pixel_stride: usize,
    packed_bytes: usize,
    entry: &str,
) -> Result<super::matrix::graph::PipelineCall, String> {
    if !identifier(entry) {
        return Err("invalid convolution packing LLVM identifier".into());
    }
    let mut ir = Ir::default();
    writeln!(
        ir.text,
        "define void @{entry}(i8** %buffers, i32* %scalars) {{"
    )
    .unwrap();
    ir.text.push_str("  %slot0 = getelementptr inbounds i8*, i8** %buffers, i32 0\n  %raw0 = load i8*, i8** %slot0\n  %out = bitcast i8* %raw0 to i16*\n  %slot1 = getelementptr inbounds i8*, i8** %buffers, i32 1\n  %raw1 = load i8*, i8** %slot1\n  %input = bitcast i8* %raw1 to float*\n");
    // Alignment padding is not consumed numerically. Initialize it too so
    // the scratch producer owns the entire advertised physical byte range.
    let index = ir.begin_indexed_loop("zero", packed_bytes / 2, vec![0, 0]);
    let pointer = ir.op(format!(
        "getelementptr inbounds i16, i16* %out, i32 {index}"
    ));
    ir.line(format!("store i16 0, i16* {pointer}"));
    ir.end_loop("zero");
    let channel = ir.begin_indexed_loop("channel", channels, vec![0, 0]);
    let row = ir.begin_indexed_loop("row", input[0], vec![1, 0]);
    let column = ir.begin_indexed_loop("column", input[1], vec![2, 0]);
    let from = ir.op(format!("mul i32 {channel}, {}", input[0] * input[1]));
    let y = ir.op(format!("mul i32 {row}, {}", input[1]));
    let from = ir.op(format!("add nsw i32 {from}, {y}"));
    let from = ir.op(format!("add nsw i32 {from}, {column}"));
    let pointer = ir.op(format!(
        "getelementptr inbounds float, float* %input, i32 {from}"
    ));
    let value = ir.op(format!("load float, float* {pointer}"));
    let half = ir.store_half(&value);
    let to = ir.op(format!("mul i32 {row}, {}", row_stride));
    let x = ir.op(format!("mul i32 {column}, {}", pixel_stride));
    let to = ir.op(format!("add nsw i32 {to}, {x}"));
    let to = ir.op(format!("add nsw i32 {to}, {channel}"));
    let pointer = ir.op(format!("getelementptr inbounds i16, i16* %out, i32 {to}"));
    ir.line(format!("store i16 {half}, i16* {pointer}"));
    ir.end_loop("column");
    ir.end_loop("row");
    ir.end_loop("channel");
    ir.line("ret void");
    ir.text.push_str("}\n");
    Ok(super::matrix::graph::PipelineCall {
        source: ir.text,
        bytes: vec![packed_bytes, channels * input[0] * input[1] * 4],
        workers: 1,
        scalars: 0,
    })
}
