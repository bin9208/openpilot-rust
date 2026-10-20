//! Source-owned pointwise Conv, tanh GELU, spatial Mean closure. Selection is
//! independent of node numbers/names and retains the serial path on failure.
use crate::compiler_onnx::{
    lower::{LoweredGraph, ScalarExpr},
    memory::{BufferSpec, Call},
    schedule::{schedule_plans, Schedule},
    Attribute, Model, Node,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::{fs, path::PathBuf};

type PipelineCall = super::matrix::graph::PipelineCall;

pub(super) struct Closure {
    input: usize,
    weight: usize,
    bias: usize,
    output: usize,
    channels_in: usize,
    channels_out: usize,
    width: usize,
    workers: usize,
    pub owned: BTreeSet<usize>,
}

impl Closure {
    pub(super) fn output(&self) -> usize {
        self.output
    }
}

pub(super) fn validate_physical(selected: &[Closure], graph: &LoweredGraph) -> Result<(), String> {
    if selected.is_empty() {
        return Ok(());
    }
    let schedule = schedule_plans(graph, "CPU", true)?;
    for closure in selected {
        if !schedule.live.contains(&closure.output)
            || graph.plans[closure.output].ty.shape != [1, closure.channels_out, 1, 1]
            || schedule.calls.iter().any(|call| {
                !closure.owned.contains(&call.buffers[0])
                    && call.buffers[1..]
                        .iter()
                        .any(|id| closure.owned.contains(id))
            })
        {
            return Err("fused CPU closure lost exclusive physical ownership".into());
        }
    }
    Ok(())
}

fn producer<'a>(model: &'a Model, name: &str, op: &str) -> Option<&'a Node> {
    model
        .graph
        .nodes
        .iter()
        .find(|node| node.op == op && node.outputs.len() == 1 && node.outputs[0] == name)
}

fn plain_pointwise(node: &Node) -> bool {
    node.attributes
        .iter()
        .all(|(name, value)| match (name.as_str(), value) {
            ("group", Attribute::Integer(1)) => true,
            ("strides" | "dilations" | "kernel_shape", Attribute::Integers(v)) => v == &[1, 1],
            ("pads", Attribute::Integers(v)) => v == &[0, 0, 0, 0],
            ("auto_pad", Attribute::String(v)) => matches!(v.as_str(), "NOTSET" | "VALID"),
            _ => false,
        })
}

fn mean_count(value: &ScalarExpr) -> Option<usize> {
    match value {
        ScalarExpr::ReduceSum {
            dtype: 1, count, ..
        } => Some(*count),
        ScalarExpr::Cast { dtype: 1, value } | ScalarExpr::Reciprocal { dtype: 1, value } => {
            mean_count(value)
        }
        ScalarExpr::Binary {
            dtype: 1,
            left,
            right,
            ..
        } => mean_count(left).or_else(|| mean_count(right)),
        _ => None,
    }
}

