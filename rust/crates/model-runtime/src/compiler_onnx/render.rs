//! Initial OpenCL renderer for typed linear float32 elementwise expressions.
//! This is a bounded compiler primitive, not a complete graph backend. Image
//! packing, reductions and fused scheduling must be implemented separately.
use super::{
    lower::{IndexExpr, LoweredGraph, ScalarExpr},
    scalar::Binary,
};
#[derive(Clone, Debug, PartialEq)]
pub struct Kernel {
    pub function: String,
    pub source: String,
    pub inputs: Vec<usize>,
    pub global: [u32; 3],
}
struct Renderer<'a> {
    graph: &'a LoweredGraph,
    output: usize,
    count: usize,
    inputs: Vec<usize>,
}
fn elements(shape: &[usize]) -> Result<usize, String> {
    shape.iter().try_fold(1usize, |n, v| {
        n.checked_mul(*v)
            .ok_or("renderer tensor size overflow".into())
    })
}
fn float(value: f64) -> Result<String, String> {
    let value = value as f32;
    if !value.is_finite() {
        return Err("nonfinite elementwise literal rendering not implemented".into());
    }
    Ok(format!("{value:?}f"))
}
impl Renderer<'_> {
    fn index(&self, value: &IndexExpr) -> Result<(String, i128, i128), String> {
        let result = match value {
            IndexExpr::Index => ("i".into(), 0, (self.count - 1) as i128),
            IndexExpr::Reduction => {
                return Err("elementwise reduction index rendering not implemented".into())
            }
            IndexExpr::Constant(v) => {
                let text = if *v == i64::MIN as i128 {
                    "(-9223372036854775807L-1L)".into()
                } else {
                    format!("{v}L")
                };
                (text, *v, *v)
            }
            IndexExpr::Add(a, b) => {
                let (a, lo, hi) = self.index(a)?;
                let (b, bl, bh) = self.index(b)?;
                (
                    format!("((long)({a})+({b}))"),
                    lo.checked_add(bl).ok_or("index addition overflow")?,
                    hi.checked_add(bh).ok_or("index addition overflow")?,
                )
            }
            IndexExpr::Mul(a, b) => {
                let (a, lo, hi) = self.index(a)?;
                let (factor, _, _) = self.index(&IndexExpr::Constant(*b))?;
                let x = lo.checked_mul(*b).ok_or("index product overflow")?;
                let y = hi.checked_mul(*b).ok_or("index product overflow")?;
                (format!("((long)({a})*({factor}))"), x.min(y), x.max(y))
            }
            IndexExpr::Div(a, b) | IndexExpr::Mod(a, b) => {
                if *b == 0 || *b > i64::MAX as usize {
                    return Err("invalid renderer index divisor".into());
                }
                let (a, lo, hi) = self.index(a)?;
                let d = *b as i128;
                if matches!(value, IndexExpr::Div(..)) {
                    (format!("({a}/{b})"), lo / d, hi / d)
                } else {
                    (
                        format!("({a}%{b})"),
                        if lo < 0 { -(d - 1) } else { 0 },
                        if hi > 0 { hi.min(d - 1) } else { 0 },
                    )
                }
            }
        };
        if result.1 < i64::MIN as i128 || result.2 > i64::MAX as i128 {
            return Err("renderer index exceeds signed64".into());
        }
        Ok(result)
    }

    fn expression(&mut self, value: &ScalarExpr) -> Result<String, String> {
        match value {
            ScalarExpr::Load { plan, index } => {
                let input = self
                    .graph
                    .plans
                    .get(*plan)
                    .filter(|_| *plan < self.output)
                    .ok_or("kernel load must precede output")?;
                let (mapping, low, high) = self.index(index)?;
                if input.ty.dtype != 1 || low < 0 || high >= elements(&input.ty.shape)? as i128 {
                    return Err("linear float32 load storage mismatch".into());
                }
                let slot = if let Some(slot) = self.inputs.iter().position(|v| v == plan) {
                    slot
                } else {
                    self.inputs.push(*plan);
                    self.inputs.len() - 1
                };
                Ok(format!(
                    "{}[{mapping}]",
                    if slot == 0 {
                        "inp".into()
                    } else {
                        format!("inp{slot}")
                    }
                ))
            }
            ScalarExpr::FloatLiteral { dtype: 1, value } => float(*value),
            ScalarExpr::Literal(value) if value.dtype == 1 => float(value.float()?),
            ScalarExpr::Cast { dtype: 1, value } => self.expression(value),
            ScalarExpr::Binary {
                kind,
                dtype: 1,
                left,
                right,
            } => {
                let a = self.expression(left)?;
                let b = self.expression(right)?;
                Ok(match kind {
                    Binary::Add => format!("({a}+{b})"),
                    Binary::Sub => format!("({a}-{b})"),
                    Binary::Mul => format!("({a}*{b})"),
                    Binary::Maximum => format!("({b}>{a}?{b}:{a})"),
                    Binary::Minimum => format!("({b}<{a}?{b}:{a})"),
                    Binary::DivTrunc => {
                        return Err("floating truncating divide in scalar IR".into())
                    }
                })
            }
            _ => Err("elementwise OpenCL rendering not implemented for this scalar IR".into()),
        }
    }
}
/// Render an exact, unpadded work-item domain. Initial support is deliberately
/// explicit; unimplemented expressions never become placeholder kernels.
pub fn render_elementwise(
    graph: &LoweredGraph,
    output: usize,
    function: &str,
) -> Result<Kernel, String> {
    if function.is_empty()
        || !function
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
    {
        return Err("invalid OpenCL kernel identifier".into());
    }
    let plan = graph
        .plans
        .get(output)
        .ok_or("kernel output plan index out of range")?;
    if plan.ty.dtype != 1 {
        return Err("elementwise OpenCL output requires float32".into());
    }
    let count = elements(&plan.ty.shape)?;
    if count == 0 || count > i32::MAX as usize {
        return Err("elementwise OpenCL requires a nonempty signed32 index domain".into());
    }
    let mut renderer = Renderer {
        graph,
        output,
        count,
        inputs: Vec::new(),
    };
    let mut expression = renderer.expression(&plan.expression)?;
    // Only the outermost arithmetic parentheses are redundant in the store.
    if expression.starts_with('(') && expression.ends_with(')') {
        expression = expression[1..expression.len() - 1].to_string();
    }
    let mut arguments = vec!["__global float *out".to_string()];
    for slot in 0..renderer.inputs.len() {
        arguments.push(format!(
            "__global const float *{}",
            if slot == 0 {
                "inp".into()
            } else {
                format!("inp{slot}")
            }
        ));
    }
    let source = format!(
        "__kernel void {function}({}) {{ int i=get_global_id(0); out[i]={expression}; }}\n",
        arguments.join(", ")
    );
    Ok(Kernel {
        function: function.into(),
        source,
        inputs: renderer.inputs,
        global: [count as u32, 1, 1],
    })
}
