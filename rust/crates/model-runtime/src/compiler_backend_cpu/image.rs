//! Packed IMAGE 1x1 residual kernel boundary, not whole-ONNX admission.
//! Layout and reduction rules are derived from tinygrad tensor.image_conv2d,
//! simplify_merge_adjacent and hand_coded_optimizations (see fixture provenance).
//! Source algorithm copyright (c) 2024, the tiny corp; MIT license retained in
//! tinygrad_repo/LICENSE. Native adaptation does not admit general convolutions.
use std::{collections::BTreeMap, fmt::Write, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidualStorage {
    LogicalF32,
    PackedHalf,
}
#[derive(Clone, Debug)]
pub struct ResidualClosure {
    pub image: Image1x1,
    pub input: usize,
    pub weights: usize,
    pub convolution: usize,
    pub output: usize,
    pub residual_storage: ResidualStorage,
    pub retain_convolution: bool,
    pub retain_input: bool,
}
/// Inspect the logical source graph before physical buffer scheduling. No
/// admission or graph mutation occurs here.
pub fn residual_closures(
    graph: &crate::compiler_onnx::lower::LoweredGraph,
    model: &crate::compiler_onnx::Model,
) -> Result<Vec<ResidualClosure>, String> {
    use crate::compiler_onnx::{lower::ScalarExpr, Attribute};
    let mut result = Vec::new();
    let live = crate::compiler_onnx::schedule::schedule_plans(graph, "CPU", true)?.live;
    for conv in model.graph.nodes.iter().filter(|n| n.op == "Conv") {
        if conv.inputs.len() != 2 || conv.outputs.len() != 1 {
            continue;
        }
        let Some((&input, &weights, &convolution)) = graph
            .names
            .get(&conv.inputs[0])
            .zip(graph.names.get(&conv.inputs[1]))
            .zip(graph.names.get(&conv.outputs[0]))
            .map(|((a, b), c)| (a, b, c))
        else {
            continue;
        };
        let x = &graph.plans[input].ty;
        let w = &graph.plans[weights].ty;
        if x.dtype != 1
            || w.dtype != 1
            || x.shape.len() != 4
            || w.shape.len() != 4
            || x.shape[0] != 1
            || x.shape[2] != 1
            || x.shape[1] != w.shape[0]
            || w.shape[1] != x.shape[1]
            || w.shape[2..] != [1, 1]
            || !matches!(graph.plans[weights].expression, ScalarExpr::Constant(_))
        {
            continue;
        }
        let image = Image1x1 {
            channels_in: x.shape[1],
            channels_out: w.shape[0],
            width: x.shape[3],
        };
        if image.layout().is_err() {
            continue;
        }
        let plain = conv
            .attributes
            .iter()
            .all(|(name, value)| match (name.as_str(), value) {
                ("group", Attribute::Integer(1)) => true,
                ("strides" | "dilations" | "kernel_shape", Attribute::Integers(v)) => v == &[1, 1],
                ("pads", Attribute::Integers(v)) => v.iter().all(|v| *v == 0),
                ("auto_pad", Attribute::String(v)) => matches!(v.as_str(), "NOTSET" | "VALID"),
                _ => false,
            });
        if !plain {
            continue;
        }
        for add in model.graph.nodes.iter().filter(|n| {
            n.op == "Add"
                && n.inputs == [conv.outputs[0].clone(), conv.inputs[0].clone()]
                && n.outputs.len() == 1
        }) {
            let Some(&output) = graph.names.get(&add.outputs[0]) else {
                continue;
            };
            if !live.contains(&output) || graph.plans[output].ty != *x {
                continue;
            }
            let retained = |name: &str| {
                model.graph.outputs.iter().any(|v| v.name == name)
                    || model
                        .graph
                        .nodes
                        .iter()
                        .any(|n| n != conv && n != add && n.inputs.iter().any(|v| v == name))
            };
            result.push(ResidualClosure {
                image,
                input,
                weights,
                convolution,
                output,
                residual_storage: if matches!(
                    graph.plans[input].expression,
                    ScalarExpr::Input(_) | ScalarExpr::Constant(_)
                ) {
                    ResidualStorage::LogicalF32
                } else {
                    ResidualStorage::PackedHalf
                },
                retain_convolution: retained(&conv.outputs[0]),
                retain_input: retained(&conv.inputs[0]),
            });
        }
    }
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub struct Image1x1 {
    pub channels_in: usize,
    pub channels_out: usize,
    pub width: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub input_pixel_stride: usize,
    pub input_elements: usize,
    pub weight_output_stride: usize,
    pub weight_elements: usize,
    /// Strides of the input-channel block and input-channel lane in weights.
    pub weight_reduction_strides: [usize; 2],
    pub reduction_axes: Vec<usize>,
    pub unroll: usize,
}

fn round_up(n: usize, alignment: usize) -> usize {
    n.div_ceil(alignment) * alignment
}
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Image1x1 {
    pub fn logical_artifact(
        &self,
        function: &str,
        entrypoint: &str,
    ) -> Result<LogicalArtifactKernel, String> {
        let packed = self.artifact(function, entrypoint)?;
        let staging_name = format!("{function}_stage");
        // Compile the staging module separately. Keeping its physical half
        // stores across a real object boundary preserves original program
        // materialization instead of inviting cross-stage FP contraction.
        let mut staging = Ir::default();
        writeln!(staging.text,"define void @{staging_name}(float* noalias align 32 %input, i16* noalias align 32 %weights) {{").unwrap();
        let layout = self.layout()?;
        let channel = staging.begin_loop("channel", self.channels_in);
        let pixel = staging.begin_loop("stage_pixel", self.width);
        let from = staging.op(format!("mul i32 {channel}, {}", self.width));
        let from = staging.op(format!("add nsw i32 {from}, {pixel}"));
        let ptr = staging.op(format!(
            "getelementptr inbounds float, float* %input, i32 {from}"
        ));
        let value = staging.op(format!("load float, float* {ptr}"));
        let half = staging.store_half(&value);
        let to = staging.op(format!("mul i32 {pixel}, {}", layout.input_pixel_stride));
        let to = staging.op(format!("add nsw i32 {to}, {channel}"));
        let ptr = staging.op(format!(
            "getelementptr inbounds i16, i16* %weights, i32 {to}"
        ));
        staging.line(format!("store i16 {half}, i16* {ptr}"));
        staging.end_loop("stage_pixel");
        staging.end_loop("channel");
        staging.line("ret void");
        staging.text.push_str("}\n");
        // Replace only the trampoline with a four-buffer runtime entrypoint.
        let mut source = self.render(function)?;
        writeln!(source,"declare void @{staging_name}(float*, i16*)\ndefine void @{entrypoint}(i8** %buffers, i32* %scalars) {{").unwrap();
        for (slot, ty) in ["float", "float", "i16", "i16"].iter().enumerate() {
            writeln!(source,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        writeln!(source,"  call void @{staging_name}(float* %ptr1, i16* %ptr2)\n  call void @{function}(float* %ptr0, i16* %ptr2, i16* %ptr3)\n  ret void\n}}").unwrap();
        Ok(LogicalArtifactKernel {
            source,
            staging_source: staging.text,
            buffers: [
                BufferKind::F32,
                BufferKind::F32,
                BufferKind::U16,
                BufferKind::U16,
            ],
            bytes: [
                packed.bytes[0],
                self.channels_in * self.width * 4,
                packed.bytes[1],
                packed.bytes[2],
            ],
            scalars: 0,
        })
    }
    /// Materialize logical NCHW float32 as IMAGE half storage. Invalid padding
    /// is never written, matching original gated contiguous stores.
    pub fn pack_input_into(&self, logical: &[u8], packed: &mut [u8]) -> Result<(), String> {
        let layout = self.layout()?;
        if logical.len() != self.channels_in * self.width * 4
            || packed.len() != layout.input_elements * 2
        {
            return Err("IMAGE input materialization byte count mismatch".into());
        }
        for channel in 0..self.channels_in {
            for pixel in 0..self.width {
                let from = (channel * self.width + pixel) * 4;
                let to = (pixel * layout.input_pixel_stride + channel) * 2;
                packed[to..to + 2].copy_from_slice(
                    &half_storage(u32::from_le_bytes(
                        logical[from..from + 4].try_into().unwrap(),
                    ))
                    .to_le_bytes(),
                );
            }
        }
        Ok(())
    }
    pub fn pack_weights_into(&self, logical: &[u8], packed: &mut [u8]) -> Result<(), String> {
        let layout = self.layout()?;
        if logical.len() != self.channels_in * self.channels_out * 4
            || packed.len() != layout.weight_elements * 2
        {
            return Err("IMAGE weight materialization byte count mismatch".into());
        }
        let out_lane_stride = if self.width == 1 { 4 } else { 1 };
        for output in 0..self.channels_out {
            for input in 0..self.channels_in {
                let from = (output * self.channels_in + input) * 4;
                let to = (output / 4 * layout.weight_output_stride
                    + input / 4 * layout.weight_reduction_strides[0]
                    + output % 4 * out_lane_stride
                    + input % 4 * layout.weight_reduction_strides[1])
                    * 2;
                packed[to..to + 2].copy_from_slice(
                    &half_storage(u32::from_le_bytes(
                        logical[from..from + 4].try_into().unwrap(),
                    ))
                    .to_le_bytes(),
                );
            }
        }
        Ok(())
    }
    pub fn artifact(&self, function: &str, entrypoint: &str) -> Result<ArtifactKernel, String> {
        let index = entrypoint
            .strip_prefix("op_kernel_")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| *n < 1_000_000);
        if index.is_none_or(|n| entrypoint != format!("op_kernel_{n}")) || function == entrypoint {
            return Err("invalid or colliding IMAGE artifact entrypoint".into());
        }
        let layout = self.layout()?;
        let mut source = self.render(function)?;
        writeln!(
            source,
            "define void @{entrypoint}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        for (slot, ty) in ["float", "i16", "i16"].iter().enumerate() {
            writeln!(source, "  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        writeln!(
            source,
            "  call void @{function}(float* %ptr0, i16* %ptr1, i16* %ptr2)\n  ret void\n}}"
        )
        .unwrap();
        Ok(ArtifactKernel {
            source,
            buffers: [BufferKind::F32, BufferKind::U16, BufferKind::U16],
            bytes: [
                self.channels_out * self.width * 4,
                layout.input_elements * 2,
                layout.weight_elements * 2,
            ],
            scalars: 0,
        })
    }
    pub fn layout(&self) -> Result<Layout, String> {
        if self.channels_in == 0
            || !self.channels_in.is_multiple_of(4)
            || self.channels_in != self.channels_out
            || self.width == 0
        {
            return Err("packed 1x1 residual requires equal nonzero four-aligned channels and nonzero width".into());
        }
        // Bound all materialized buffers and i32 index arithmetic before computing
        // pad_align products. This boundary does not admit larger kernels.
        if self.channels_in > 4096 || self.width > 65536 {
            return Err("packed IMAGE kernel exceeds materialization bound".into());
        }
        let blocks = self.channels_in / 4;
        let bank_conflict = self.channels_in >= 8 && blocks.is_power_of_two();
        let input_pixel_stride = if bank_conflict {
            round_up(blocks + 1, 32 / gcd(self.width * 4, 32)) * 4
        } else {
            self.channels_in
        };
        let input_elements = if bank_conflict {
            self.width * input_pixel_stride
        } else {
            round_up(self.width, 32 / gcd(self.channels_in, 32)) * self.channels_in
        };
        let weight_align = if blocks == 1 || self.channels_in * self.channels_out < 65536 {
            32
        } else {
            256
        };
        let weight_output_stride = if bank_conflict {
            round_up(blocks + 1, weight_align / gcd(16, weight_align)) * 16
        } else {
            round_up(1, weight_align / gcd(blocks * 16, weight_align)) * blocks * 16
        };
        let weight_reduction_strides = if self.width == 1 { [16, 1] } else { [16, 4] };
        // For affine indices the original div/mod merge simplifies precisely
        // when the outer stride equals inner_extent * inner_stride for every
        // buffer. Input strides are [4,1]; weights decide this merge.
        let merge = weight_reduction_strides[0] == 4 * weight_reduction_strides[1];
        let reduction_axes = if merge {
            vec![self.channels_in]
        } else if blocks == 1 {
            vec![4]
        } else {
            vec![blocks, 4]
        };
        let last = *reduction_axes.last().unwrap();
        let unroll = if last <= 32 { last } else { 4 };
        let weight_elements = blocks * weight_output_stride;
        if input_elements > i32::MAX as usize / 2 || weight_elements > i32::MAX as usize / 2 {
            return Err("packed IMAGE address exceeds i32 bound".into());
        }
        Ok(Layout {
            input_pixel_stride,
            input_elements,
            weight_output_stride,
            weight_elements,
            weight_reduction_strides,
            reduction_axes,
            unroll,
        })
    }

    /// Input/weights are already materialized emulated-half IMAGE storage. The
    /// output is float32 NCHW. Residual addition follows the reduction, as in the
    /// independently captured final ImageMulAdd kernel.
    pub fn render(&self, function: &str) -> Result<String, String> {
        let layout = self.layout()?;
        if function.is_empty()
            || !function
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
        {
            return Err("invalid LLVM function identifier".into());
        }
        let mut ir = Ir::default();
        writeln!(ir.text, "define void @{function}(float* noalias align 32 %out, i16* noalias align 32 %input, i16* noalias align 32 %weights) #0 {{").unwrap();
        let loop_count = layout.reduction_axes.iter().product::<usize>() / layout.unroll;
        if loop_count > 1 {
            ir.line("%acc = alloca float");
        }
        let block = ir.begin_loop("out_block", self.channels_out / 4);
        let weight_block = ir.op(format!("mul i32 {block}, {}", layout.weight_output_stride));
        let channel_block = ir.op(format!("mul i32 {block}, 4"));
        let lane = ir.begin_loop("out_lane", 4);
        let channel = ir.op(format!("add nsw i32 {channel_block}, {lane}"));
        let weight_lane_stride = if self.width == 1 { 4 } else { 1 };
        let weight_lane = ir.op(format!("mul i32 {lane}, {weight_lane_stride}"));
        let weight_base = ir.op(format!("add nsw i32 {weight_block}, {weight_lane}"));
        let weights = if loop_count == 1 {
            Some(ir.half_values(
                "%weights",
                &weight_base,
                layout.unroll,
                layout.weight_reduction_strides[1],
                self.width == 1,
            ))
        } else {
            None
        };
        let pixel = if self.width > 1 {
            ir.begin_loop("pixel", self.width)
        } else {
            "0".into()
        };
        let input_base = ir.op(format!("mul i32 {pixel}, {}", layout.input_pixel_stride));
        if loop_count > 1 {
            ir.line("store float 0.0, float* %acc");
        }
        let reduce = if loop_count > 1 {
            ir.begin_loop("reduce", loop_count)
        } else {
            "0".into()
        };
        let start = ir.op(format!("mul i32 {reduce}, {}", layout.unroll));
        let input_start = ir.op(format!("add nsw i32 {input_base}, {start}"));
        let inputs = ir.half_values("%input", &input_start, layout.unroll, 1, true);
        let weights = weights.unwrap_or_else(|| {
            let weight_start = ir.op(format!("mul i32 {reduce}, 16"));
            let weight_start = ir.op(format!("add nsw i32 {weight_start}, {weight_base}"));
            ir.half_values(
                "%weights",
                &weight_start,
                layout.unroll,
                layout.weight_reduction_strides[1],
                self.width == 1,
            )
        });
        let mut accumulator = if loop_count > 1 {
            Some(ir.op("load float, float* %acc"))
        } else {
            None
        };
        for (x, w) in inputs.iter().zip(&weights) {
            let product = ir.op(format!("fmul nsz arcp contract afn float {x}, {w}"));
            // Fully expanded REDUCE loses the identity ADD during original
            // symbolic rewriting. Looped REDUCE keeps its explicit +0 buffer.
            accumulator = Some(match accumulator {
                None => product,
                Some(a) => ir.op(format!("fadd nsz arcp contract afn float {a}, {product}")),
            });
        }
        let mut accumulator = accumulator.unwrap();
        if loop_count > 1 {
            ir.line(format!("store float {accumulator}, float* %acc"));
            ir.end_loop("reduce");
            accumulator = ir.op("load float, float* %acc");
        }
        let residual_index = ir.op(format!("add nsw i32 {input_base}, {channel}"));
        let residual = ir.load_half("%input", &residual_index);
        let value = ir.op(format!(
            "fadd nsz arcp contract afn float {accumulator}, {residual}"
        ));
        let output_base = ir.op(format!("mul i32 {channel}, {}", self.width));
        let output_index = ir.op(format!("add nsw i32 {output_base}, {pixel}"));
        let output_ptr = ir.op(format!(
            "getelementptr inbounds float, float* %out, i32 {output_index}"
        ));
        ir.line(format!("store float {value}, float* {output_ptr}"));
        if self.width > 1 {
            ir.end_loop("pixel");
        }
        ir.end_loop("out_lane");
        ir.end_loop("out_block");
        ir.line("ret void");
        ir.text.push_str("}\nattributes #0 = { alwaysinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        Ok(ir.text)
    }
}

fn half_storage(mut bits: u32) -> u16 {
    let magnitude = bits & 0x7fff_ffff;
    if magnitude > 1199562752 && magnitude <= 0x7f80_0000 {
        bits = bits & 0x8000_0000 | 0x7f80_0000;
    }
    let magnitude = bits & 0x7fff_ffff;
    let sign = bits >> 16 & 0x8000;
    let exponent = magnitude >> 23;
    let rounded = (magnitude >> 13)
        + ((magnitude >> 12 & 1) & u32::from(magnitude & 4095 != 0 || magnitude >> 13 & 1 != 0));
    (sign
        | if exponent == 255 {
            0x7c00 | (magnitude >> 13 & 1023)
        } else if exponent < 113 {
            0
        } else {
            rounded.wrapping_sub(114688)
        }) as u16
}

/// Fixed synchronous runtime ABI; buffer order and required view bytes are
/// explicit rather than inferred by a C wrapper or environment defaults.
pub struct ArtifactKernel {
    pub source: String,
    pub buffers: [BufferKind; 3],
    pub bytes: [usize; 3],
    pub scalars: usize,
}
pub struct LogicalArtifactKernel {
    pub source: String,
    pub staging_source: String,
    pub buffers: [BufferKind; 4],
    pub bytes: [usize; 4],
    pub scalars: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    F32,
    U16,
}

#[derive(Default)]
struct Ir {
    text: String,
    next: usize,
    nodes: BTreeMap<String, Rc<Key>>,
    available: BTreeMap<Rc<Key>, String>,
    pending: Vec<Instruction>,
    scopes: Vec<Scope>,
}
struct Scope {
    available: BTreeMap<Rc<Key>, String>,
    nodes: BTreeMap<String, Rc<Key>>,
}

// Relative Ops order, dtype order and tuple ordering from retained tinygrad.
// These are semantic keys, independent of SSA numbering and allocation order.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    op: u8,
    arg: Vec<i64>,
    dtype: u8,
    src: Vec<Rc<Key>>,
}
struct Instruction {
    name: String,
    text: String,
    key: Rc<Key>,
    deps: Vec<String>,
}

impl Ir {
    fn line(&mut self, line: impl AsRef<str>) {
        self.flush();
        writeln!(self.text, "  {}", line.as_ref()).unwrap();
    }
    fn op(&mut self, expression: impl AsRef<str>) -> String {
        let expression = expression.as_ref();
        let tokens = expression
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if tokens[0] == "add" && tokens.last() == Some(&"0") {
            return tokens[tokens.len() - 2].into();
        }
        if tokens[0] == "mul" {
            let rhs = tokens.last().unwrap().parse::<usize>().unwrap();
            let lhs = tokens[tokens.len() - 2];
            if rhs == 0 || lhs == "0" {
                return "0".into();
            }
            if rhs == 1 {
                return lhs.into();
            }
            if rhs.is_power_of_two() {
                return self.op(format!("shl i32 {lhs}, {}", rhs.trailing_zeros()));
            }
        }
        let op = match tokens[0] {
            "extractelement" => 16,
            "getelementptr" => 21,
            "load" => 23,
            "zext" => 27,
            "trunc" => 27,
            "bitcast" => 28,
            "add" | "fadd" => 34,
            "mul" | "fmul" => 35,
            "shl" => 36,
            "lshr" => 37,
            "icmp" => {
                if tokens[1] == "eq" {
                    43
                } else {
                    41
                }
            }
            "or" => 45,
            "and" => 46,
            "select" => 53,
            "phi" => 56,
            _ => panic!("unsupported bounded IMAGE instruction: {expression}"),
        };
        let dtype = if tokens[0] == "icmp" {
            0
        } else if tokens[0] == "bitcast" && tokens.last() == Some(&"i32") {
            6
        } else if tokens[0] == "bitcast" || expression.contains("float") {
            13
        } else if expression.contains("i16") && tokens[0] != "zext" {
            4
        } else if matches!(tokens[0], "zext" | "lshr" | "or" | "and" | "select")
            || tokens
                .iter()
                .find_map(|s| self.nodes.get(*s))
                .is_some_and(|k| k.dtype == 6)
        {
            6
        } else {
            5
        };
        let mut deps = Vec::new();
        let mut src = Vec::new();
        for token in &tokens[1..] {
            if token.starts_with('%') {
                if let Some(key) = self.nodes.get(*token) {
                    deps.push((*token).into());
                    src.push(key.clone());
                } else if let Some(slot) = ["%out", "%input", "%weights", "%acc"]
                    .iter()
                    .position(|v| v == token)
                {
                    src.push(Rc::new(Key {
                        op: if slot == 3 { 3 } else { 6 },
                        arg: vec![slot as i64],
                        dtype,
                        src: vec![],
                    }));
                }
            } else if let Ok(n) = token.parse::<i64>() {
                src.push(Rc::new(Key {
                    op: 61,
                    arg: vec![n],
                    dtype,
                    src: vec![],
                }));
            }
        }
        let arg = if tokens[0] == "extractelement" {
            vec![tokens.last().unwrap().parse().unwrap()]
        } else {
            vec![]
        };
        if tokens[0] == "extractelement" {
            src.pop();
        }
        let key = Rc::new(Key {
            op,
            arg,
            dtype,
            src,
        });
        if let Some(existing) = self.available.get(&key) {
            if tokens[0] != "phi" && !expression.contains("%acc") {
                return existing.clone();
            }
        }
        let name = format!("%v{}", self.next);
        self.next += 1;
        self.pending.push(Instruction {
            text: format!("{name} = {expression}"),
            name: name.clone(),
            key: key.clone(),
            deps,
        });
        self.nodes.insert(name.clone(), key.clone());
        self.available.insert(key, name.clone());
        name
    }
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let indices = pending
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.as_str(), i))
            .collect::<BTreeMap<_, _>>();
        let mut consumers = vec![0; pending.len()];
        for node in &pending {
            for src in &node.deps {
                if let Some(&i) = indices.get(src.as_str()) {
                    consumers[i] += 1;
                }
            }
        }
        let mut ideal = (0..pending.len()).collect::<Vec<_>>();
        ideal.sort_by(|&a, &b| {
            let priority = |i: usize| if pending[i].key.op == 23 { -1 } else { 0 };
            (priority(a), &pending[a].key).cmp(&(priority(b), &pending[b].key))
        });
        let mut ranks = vec![0; pending.len()];
        for (rank, i) in ideal.iter().enumerate() {
            ranks[*i] = rank;
        }
        let mut ready = std::collections::BinaryHeap::new();
        for (i, &n) in consumers.iter().enumerate() {
            if n == 0 {
                ready.push((ranks[i], i));
            }
        }
        let mut order = Vec::new();
        while let Some((_, i)) = ready.pop() {
            order.push(i);
            for src in &pending[i].deps {
                if let Some(&j) = indices.get(src.as_str()) {
                    consumers[j] -= 1;
                    if consumers[j] == 0 {
                        ready.push((ranks[j], j));
                    }
                }
            }
        }
        assert_eq!(order.len(), pending.len());
        for i in order.into_iter().rev() {
            writeln!(self.text, "  {}", pending[i].text).unwrap();
        }
    }
    fn begin_loop(&mut self, name: &str, count: usize) -> String {
        self.scopes.push(Scope {
            available: self.available.clone(),
            nodes: self.nodes.clone(),
        });
        self.line(format!(
            "br label %{name}_entry\n{name}_entry:\n  br label %{name}_latch\n{name}_latch:"
        ));
        let value = self.op(format!(
            "phi i32 [0, %{name}_entry], [%{name}_next, %{name}_footer]"
        ));
        let arg = match name {
            "out_block" => vec![2, 0, 0],
            "out_lane" => vec![2, 1, 0],
            "pixel" => vec![3, 0],
            "reduce" => vec![0, 1],
            "channel" => vec![0, 0],
            "stage_pixel" => vec![1, 0],
            _ => unreachable!(),
        };
        self.nodes.insert(
            value.clone(),
            Rc::new(Key {
                op: 56,
                arg,
                dtype: 5,
                src: vec![],
            }),
        );
        self.line(format!("%{name}_next = add i32 {value}, 1"));
        let condition = self.op(format!("icmp ult i32 {value}, {count}"));
        self.line(format!(
            "br i1 {condition}, label %{name}_body, label %{name}_exit\n{name}_body:"
        ));
        value
    }
    fn end_loop(&mut self, name: &str) {
        self.line(format!(
            "br label %{name}_footer\n{name}_footer:\n  br label %{name}_latch\n{name}_exit:"
        ));
        let scope = self.scopes.pop().unwrap();
        self.available = scope.available;
        self.nodes = scope.nodes;
    }
    fn half_values(
        &mut self,
        buffer: &str,
        base: &str,
        count: usize,
        stride: usize,
        vectorize: bool,
    ) -> Vec<String> {
        let mut values = Vec::new();
        for offset in (0..count).step_by(4) {
            let group_offset = self.op(format!("add nsw i32 {base}, {}", offset * stride));
            let mut group = Vec::new();
            for lane in 0..4 {
                let index = self.op(format!("add nsw i32 {group_offset}, {}", lane * stride));
                group.push(self.load_half(buffer, &index));
            }
            if vectorize {
                let key = Rc::new(Key {
                    op: 17,
                    arg: vec![],
                    dtype: 13,
                    src: group.iter().map(|s| self.nodes[s].clone()).collect(),
                });
                let name = format!("%v{}", self.next);
                self.next += 1;
                let mut text = String::new();
                let mut previous = "poison".to_owned();
                for (lane, value) in group.iter().enumerate() {
                    let dest = if lane == 3 {
                        name.clone()
                    } else {
                        format!("{name}_{lane}")
                    };
                    writeln!(
                        text,
                        "{dest} = insertelement <4 x float> {previous}, float {value}, i32 {lane}"
                    )
                    .unwrap();
                    previous = dest;
                }
                self.pending.push(Instruction {
                    name: name.clone(),
                    text,
                    key: key.clone(),
                    deps: group,
                });
                self.nodes.insert(name.clone(), key);
                for lane in 0..4 {
                    values.push(self.op(format!("extractelement <4 x float> {name}, i32 {lane}")));
                }
            } else {
                values.extend(group);
            }
        }
        values
    }
    fn load_half(&mut self, buffer: &str, index: &str) -> String {
        let ptr = self.op(format!(
            "getelementptr inbounds i16, i16* {buffer}, i32 {index}"
        ));
        let raw = self.op(format!("load i16, i16* {ptr}"));
        // Original non-native half decomposition: subnormals flush to signed
        // zero; infinity and NaN retain sign and the complete ten payload bits.
        let magnitude = self.op(format!("and i16 {raw}, 32767"));
        let magnitude = self.op(format!("zext i16 {magnitude} to i32"));
        let sign = self.op(format!("and i16 {raw}, 32768"));
        let sign = self.op(format!("zext i16 {sign} to i32"));
        let mantissa = self.op(format!("shl i32 {magnitude}, 13"));
        let finite = self.op(format!("add i32 {mantissa}, 939524096"));
        let sign = self.op(format!("shl i32 {sign}, 16"));
        let exponent = self.op(format!("lshr i32 {magnitude}, 10"));
        let zero = self.op(format!("icmp eq i32 {exponent}, 0"));
        let special = self.op(format!("icmp eq i32 {exponent}, 31"));
        let infinity = self.op(format!("or i32 {mantissa}, 2139095040"));
        let bits = self.op(format!("select i1 {special}, i32 {infinity}, i32 {finite}"));
        let bits = self.op(format!("select i1 {zero}, i32 0, i32 {bits}"));
        let bits = self.op(format!("or i32 {sign}, {bits}"));
        self.op(format!("bitcast i32 {bits} to float"))
    }
    fn store_half(&mut self, value: &str) -> String {
        let raw = self.op(format!("bitcast float {value} to i32"));
        let mag = self.op(format!("and i32 {raw}, 2147483647"));
        let overflow = self.op(format!("icmp ult i32 1199562752, {mag}"));
        let finite = self.op(format!("icmp ult i32 {mag}, 2139095041"));
        let overflow = self.op(format!("and i1 {overflow}, {finite}"));
        let sign = self.op(format!("and i32 {raw}, 2147483648"));
        let infinity = self.op(format!("or i32 {sign}, 2139095040"));
        let raw = self.op(format!("select i1 {overflow}, i32 {infinity}, i32 {raw}"));
        let mag = self.op(format!("and i32 {raw}, 2147483647"));
        let high = self.op(format!("lshr i32 {mag}, 13"));
        let low = self.op(format!("and i32 {mag}, 4095"));
        let odd = self.op(format!("and i32 {high}, 1"));
        let low_zero = self.op(format!("icmp eq i32 {low}, 0"));
        let lower = self.op(format!("select i1 {low_zero}, i32 {odd}, i32 1"));
        let round = self.op(format!("lshr i32 {mag}, 12"));
        let round = self.op(format!("and i32 {round}, 1"));
        let round = self.op(format!("and i32 {round}, {lower}"));
        let rounded = self.op(format!("add i32 {high}, {round}"));
        let normal = self.op(format!("add i32 {rounded}, -114688"));
        let exp = self.op(format!("lshr i32 {mag}, 23"));
        let small = self.op(format!("icmp ult i32 {exp}, 113"));
        let normal = self.op(format!("select i1 {small}, i32 0, i32 {normal}"));
        let special = self.op(format!("icmp eq i32 {exp}, 255"));
        let payload = self.op(format!("and i32 {high}, 1023"));
        let special_bits = self.op(format!("or i32 {payload}, 31744"));
        let result = self.op(format!(
            "select i1 {special}, i32 {special_bits}, i32 {normal}"
        ));
        let sign = self.op(format!("lshr i32 {raw}, 16"));
        let sign = self.op(format!("and i32 {sign}, 32768"));
        let bits = self.op(format!("or i32 {result}, {sign}"));
        self.op(format!("trunc i32 {bits} to i16"))
    }
}
