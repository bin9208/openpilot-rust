//! Configured LLVM pointwise GELU boundary. This emits the retained original
//! tanh/xexp2 DAG and contract flags, leaving instruction contraction to the
//! explicitly selected LLVM target. It does not select ONNX graph closures.
use std::fmt::Write;

pub struct GeluTanh {
    elements: usize,
}
pub struct Artifact {
    pub source: String,
    pub bytes: [usize; 2],
    pub scalars: usize,
    pub workers: usize,
}
fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
fn literal(value: f32) -> String {
    format!("0x{:016X}", f64::from(value).to_bits())
}
#[derive(Default)]
struct Ir {
    text: String,
    next: usize,
}
impl Ir {
    fn op(&mut self, expression: impl AsRef<str>) -> String {
        let name = format!("%a{}", self.next);
        self.next += 1;
        writeln!(self.text, "  {name} = {}", expression.as_ref()).unwrap();
        name
    }
    fn arithmetic(&mut self, op: &str, a: &str, b: &str) -> String {
        self.op(format!("{op} nsz arcp contract afn float {a}, {b}"))
    }
    fn compare(&mut self, predicate: &str, a: &str, b: &str) -> String {
        self.op(format!(
            "fcmp nsz arcp contract afn {predicate} float {a}, {b}"
        ))
    }
    fn select(&mut self, condition: &str, yes: &str, no: &str) -> String {
        self.op(format!("select i1 {condition}, float {yes}, float {no}"))
    }
    fn exp2(&mut self, d: &str) -> String {
        // Source _lazy_map_numbers, rintk and ldexp2k, including negative odd q.
        let nan = self.compare("une", d, d);
        let not_negative_infinity = self.compare("une", d, "0xFFF0000000000000");
        let not_positive_infinity = self.compare("une", d, "0x7FF0000000000000");
        let finite = self.select(&not_negative_infinity, d, "0.0");
        let finite = self.select(&nan, "0.0", &finite);
        let finite = self.select(&not_positive_infinity, &finite, "0.0");
        let negative = self.compare("olt", &finite, "0.0");
        let rounding = self.select(&negative, "-0.5", "0.5");
        let rounded = self.arithmetic("fadd", &finite, &rounding);
        let q = self.op(format!("fptosi float {rounded} to i32"));
        let odd = self.op(format!("srem i32 {q}, 2"));
        let negative_q = self.op(format!("icmp slt i32 {q}, 0"));
        let odd = self.op(format!("icmp ne i32 {odd}, 0"));
        let negative_odd = self.op(format!("and i1 {odd}, {negative_q}"));
        let negative_odd = self.op(format!("zext i1 {negative_odd} to i32"));
        let q_float = self.op(format!("sitofp i32 {q} to float"));
        let adjustment = self.op(format!("select i1 {negative_q}, i32 1, i32 0"));
        let adjusted = self.op(format!("add nsw i32 {q}, {adjustment}"));
        let adjustment = self.op(format!("mul i32 {negative_odd}, -1"));
        let lo = self.op(format!("ashr i32 {adjusted}, 1"));
        let lo = self.op(format!("add nsw i32 {lo}, {adjustment}"));
        let negative_lo = self.op(format!("mul i32 {lo}, -1"));
        let hi = self.op(format!("add nsw i32 {q}, {negative_lo}"));
        let hi = self.op(format!("add nsw i32 {hi}, 127"));
        let hi = self.op(format!("shl i32 {hi}, 23"));
        let hi = self.op(format!("bitcast i32 {hi} to float"));
        let lo = self.op(format!("add nsw i32 {lo}, 127"));
        let lo = self.op(format!("shl i32 {lo}, 23"));
        let lo = self.op(format!("bitcast i32 {lo} to float"));
        let minus_q = self.arithmetic("fmul", &q_float, "-1.0");
        let s = self.arithmetic("fadd", &finite, &minus_q);
        let coefficients = [
            f32::from_bits(0x39210d90),
            f32::from_bits(0x3aaf8a33),
            f32::from_bits(0x3c1d966e),
            f32::from_bits(0x3d63579c),
            f32::from_bits(0x3e75fdec),
            f32::from_bits(0x3f317218),
            f32::from_bits(0x3f800000),
        ];
        let mut polynomial = literal(coefficients[0]);
        for coefficient in &coefficients[1..] {
            let product = self.arithmetic("fmul", &polynomial, &s);
            polynomial = self.arithmetic("fadd", &product, &literal(*coefficient));
        }
        let scaled = self.arithmetic("fmul", &polynomial, &lo);
        let scaled = self.arithmetic("fmul", &scaled, &hi);
        let below = self.compare("olt", d, "-150.0");
        let below_upper = self.compare("olt", d, "128.0");
        let scaled = self.select(&below_upper, &scaled, "0x7FF0000000000000");
        let scaled = self.select(&below, "0.0", &scaled);
        self.select(&nan, "0x7FF8000000000000", &scaled)
    }
    fn gelu(&mut self, index: &str) {
        let pointer = self.op(format!(
            "getelementptr inbounds float, float* %input, i32 {index}"
        ));
        let value = self.op(format!("load float, float* {pointer}"));
        let square = self.arithmetic("fmul", &value, &value);
        let cube = self.arithmetic("fmul", &square, &value);
        let scaled_cube = self.arithmetic("fmul", &literal(0.044715), &cube);
        let inner = self.arithmetic("fadd", &value, &scaled_cube);
        let exponent = self.arithmetic("fmul", &inner, &literal(f32::from_bits(0xc0135761)));
        let exp = self.exp2(&exponent);
        let denominator = self.arithmetic("fadd", "1.0", &exp);
        let result = self.arithmetic("fdiv", &value, &denominator);
        let pointer = self.op(format!(
            "getelementptr inbounds float, float* %out, i32 {index}"
        ));
        writeln!(self.text, "  store float {result}, float* {pointer}").unwrap();
    }
}
impl GeluTanh {
    pub fn new(shape: &[usize]) -> Result<Self, String> {
        let elements = shape
            .iter()
            .try_fold(1usize, |n, &d| n.checked_mul(d))
            .filter(|&n| n > 0 && n <= i32::MAX as usize / 4)
            .ok_or("GELU tensor exceeds bounded LLVM index/storage range")?;
        Ok(Self { elements })
    }
    pub fn artifact(&self, function: &str, entry: &str) -> Result<Artifact, String> {
        if !identifier(function) || !identifier(entry) || function == entry {
            return Err("invalid GELU LLVM identifiers".into());
        }
        let mut ir = Ir::default();
        writeln!(ir.text, "define void @{function}(float* noalias align 32 %out, float* noalias align 32 %input) #0 {{").unwrap();
        let chunks = self.elements / 4;
        if chunks > 0 {
            ir.text.push_str("  br label %loop\nloop:\n  %chunk = phi i32 [0, %0], [%next, %loop]\n  %base = mul i32 %chunk, 4\n");
            for lane in 0..4 {
                let index = ir.op(format!("add nsw i32 %base, {lane}"));
                ir.gelu(&index);
            }
            writeln!(ir.text, "  %next = add nsw i32 %chunk, 1\n  %more = icmp ult i32 %next, {chunks}\n  br i1 %more, label %loop, label %tail\ntail:").unwrap();
        }
        for index in chunks * 4..self.elements {
            ir.gelu(&index.to_string());
        }
        ir.text.push_str("  ret void\n}\nattributes #0 = { alwaysinline nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }\n");
        writeln!(ir.text,"define void @{entry}(i8** %buffers, i32* %scalars) {{\n  %o = load i8*, i8** %buffers\n  %out = bitcast i8* %o to float*\n  %s = getelementptr inbounds i8*, i8** %buffers, i32 1\n  %i = load i8*, i8** %s\n  %input = bitcast i8* %i to float*\n  call void @{function}(float* %out, float* %input)\n  ret void\n}}").unwrap();
        Ok(Artifact {
            source: ir.text,
            bytes: [self.elements * 4; 2],
            scalars: 0,
            workers: 1,
        })
    }
}

