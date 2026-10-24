//! Source image_conv2d pointwise specialization with a fully expanded channel
//! reduction. Graph selection and wider heuristic schedules remain separate.
use super::{gcd, identifier, Artifact};
use crate::compiler_backend_cpu::image::{half_storage, Ir};
use std::fmt::Write;

pub struct Pointwise {
    input: [usize; 3],
    output_channels: usize,
    row_stride: usize,
    pixel_stride: usize,
    weight_stride: usize,
}
impl Pointwise {
    pub fn new(input: [usize; 3], output_channels: usize) -> Result<Self, String> {
        let [channels, height, width] = input;
        if channels == 0
            || !channels.is_multiple_of(4)
            || channels > 32
            || output_channels == 0
            || !output_channels.is_multiple_of(4)
            || height == 0
            || width == 0
        {
            return Err(
                "pointwise requires aligned channels and a fully unrolled reduction".into(),
            );
        }
        let spatial = height
            .checked_mul(width)
            .filter(|&n| n > 1)
            .ok_or("pointwise singleton spatial input has a distinct cin-last schedule")?;
        spatial
            .checked_mul(output_channels)
            .filter(|&n| n < 1024)
            .ok_or("pointwise requires the source schedule before extra output upcasts")?;
        let padded_output_width = width.next_multiple_of(32 / gcd(output_channels, 32));
        height
            .checked_mul(padded_output_width)
            .and_then(|n| n.checked_mul(output_channels))
            .filter(|&n| n < 1024)
            .ok_or("pointwise padded output requires an unproved upcast schedule")?;
        let elements = spatial
            .checked_mul(channels)
            .ok_or("pointwise input storage overflow")?;
        let blocks = channels / 4;
        let bank = channels >= 8 && blocks.is_power_of_two();
        // Retain the original image width here. Flattening H*W before pad_align
        // changes the bank-conflict channel stride for some spatial layouts.
        let align = if height == 1 || elements < 65536 {
            32
        } else {
            256
        };
        let pixel_stride = if bank {
            (blocks + 1).next_multiple_of(align / gcd(width * 4, align)) * 4
        } else {
            channels
        };
        let padded_width = if bank {
            width
        } else {
            width.next_multiple_of(align / gcd(channels, align))
        };
        let row_stride = padded_width * pixel_stride;
        let weight_align = if output_channels / 4 == 1 || channels * output_channels < 65536 {
            32
        } else {
            256
        };
        let weight_stride = if bank {
            (blocks + 1).next_multiple_of(weight_align / gcd(16, weight_align)) * 16
        } else {
            // pad_align expands the kernel-height axis, rather than rounding
            // the total row byte count; non-power-of-two channel blocks differ.
            (weight_align / gcd(blocks * 16, weight_align)) * blocks * 16
        };
        Ok(Self {
            input,
            output_channels,
            row_stride,
            pixel_stride,
            weight_stride,
        })
    }
    pub(super) fn bytes(&self) -> [usize; 4] {
        [
            self.output_channels * self.input[1] * self.input[2] * 4,
            self.input[1] * self.row_stride * 2,
            self.output_channels / 4 * self.weight_stride * 2,
            self.output_channels * 2,
        ]
    }
    pub fn pack_input(&self, logical: &[u8]) -> Result<Vec<u8>, String> {
        let [channels, height, width] = self.input;
        let spatial = height * width;
        if logical.len() != channels * spatial * 4 {
            return Err("pointwise logical f32 input byte count differs".into());
        }
        let mut result = vec![0; self.bytes()[1]];
        for channel in 0..channels {
            for y in 0..height {
                for x in 0..width {
                    let from = (channel * spatial + y * width + x) * 4;
                    let half = half_storage(u32::from_le_bytes(
                        logical[from..from + 4].try_into().unwrap(),
                    ));
                    let to = (y * self.row_stride + x * self.pixel_stride + channel) * 2;
                    result[to..to + 2].copy_from_slice(&half.to_le_bytes());
                }
            }
        }
        Ok(result)
    }
    pub fn pack_weights(&self, logical: &[u8]) -> Result<Vec<u8>, String> {
        let channels = self.input[0];
        if logical.len() != self.output_channels * channels * 2 {
            return Err("pointwise logical half weight byte count differs".into());
        }
        let mut result = vec![0; self.bytes()[2]];
        for output in 0..self.output_channels {
            for input in 0..channels {
                let from = (output * channels + input) * 2;
                let bits = u16::from_le_bytes(logical[from..from + 2].try_into().unwrap());
                // Original half contiguous materialization goes through float,
                // retaining payload bits and flushing emulated subnormals.
                let bits = if bits & 0x7c00 == 0 {
                    bits & 0x8000
                } else {
                    bits
                };
                let to = (output / 4 * self.weight_stride + input * 4 + output % 4) * 2;
                result[to..to + 2].copy_from_slice(&bits.to_le_bytes());
            }
        }
        Ok(result)
    }
    pub(super) fn input_artifact(
        &self,
        entry: &str,
    ) -> Result<crate::compiler_backend_cpu::matrix::graph::PipelineCall, String> {
        super::input_artifact(
            self.input[0],
            [self.input[1], self.input[2]],
            self.row_stride,
            self.pixel_stride,
            self.bytes()[1],
            entry,
        )
    }
    pub fn artifact(&self, function: &str, entry: &str) -> Result<Artifact, String> {
        if !identifier(function) || !identifier(entry) || function == entry {
            return Err("invalid pointwise LLVM identifiers".into());
        }
        let [channels, height, width] = self.input;
        let spatial = height * width;
        let mut ir = Ir::default();
        ir.parameters(&[
            ("%data0", 6, 0),
            ("%data1", 6, 1),
            ("%data2", 6, 2),
            ("%data3", 6, 3),
        ]);
        writeln!(ir.text,"define void @{function}(float* noalias align 32 %data0, i16* noalias align 32 %data1, i16* noalias align 32 %data2, i16* noalias align 32 %data3) #0 {{").unwrap();
        // A reduction's merged block/lane ranges retain their original range
        // numbers, even though the fully unrolled reduction emits no loop.
        let axis = if channels / 4 > 1 { 2 } else { 1 };
        let blocks = self.output_channels / 4;
        let block = if blocks > 1 {
            ir.begin_indexed_loop("out_block", blocks, vec![axis, 0])
        } else {
            "0".to_owned()
        };
        let lane = ir.begin_indexed_loop(
            "out_lane",
            4,
            if blocks > 1 {
                vec![axis, 1]
            } else {
                vec![axis]
            },
        );
        let output_channel = ir.op(format!("mul i32 {block}, 4"));
        let output_channel = ir.op(format!("add nsw i32 {output_channel}, {lane}"));
        let weight_base = ir.op(format!("mul i32 {block}, {}", self.weight_stride));
        let weight_base = ir.op(format!("add nsw i32 {weight_base}, {lane}"));
        let weights = ir.half_values("%data2", &weight_base, channels, 4, false);
        let bias = ir.load_half("%data3", &output_channel);
        // Adjacent H/W ranges merge iff all affine strides agree. Input row
        // padding prevents that merge; keep the original separate ranges then.
        let merged = self.row_stride == width * self.pixel_stride;
        let (pixel, input_base) = if merged {
            let pixel = ir.begin_indexed_loop("pixel", spatial, vec![axis + 1]);
            let base = ir.op(format!("mul i32 {pixel}, {}", self.pixel_stride));
            (pixel, base)
        } else {
            let row = if height > 1 {
                ir.begin_indexed_loop("row", height, vec![axis + 1])
            } else {
                "0".to_owned()
            };
            let column = if width > 1 {
                ir.begin_indexed_loop("column", width, vec![axis + 1 + i64::from(height > 1)])
            } else {
                "0".to_owned()
            };
            let pixel = ir.op(format!("mul i32 {row}, {width}"));
            let pixel = ir.op(format!("add nsw i32 {pixel}, {column}"));
            let base = ir.op(format!("mul i32 {row}, {}", self.row_stride));
            let x = ir.op(format!("mul i32 {column}, {}", self.pixel_stride));
            let base = ir.op(format!("add nsw i32 {base}, {x}"));
            (pixel, base)
        };
        let inputs = ir.half_values("%data1", &input_base, channels, 1, true);
        let mut accumulator = None;
        for (input, weight) in inputs.iter().zip(&weights) {
            let product = ir.op(format!(
                "fmul nsz arcp contract afn float {input}, {weight}"
            ));
            accumulator = Some(match accumulator {
                None => product,
                Some(previous) => ir.op(format!(
                    "fadd nsz arcp contract afn float {previous}, {product}"
                )),
            });
        }
        let value = ir.op(format!(
            "fadd nsz arcp contract afn float {}, {bias}",
            accumulator.unwrap()
        ));
        let output = ir.op(format!("mul i32 {output_channel}, {spatial}"));
        let output = ir.op(format!("add nsw i32 {output}, {pixel}"));
        let pointer = ir.op(format!(
            "getelementptr inbounds float, float* %data0, i32 {output}"
        ));
        ir.store_float(&pointer, &value);
        if merged {
            ir.end_loop("pixel");
        } else {
            if width > 1 {
                ir.end_loop("column");
            }
            if height > 1 {
                ir.end_loop("row");
            }
        }
        ir.end_loop("out_lane");
        if blocks > 1 {
            ir.end_loop("out_block");
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