pub(super) fn closures(
    graph: &LoweredGraph,
    model: &Model,
    workers: usize,
) -> Result<Vec<Closure>, String> {
    if workers == 0 || workers > 64 {
        return Err("fused CPU worker count outside bounded source range".into());
    }
    let schedule = schedule_plans(graph, "CPU", true)?;
    let calls = schedule
        .calls
        .iter()
        .map(|call| (call.buffers[0], &call.buffers[1..]))
        .collect::<BTreeMap<_, _>>();
    let held = graph.outputs.values().copied().collect::<BTreeSet<_>>();
    let mut result: Vec<Closure> = Vec::new();
    let diagnostic_root = std::env::var_os("FUSED_MEAN_DIAGNOSTICS").map(PathBuf::from);
    let mut findings: Vec<Value> = Vec::new();
    macro_rules! note {
        ($mean:ident, $reason:expr, $detail:expr) => {
            if diagnostic_root.is_some() {
                if findings.len() == 32 {
                    findings.remove(0);
                }
                findings.push(json!({"output":$mean.outputs[0], "reason":$reason, "detail":$detail}));
            }
        };
    }
    for mean in
        model.graph.nodes.iter().filter(|node| {
            node.op == "ReduceMean" && node.inputs.len() <= 2 && node.outputs.len() == 1
        })
    {
        let Some(gelu) = mean
            .inputs
            .first()
            .and_then(|name| producer(model, name, "Gelu"))
        else {
            continue;
        };
        if gelu.inputs.len() != 1
            || !matches!(gelu.attributes.get("approximate"), Some(Attribute::String(v)) if v == "tanh")
            || gelu.attributes.len() != 1
        {
            note!(
                mean,
                "Gelu tanh attribute or arity",
                json!({"inputs":gelu.inputs,"attributes":format!("{:?}", gelu.attributes)})
            );
            continue;
        }
        let Some(conv) = producer(model, &gelu.inputs[0], "Conv")
            .filter(|node| node.inputs.len() == 3 && plain_pointwise(node))
        else {
            note!(
                mean,
                "pointwise Conv producer",
                json!({"gelu_input":gelu.inputs[0]})
            );
            continue;
        };
        let names = [
            &conv.inputs[0],
            &conv.inputs[1],
            &conv.inputs[2],
            &conv.outputs[0],
            &gelu.outputs[0],
            &mean.outputs[0],
        ];
        let Some([input, weight, bias, convolution, activation, output]) = names
            .map(|name| graph.names.get(name).copied())
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .and_then(|ids| <[usize; 6]>::try_from(ids).ok())
        else {
            note!(mean, "source name missing from lowered graph", names);
            continue;
        };
        if !schedule.live.contains(&output) || !held.contains(&output) {
            note!(
                mean,
                "final output not scheduled and held",
                json!({"live":schedule.live.contains(&output), "held":held.contains(&output)})
            );
            continue;
        }
        let x = &graph.plans[input].ty;
        let w = &graph.plans[weight].ty;
        let b = &graph.plans[bias].ty;
        let c = &graph.plans[convolution].ty;
        let spatial = x.shape.get(2..).and_then(|axes| {
            axes.iter()
                .try_fold(1usize, |count, &axis| count.checked_mul(axis))
        });
        if x.dtype != 1
            || x.shape.len() != 4
            || x.shape[0] != 1
            || x.shape[1] < 4
            || !x.shape[1].is_multiple_of(4)
            || !matches!(spatial, Some(width) if width > 0 && width.is_multiple_of(4))
            || c.shape.len() != 4
            || !matches!(w.dtype, 1 | 10)
            || w.shape != [c.shape[1], x.shape[1], 1, 1]
            || !matches!(graph.plans[weight].expression, ScalarExpr::Constant(_))
            || !matches!(graph.plans[bias].expression, ScalarExpr::Constant(_))
            || !matches!(b.dtype, 1 | 10)
            || b.shape != [c.shape[1]]
            || c.dtype != 1
            || c.shape != [1, w.shape[0], x.shape[2], x.shape[3]]
            || graph.plans[activation].ty != *c
            || graph.plans[output].ty.dtype != 1
            || graph.plans[output].ty.shape != [1, w.shape[0], 1, 1]
            || mean_count(&graph.plans[output].expression) != spatial
            || w.shape[0] < workers * 4
            || !w.shape[0].is_multiple_of(workers * 4)
        {
            note!(
                mean,
                "typed shape or reduction geometry",
                json!({"input_dtype":x.dtype,"input_shape":x.shape,"weight_dtype":w.dtype,"weight_shape":w.shape,"bias_dtype":b.dtype,"bias_shape":b.shape,"conv_dtype":c.dtype,"conv_shape":c.shape,"gelu_dtype":graph.plans[activation].ty.dtype,"gelu_shape":graph.plans[activation].ty.shape,"mean_dtype":graph.plans[output].ty.dtype,"mean_shape":graph.plans[output].ty.shape,"spatial":spatial,"mean_count":mean_count(&graph.plans[output].expression),"weight_constant":matches!(graph.plans[weight].expression, ScalarExpr::Constant(_)),"bias_constant":matches!(graph.plans[bias].expression, ScalarExpr::Constant(_)),"workers":workers})
            );
            continue;
        }
        let expected = BTreeSet::from([convolution, activation, output]);
        let leaves = BTreeSet::from([input, weight, bias]);
        let mut owned = BTreeSet::new();
        let mut used_leaves = BTreeSet::new();
        let mut pending = vec![output];
        let mut supported = true;
        while let Some(id) = pending.pop() {
            if leaves.contains(&id) {
                used_leaves.insert(id);
                continue;
            }
            if !owned.insert(id) {
                continue;
            }
            if graph.names.values().any(|&named| named == id) && !expected.contains(&id) {
                supported = false;
                break;
            }
            let Some(inputs) = calls.get(&id) else {
                supported = false;
                break;
            };
            pending.extend(inputs.iter().copied());
        }
        if !supported
            || !expected.is_subset(&owned)
            || used_leaves != leaves
            || owned.iter().any(|id| held.contains(id) && *id != output)
            || schedule.calls.iter().any(|call| {
                !owned.contains(&call.buffers[0])
                    && call.buffers[1..].iter().any(|id| owned.contains(id))
            })
            || result.iter().any(|c| !c.owned.is_disjoint(&owned))
        {
            note!(
                mean,
                "physical call or exclusive ownership",
                json!({"supported":supported,"expected_owned":expected.is_subset(&owned),"used_leaves":used_leaves,"expected_leaves":leaves,"owned":owned,"held_overlap":owned.iter().any(|id| held.contains(id) && *id != output),"external_consumer":schedule.calls.iter().any(|call| !owned.contains(&call.buffers[0]) && call.buffers[1..].iter().any(|id| owned.contains(id)))})
            );
            continue;
        }
        note!(mean, "selected", json!({"spatial":spatial,"owned":owned}));
        result.push(Closure {
            input,
            weight,
            bias,
            output,
            channels_in: x.shape[1],
            channels_out: w.shape[0],
            width: spatial.expect("selected bounded spatial shape"),
            workers,
            owned,
        });
    }
    if let Some(root) = diagnostic_root {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        fs::write(
            root.join("fused-mean-selection.json"),
            serde_json::to_vec_pretty(&findings).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(result)
}

#[derive(Clone, Copy)]
struct Layout {
    channels_in: usize,
    channels_out: usize,
    width: usize,
    workers: usize,
    input_stride: usize,
    weight_stride: usize,
}

impl Layout {
    fn new(c: &Closure) -> Result<Self, String> {
        let blocks = c.channels_in / 4;
        // IMAGE pad_align and bank-conflict padding on the materialized half
        // input/weight tensors. Both axes are derived from tensor dimensions.
        let align = 256usize;
        let input_axis = c.width.checked_mul(4).ok_or("fused IMAGE width overflow")?;
        let input_stride = blocks
            .checked_add(1)
            .and_then(|v| v.checked_next_multiple_of(align / gcd(input_axis, align)))
            .and_then(|v| v.checked_mul(4))
            .ok_or("fused IMAGE input stride overflow")?;
        let weight_stride = blocks
            .checked_add(1)
            .and_then(|v| v.checked_next_multiple_of(align / gcd(16, align)))
            .and_then(|v| v.checked_mul(16))
            .ok_or("fused IMAGE weight stride overflow")?;
        let result = Self {
            channels_in: c.channels_in,
            channels_out: c.channels_out,
            width: c.width,
            workers: c.workers,
            input_stride,
            weight_stride,
        };
        if result
            .checked_bytes()?
            .iter()
            .any(|&n| n == 0 || n > i32::MAX as usize / 4)
        {
            return Err("fused IMAGE physical storage exceeds bounded index".into());
        }
        Ok(result)
    }
    fn bytes(&self) -> [usize; 4] {
        self.checked_bytes().expect("validated fused IMAGE bounds")
    }
    fn checked_bytes(&self) -> Result<[usize; 4], String> {
        Ok([
            self.channels_out
                .checked_mul(4)
                .ok_or("fused IMAGE output capacity overflow")?,
            self.width
                .checked_mul(self.input_stride)
                .and_then(|n| n.checked_mul(2))
                .ok_or("fused IMAGE input capacity overflow")?,
            (self.channels_out / 4)
                .checked_mul(self.weight_stride)
                .and_then(|n| n.checked_mul(2))
                .ok_or("fused IMAGE weight capacity overflow")?,
            self.channels_out
                .checked_mul(2)
                .ok_or("fused IMAGE bias capacity overflow")?,
        ])
    }
    fn pack_weights(&self, source: &[u8], dtype: u32) -> Result<Vec<u8>, String> {
        let element_bytes = match dtype {
            1 => 4,
            10 => 2,
            _ => return Err("fused IMAGE weight dtype differs from typed ABI".into()),
        };
        let expected = self
            .channels_in
            .checked_mul(self.channels_out)
            .and_then(|n| n.checked_mul(element_bytes))
            .ok_or("fused IMAGE weight source size overflow")?;
        if source.len() != expected {
            return Err("fused IMAGE weight source differs from typed ABI".into());
        }
        let mut packed = vec![0; self.bytes()[2]];
        for output in 0..self.channels_out {
            for input in 0..self.channels_in {
                let at = (output * self.channels_in + input) * element_bytes;
                let to = (output / 4 * self.weight_stride + input * 4 + output % 4) * 2;
                let half = if dtype == 10 {
                    // The source initializer already carries the exact IMAGE
                    // half-storage payload; do not round it a second time.
                    u16::from_le_bytes(source[at..at + 2].try_into().unwrap())
                } else {
                    let bits = u32::from_le_bytes(source[at..at + 4].try_into().unwrap());
                    super::image::half_storage(bits)
                };
                packed[to..to + 2].copy_from_slice(&half.to_le_bytes());
            }
        }
        Ok(packed)
    }
    fn pack_bias(&self, source: &[u8], dtype: u32) -> Result<Vec<u8>, String> {
        match dtype {
            10 if source.len() == self.bytes()[3] => Ok(source.to_vec()),
            1 if source.len() == self.channels_out * 4 => Ok(source
                .chunks_exact(4)
                .flat_map(|chunk| {
                    super::image::half_storage(u32::from_le_bytes(chunk.try_into().unwrap()))
                        .to_le_bytes()
                })
                .collect()),
            _ => Err("fused IMAGE bias source differs from typed ABI".into()),
        }
    }
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

pub(super) struct Staged {
    output: usize,
    packed: usize,
    layout: Layout,
}

pub(super) fn insert(
    selected: &[Closure],
    graph: &LoweredGraph,
    schedule: &mut Schedule,
    initialized: &mut BTreeMap<usize, Vec<u8>>,
) -> Result<Vec<Staged>, String> {
    let mut staged = Vec::new();
    for c in selected {
        let layout = Layout::new(c)?;
        let raw_weight = match &graph.plans[c.weight].expression {
            ScalarExpr::Constant(raw) => raw,
            _ => return Err("fused IMAGE source weight changed after selection".into()),
        };
        let raw_bias = match &graph.plans[c.bias].expression {
            ScalarExpr::Constant(raw) => raw,
            _ => return Err("fused IMAGE source bias changed after selection".into()),
        };
        let mut buffer = |bytes, held| {
            let id = schedule.buffers.len();
            schedule.buffers.push(BufferSpec {
                device: "CPU".into(),
                bytes,
                held,
                supports_offset: true,
            });
            id
        };
        let packed = buffer(layout.bytes()[1], false);
        let weight = buffer(layout.bytes()[2], true);
        let bias = buffer(layout.bytes()[3], true);
        initialized.insert(
            weight,
            layout.pack_weights(raw_weight, graph.plans[c.weight].ty.dtype)?,
        );
        initialized.insert(
            bias,
            layout.pack_bias(raw_bias, graph.plans[c.bias].ty.dtype)?,
        );
        let at = schedule
            .calls
            .iter()
            .position(|call| c.owned.contains(&call.buffers[0]))
            .ok_or("fused IMAGE owned calls disappeared")?;
        schedule
            .calls
            .retain(|call| !c.owned.contains(&call.buffers[0]));
        schedule.calls.splice(
            at..at,
            [
                Call {
                    copy: false,
                    buffers: vec![packed, c.input],
                },
                Call {
                    copy: false,
                    buffers: vec![c.output, packed, weight, bias],
                },
            ],
        );
        staged.push(Staged {
            output: c.output,
            packed,
            layout,
        });
    }
    Ok(staged)
}

fn literal(value: f32) -> String {
    format!("0x{:016X}", f64::from(value).to_bits())
}

fn arithmetic(ir: &mut super::image::Ir, op: &str, a: &str, b: &str) -> String {
    ir.op(format!("{op} nsz arcp contract afn float {a}, {b}"))
}

fn exp2(ir: &mut super::image::Ir, d: &str) -> String {
    let compare = |ir: &mut super::image::Ir, predicate: &str, a: &str, b: &str| {
        ir.op(format!(
            "fcmp nsz arcp contract afn {predicate} float {a}, {b}"
        ))
    };
    let select = |ir: &mut super::image::Ir, condition: &str, yes: &str, no: &str| {
        ir.op(format!("select i1 {condition}, float {yes}, float {no}"))
    };
    let nan = compare(ir, "une", d, d);
    let not_negative_infinity = compare(ir, "une", d, "0xFFF0000000000000");
    let not_positive_infinity = compare(ir, "une", d, "0x7FF0000000000000");
    let finite = select(ir, &not_negative_infinity, d, "0.0");
    let finite = select(ir, &nan, "0.0", &finite);
    let finite = select(ir, &not_positive_infinity, &finite, "0.0");
    let negative = compare(ir, "olt", &finite, "0.0");
    let rounding = select(ir, &negative, "-0.5", "0.5");
    let rounded = arithmetic(ir, "fadd", &finite, &rounding);
    let q = ir.op(format!("fptosi float {rounded} to i32"));
    let odd = ir.op(format!("srem i32 {q}, 2"));
    let negative_q = ir.op(format!("icmp slt i32 {q}, 0"));
    let odd = ir.op(format!("icmp ne i32 {odd}, 0"));
    let negative_odd = ir.op(format!("and i1 {odd}, {negative_q}"));
    let negative_odd = ir.op(format!("zext i1 {negative_odd} to i32"));
    let q_float = ir.op(format!("sitofp i32 {q} to float"));
    let adjustment = ir.op(format!("select i1 {negative_q}, i32 1, i32 0"));
    let adjusted = ir.op(format!("add nsw i32 {q}, {adjustment}"));
    let adjustment = ir.op(format!("mul i32 {negative_odd}, -1"));
    let lo = ir.op(format!("ashr i32 {adjusted}, 1"));
    let lo = ir.op(format!("add nsw i32 {lo}, {adjustment}"));
    let negative_lo = ir.op(format!("mul i32 {lo}, -1"));
    let hi = ir.op(format!("add nsw i32 {q}, {negative_lo}"));
    let hi = ir.op(format!("add nsw i32 {hi}, 127"));
    let hi = ir.op(format!("shl i32 {hi}, 23"));
    let hi = ir.op(format!("bitcast i32 {hi} to float"));
    let lo = ir.op(format!("add nsw i32 {lo}, 127"));
    let lo = ir.op(format!("shl i32 {lo}, 23"));
    let lo = ir.op(format!("bitcast i32 {lo} to float"));
    let minus_q = arithmetic(ir, "fmul", &q_float, "-1.0");
    let s = arithmetic(ir, "fadd", &finite, &minus_q);
    let coefficients = [
        0x39210d90, 0x3aaf8a33, 0x3c1d966e, 0x3d63579c, 0x3e75fdec, 0x3f317218, 0x3f800000,
    ];
    let mut polynomial = literal(f32::from_bits(coefficients[0]));
    for coefficient in &coefficients[1..] {
        let product = arithmetic(ir, "fmul", &polynomial, &s);
        polynomial = arithmetic(ir, "fadd", &product, &literal(f32::from_bits(*coefficient)));
    }
    let scaled = arithmetic(ir, "fmul", &polynomial, &lo);
    let scaled = arithmetic(ir, "fmul", &scaled, &hi);
    let below = compare(ir, "olt", d, "-150.0");
    let below_upper = compare(ir, "olt", d, "128.0");
    let scaled = select(ir, &below_upper, &scaled, "0x7FF0000000000000");
    let scaled = select(ir, &below, "0.0", &scaled);
    select(ir, &nan, "0x7FF8000000000000", &scaled)
}

fn gelu(ir: &mut super::image::Ir, value: &str) -> String {
    let square = arithmetic(ir, "fmul", value, value);
    let cube = arithmetic(ir, "fmul", &square, value);
    let scaled_cube = arithmetic(ir, "fmul", &literal(0.044715), &cube);
    let inner = arithmetic(ir, "fadd", value, &scaled_cube);
    let exponent = arithmetic(ir, "fmul", &inner, &literal(f32::from_bits(0xc0135761)));
    let exp = exp2(ir, &exponent);
    let denominator = arithmetic(ir, "fadd", "1.0", &exp);
    arithmetic(ir, "fdiv", value, &denominator)
}

impl Layout {
    fn artifact(&self, function: &str, entry: &str) -> PipelineCall {
        let mut ir = super::image::Ir::default();
        // Constants are part of the semantic key: exp2's lower and upper
        // bounds must not collapse into the same comparison.
        ir.parameters(&[
            ("%out", 6, 0),
            ("%input", 6, 1),
            ("%weight", 6, 2),
            ("%bias", 6, 3),
            ("%acc_mean", 3, 4),
            ("%acc_conv0", 3, 5),
            ("%acc_conv1", 3, 6),
            ("%acc_conv2", 3, 7),
            ("%acc_conv3", 3, 8),
            ("%worker", 6, 9),
        ]);
        writeln!(ir.text, "define void @{function}(float* noalias align 32 %out, i16* noalias align 32 %input, i16* noalias align 32 %weight, i16* noalias align 32 %bias, i32 %worker) #0 {{").unwrap();
        // Stack slots are declared once at entry. Image::Ir deliberately treats
        // %acc loads as mutable across stores, so each reduction has its own
        // acc-prefixed slot and cannot be commoned with an earlier iteration.
        ir.line("%acc_mean = alloca float");
        for spatial_lane in 0..4 {
            ir.line(format!("%acc_conv{spatial_lane} = alloca float"));
        }
        let per_worker = self.channels_out / self.workers;
        let base = ir.op(format!("mul i32 %worker, {per_worker}"));
        let block = ir.begin_indexed_loop("block", per_worker / 4, vec![4, 0]);
        let block = ir.op(format!("mul i32 {block}, 4"));
        let base = ir.op(format!("add nsw i32 {base}, {block}"));
        let lane = ir.begin_indexed_loop("lane", 4, vec![4, 1]);
        let channel = ir.op(format!("add nsw i32 {base}, {lane}"));
        let bias = ir.load_half("%bias", &channel);
        let sum = "%acc_mean";
        ir.line(format!("store float 0.0, float* {sum}"));
        let group = ir.begin_indexed_loop("group", self.width / 4, vec![2, 0]);
        let accumulators = (0..4)
            .map(|lane| format!("%acc_conv{lane}"))
            .collect::<Vec<_>>();
        for pointer in &accumulators {
            ir.line(format!("store float 0.0, float* {pointer}"));
        }
        let reduction = ir.begin_indexed_loop("reduction", self.channels_in, vec![0, 0]);
        // For every i32 bit pattern, unsigned /4 and %4 are exactly a
        // logical shift and low-bit mask. These are admitted IMAGE operations.
        let group_weight = ir.op(format!("lshr i32 {channel}, 2"));
        let group_weight = ir.op(format!("mul i32 {group_weight}, {}", self.weight_stride));
        let weight_index = ir.op(format!("mul i32 {reduction}, 4"));
        let weight_index = ir.op(format!("add nsw i32 {group_weight}, {weight_index}"));
        let weight_lane = ir.op(format!("and i32 {channel}, 3"));
        let weight_index = ir.op(format!("add nsw i32 {weight_index}, {weight_lane}"));
        let weight = ir.load_half("%weight", &weight_index);
        let group_input = ir.op(format!("mul i32 {group}, 4"));
        for (spatial_lane, accumulator) in accumulators.iter().enumerate() {
            let pixel = ir.op(format!("add nsw i32 {group_input}, {spatial_lane}"));
            let at = ir.op(format!("mul i32 {pixel}, {}", self.input_stride));
            let at = ir.op(format!("add nsw i32 {at}, {reduction}"));
            let input = ir.load_half("%input", &at);
            let previous = ir.op(format!("load float, float* {accumulator}"));
            let product = arithmetic(&mut ir, "fmul", &input, &weight);
            let next = arithmetic(&mut ir, "fadd", &previous, &product);
            ir.line(format!("store float {next}, float* {accumulator}"));
        }
        ir.end_loop("reduction");
        let mut total = ir.op(format!("load float, float* {sum}"));
        for accumulator in &accumulators {
            let conv = ir.op(format!("load float, float* {accumulator}"));
            let conv = arithmetic(&mut ir, "fadd", &conv, &bias);
            let activated = gelu(&mut ir, &conv);
            total = arithmetic(&mut ir, "fadd", &total, &activated);
        }
        ir.line(format!("store float {total}, float* {sum}"));
        ir.end_loop("group");
        let total = ir.op(format!("load float, float* {sum}"));
        let mean = arithmetic(&mut ir, "fmul", &total, &literal(1.0 / self.width as f32));
        let pointer = ir.op(format!(
            "getelementptr inbounds float, float* %out, i32 {channel}"
        ));
        ir.store_float(&pointer, &mean);
        ir.end_loop("lane");
        ir.end_loop("block");
        ir.line("ret void");
        ir.text.push_str("}\nattributes #0 = { noinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(
            ir.text,
            "define void @{entry}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        for (slot, ty) in ["float", "i16", "i16", "i16"].iter().enumerate() {
            writeln!(ir.text, "  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %arg{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        ir.text.push_str("  %worker = load i32, i32* %scalars\n");
        writeln!(ir.text, "  call void @{function}(float* %arg0, i16* %arg1, i16* %arg2, i16* %arg3, i32 %worker)\n  ret void\n}}").unwrap();
        PipelineCall {
            source: ir.text,
            bytes: self.bytes().to_vec(),
            workers: self.workers,
            scalars: 1,
        }
    }
}

pub(super) fn scheduled(
    staged: &[Staged],
    schedule: &Schedule,
) -> Result<BTreeMap<usize, PipelineCall>, String> {
    let mut result = BTreeMap::new();
    for (index, call) in schedule.calls.iter().enumerate() {
        if let Some(c) = staged.iter().find(|c| c.packed == call.buffers[0]) {
            let artifact = super::convolution::input_artifact(
                c.layout.channels_in,
                [1, c.layout.width],
                c.layout.width * c.layout.input_stride,
                c.layout.input_stride,
                c.layout.bytes()[1],
                &format!("op_kernel_{index}"),
            )?;
            if artifact.bytes
                != call
                    .buffers
                    .iter()
                    .map(|&id| schedule.buffers[id].bytes)
                    .collect::<Vec<_>>()
            {
                return Err("fused IMAGE pack capacities differ from typed ABI".into());
            }
            result.insert(index, artifact);
            continue;
        }
        let Some(c) = staged.iter().find(|c| c.output == call.buffers[0]) else {
            continue;
        };
        if !call.buffers.contains(&c.packed) {
            return Err("fused IMAGE schedule lost owned packed input".into());
        }
        let artifact = c.layout.artifact(
            &format!("native_fused_mean_{index}"),
            &format!("op_kernel_{index}"),
        );
        if artifact.bytes
            != call
                .buffers
                .iter()
                .map(|&id| schedule.buffers[id].bytes)
                .collect::<Vec<_>>()
        {
            return Err("fused IMAGE staged capacities differ from typed ABI".into());
        }
        result.insert(index, artifact);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{Closure, Layout};
    use std::collections::BTreeSet;

    #[test]
    fn fused_ir_keeps_four_mutable_gelu_lanes_and_worker_index() {
        let closure = Closure {
            input: 0,
            weight: 1,
            bias: 2,
            output: 3,
            channels_in: 4,
            channels_out: 8,
            width: 4,
            workers: 2,
            owned: BTreeSet::new(),
        };
        let ir = Layout::new(&closure)
            .unwrap()
            .artifact("fused_slot_regression", "entry_slot_regression")
            .source;
        assert_eq!(
            ir.lines()
                .filter(|line| line.contains(" = fdiv nsz arcp contract afn float "))
                .count(),
            4,
            "each Conv lane requires its own GELU evaluation"
        );
        for lane in 0..4 {
            assert!(ir.contains(&format!("load float, float* %acc_conv{lane}")));
        }
        assert!(ir.contains("shl i32 %worker, 2"));
        for bound in ["-150.0", "128.0"] {
            assert_eq!(
                ir.lines()
                    .filter(
                        |line| line.contains(" = fcmp ") && line.ends_with(&format!(", {bound}"))
                    )
                    .count(),
                4,
                "each GELU lane needs a distinct exp2 {bound} bound"
            );
        }
    }

    #[test]
    fn dynamic_worker_and_other_scalar_have_distinct_ir_keys() {
        let mut ir = super::super::image::Ir::default();
        ir.parameters(&[("%worker", 6, 9), ("%other", 6, 10)]);
        let worker = ir.op("add i32 %worker, 7");
        let other = ir.op("add i32 %other, 7");
        assert_ne!(
            worker, other,
            "worker index must not be commoned with another scalar"
        );
    }
}