/// Replace only the code for an existing, direct f32 GELU call. The serial
/// schedule, input producer, held outputs and all allocation lifetimes stay
/// intact. Any projected half-storage barrier keeps its established renderer.
pub(super) fn scheduled(
    graph: &crate::compiler_onnx::lower::LoweredGraph,
    model: &crate::compiler_onnx::Model,
    schedule: &crate::compiler_onnx::schedule::Schedule,
) -> Result<std::collections::BTreeMap<usize, super::matrix::graph::PipelineCall>, String> {
    use crate::compiler_onnx::{
        lower::{IndexExpr, ScalarExpr},
        Attribute,
    };
    fn plain(value: &ScalarExpr, input: usize, mapping: &IndexExpr) -> bool {
        match value {
            ScalarExpr::Load { plan, index } => *plan == input && index == mapping,
            ScalarExpr::FloatLiteral { dtype: 1, .. } | ScalarExpr::Literal(_) => true,
            ScalarExpr::Cast { dtype: 1, value }
            | ScalarExpr::Reciprocal { dtype: 1, value }
            | ScalarExpr::Exp2 { dtype: 1, value } => plain(value, input, mapping),
            ScalarExpr::Binary {
                dtype: 1,
                left,
                right,
                ..
            } => plain(left, input, mapping) && plain(right, input, mapping),
            ScalarExpr::MulAcc { dtype: 1, a, b, c } => {
                [a, b, c].iter().all(|v| plain(v, input, mapping))
            }
            // Includes StorageRound: never silently remove an observable half
            // conversion, even when the logical input/output dtype is f32.
            _ => false,
        }
    }
    let mut selected = std::collections::BTreeMap::new();
    for node in &model.graph.nodes {
        if node.op != "Gelu"
            || node.inputs.len() != 1
            || node.outputs.len() != 1
            || node.attributes.len() != 1
            || !matches!(node.attributes.get("approximate"), Some(Attribute::String(value)) if value == "tanh")
        {
            continue;
        }
        let Some((&input, &output)) = graph
            .names
            .get(&node.inputs[0])
            .zip(graph.names.get(&node.outputs[0]))
        else {
            continue;
        };
        let source = &graph.plans[input];
        let target = &graph.plans[output];
        if source.ty.dtype != 1 || target.ty != source.ty {
            continue;
        }
        // lower_activation uses the full-shape identity, not a broadcast map.
        let mapping = IndexExpr::Index;
        if !plain(&target.expression, input, &mapping) {
            continue;
        }
        let Some((index, _)) = schedule
            .calls
            .iter()
            .enumerate()
            .find(|(_, call)| call.buffers == [output, input])
        else {
            continue;
        };
        let artifact = GeluTanh::new(&source.ty.shape)?.artifact(
            &format!("native_gelu_{index}"),
            &format!("op_kernel_{index}"),
        )?;
        selected.insert(
            index,
            super::matrix::graph::PipelineCall {
                source: artifact.source,
                bytes: artifact.bytes.to_vec(),
                workers: artifact.workers,
                scalars: artifact.scalars,
            },
        );
    }
    Ok(selected)
}
