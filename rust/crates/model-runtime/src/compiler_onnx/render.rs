//! Bounded OpenCL preprocessing renderer and explicit CPU reduction renderer.
//! This is a bounded compiler primitive, not a complete graph backend. Image
//! packing, GPU reductions and fused scheduling remain separate pending work.
mod reduction;
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
    cpu_emulated_half: bool,
    preserve_half_storage: bool,
    reduction: Option<(String, usize)>,
    statements: Vec<String>,
}
fn elements(shape: &[usize]) -> Result<usize, String> {
    shape.iter().try_fold(1usize, |n, v| {
        n.checked_mul(*v)
            .ok_or("renderer tensor size overflow".into())
    })
}
pub fn storage_type(dtype: u32) -> Result<&'static str, String> {
    match dtype {
        1 => Ok("float"),
        2 => Ok("uchar"),
        10 => Ok("half"),
        _ => Err("elementwise storage dtype not implemented".into()),
    }
}
fn rounded(dtype: u32, expression: String) -> Result<String, String> {
    match dtype {
        1 => Ok(expression),
        10 => Ok(format!("((half)({expression}))")),
        _ => Err("elementwise arithmetic dtype not implemented".into()),
    }
}
fn float(value: f64) -> Result<String, String> {
    let value = value as f32;
    if !value.is_finite() {
        return Err("nonfinite elementwise literal rendering not implemented".into());
    }
    Ok(format!("{value:?}f"))
}
impl Renderer<'_> {
    fn storage(&self, dtype: u32) -> Result<&'static str, String> {
        if self.cpu_emulated_half && dtype == 10 {
            Ok("ushort")
        } else {
            storage_type(dtype)
        }
    }
    fn round(&self, dtype: u32, expression: String) -> Result<String, String> {
        if self.cpu_emulated_half && dtype == 10 {
            Ok(expression)
        } else {
            rounded(dtype, expression)
        }
    }
    fn index(&self, value: &IndexExpr) -> Result<(String, i128, i128), String> {
        let result = match value {
            IndexExpr::Index => ("i".into(), 0, (self.count - 1) as i128),
            IndexExpr::Reduction => {
                let (name, count) = self
                    .reduction
                    .as_ref()
                    .ok_or("reduction index outside a reduction")?;
                (name.clone(), 0, (*count - 1) as i128)
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
                storage_type(input.ty.dtype)?;
                if low < 0 || high >= elements(&input.ty.shape)? as i128 {
                    return Err("linear typed load storage mismatch".into());
                }
                let slot = if let Some(slot) = self.inputs.iter().position(|v| v == plan) {
                    slot
                } else {
                    self.inputs.push(*plan);
                    self.inputs.len() - 1
                };
                let value = format!(
                    "{}[{mapping}]",
                    if slot == 0 {
                        "inp".into()
                    } else {
                        format!("inp{slot}")
                    }
                );
                Ok(
                    if self.cpu_emulated_half && !self.preserve_half_storage && input.ty.dtype == 10
                    {
                        format!("native_half_load({value})")
                    } else {
                        value
                    },
                )
            }
            ScalarExpr::StorageRound { dtype: 10, value } => {
                let value = self.expression(value)?;
                if self.cpu_emulated_half {
                    Ok(format!(
                        "native_half_load(native_half_store((float)({value})))"
                    ))
                } else {
                    rounded(10, value)
                }
            }
            ScalarExpr::ReduceSum {
                dtype: 1,
                count,
                value,
            } => self.reduce_sum(*count, value),
            ScalarExpr::PaddedLoad { plan, coordinates } => self.padded_load(*plan, coordinates),
            ScalarExpr::FloatLiteral { dtype, value } if matches!(dtype, 1 | 10) => {
                self.round(*dtype, float(*value)?)
            }
            ScalarExpr::Literal(value) if matches!(value.dtype, 1 | 10) => {
                self.round(value.dtype, float(value.float()?)?)
            }
            ScalarExpr::Cast { dtype, value } if matches!(dtype, 1 | 10) => {
                let value = self.expression(value)?;
                if self.preserve_half_storage && *dtype == 10 {
                    Ok(value)
                } else if self.cpu_emulated_half && *dtype == 10 {
                    Ok(format!("native_half_clamp((float)({value}))"))
                } else {
                    Ok(format!("(({})({value}))", storage_type(*dtype)?))
                }
            }
            ScalarExpr::Reciprocal { dtype, value } if matches!(dtype, 1 | 10) => {
                let value = self.expression(value)?;
                // Original half reciprocal rounds before the following multiply.
                self.round(*dtype, format!("(1.0f/(float)({value}))"))
            }
            ScalarExpr::Binary {
                kind,
                dtype,
                left,
                right,
            } if matches!(dtype, 1 | 10) => {
                if *dtype == 10 && matches!(kind, Binary::Minimum | Binary::Maximum) {
                    return Err("half minimum/maximum rendering not implemented".into());
                }
                if self.cpu_emulated_half && matches!(dtype, 1 | 10) && *kind == Binary::Mul {
                    if let ScalarExpr::Reciprocal {
                        dtype: reciprocal_dtype,
                        value,
                    } = right.as_ref()
                    {
                        if reciprocal_dtype != dtype {
                            return Err("division reciprocal dtype mismatch".into());
                        }
                        // Original late FDIV rewrite, after half ALU emulation.
                        let a = self.expression(left)?;
                        if let ScalarExpr::Literal(constant) = value.as_ref() {
                            let number = constant.float()? as f32;
                            let reciprocal = 1.0f32 / number;
                            if reciprocal.is_finite() {
                                return Ok(format!("({a}*{})", float(reciprocal as f64)?));
                            }
                        }
                        let b = self.expression(value)?;
                        return Ok(format!("((float)({a})/(float)({b}))"));
                    }
                }
                let mut a = self.expression(left)?;
                let mut b = self.expression(right)?;
                if *dtype == 10 && !self.cpu_emulated_half {
                    // Half ALU operands and result each have an observable cast.
                    a = format!("(float)((half)({a}))");
                    b = format!("(float)((half)({b}))");
                }
                self.round(
                    *dtype,
                    match kind {
                        Binary::Add => format!("({a}+{b})"),
                        Binary::Sub => format!("({a}-{b})"),
                        Binary::Mul => format!("({a}*{b})"),
                        Binary::Maximum => format!("({b}>{a}?{b}:{a})"),
                        Binary::Minimum => format!("({b}<{a}?{b}:{a})"),
                        Binary::DivTrunc => {
                            return Err("floating truncating divide in scalar IR".into())
                        }
                    },
                )
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
    render(graph, output, function, false)
}
/// Match the original CPU:LLVM half decomposition: float ALU, denormal-zero
/// loads/stores, and explicit half casts clamp range without intermediate rounding.
pub fn render_cpu_elementwise(
    graph: &LoweredGraph,
    output: usize,
    function: &str,
) -> Result<Kernel, String> {
    render(graph, output, function, true)
}
fn render(
    graph: &LoweredGraph,
    output: usize,
    function: &str,
    cpu_emulated_half: bool,
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
    let output_type = if cpu_emulated_half && plan.ty.dtype == 10 {
        "ushort"
    } else {
        storage_type(plan.ty.dtype)?
    };
    if plan.ty.dtype == 2 && !matches!(plan.expression, ScalarExpr::Load { .. }) {
        return Err("uint8 output only supports static storage views".into());
    }
    let count = elements(&plan.ty.shape)?;
    if count == 0 || count > i32::MAX as usize {
        return Err("elementwise OpenCL requires a nonempty signed32 index domain".into());
    }
    fn raw_half_view(value: &ScalarExpr, graph: &LoweredGraph) -> bool {
        match value {
            ScalarExpr::Load { plan, .. } => {
                graph.plans.get(*plan).is_some_and(|p| p.ty.dtype == 10)
            }
            ScalarExpr::Cast { dtype: 10, value } => raw_half_view(value, graph),
            _ => false,
        }
    }
    let preserve_half_storage =
        cpu_emulated_half && plan.ty.dtype == 10 && raw_half_view(&plan.expression, graph);
    let mut renderer = Renderer {
        graph,
        output,
        count,
        inputs: Vec::new(),
        cpu_emulated_half,
        preserve_half_storage,
        reduction: None,
        statements: Vec::new(),
    };
    let mut expression = renderer.expression(&plan.expression)?;
    // Only the outermost arithmetic parentheses are redundant in the store.
    if plan.ty.dtype == 1
        && matches!(plan.expression, ScalarExpr::Binary { dtype: 1, .. })
        && expression.starts_with('(')
        && expression.ends_with(')')
    {
        expression = expression[1..expression.len() - 1].to_string();
    }
    if cpu_emulated_half && plan.ty.dtype == 10 && !preserve_half_storage {
        expression = format!("native_half_store({expression})");
    }
    let mut arguments = vec![format!("__global {output_type} *out")];
    for slot in 0..renderer.inputs.len() {
        arguments.push(format!(
            "__global const {} *{}",
            renderer.storage(graph.plans[renderer.inputs[slot]].ty.dtype)?,
            if slot == 0 {
                "inp".into()
            } else {
                format!("inp{slot}")
            }
        ));
    }
    let needs_half = plan.ty.dtype == 10
        || renderer
            .inputs
            .iter()
            .any(|&id| graph.plans[id].ty.dtype == 10)
        || expression.contains("half")
        || renderer
            .statements
            .iter()
            .any(|statement| statement.contains("half"));
    let header = if cpu_emulated_half && needs_half {
        CPU_HALF
    } else if needs_half {
        "#pragma OPENCL EXTENSION cl_khr_fp16 : enable\n"
    } else {
        ""
    };
    let source = format!(
        "{header}__kernel void {function}({}) {{ int i=get_global_id(0); {}out[i]={expression}; }}\n",
        arguments.join(", "), renderer.statements.join(" ")
    );
    Ok(Kernel {
        function: function.into(),
        source,
        inputs: renderer.inputs,
        global: [count as u32, 1, 1],
    })
}

// Native transcription of original f2f/f2f_clamp for half storage. Integer bit
// operations preserve signed zero and NaN payloads; no host half ABI is used.
const CPU_HALF: &str = r#"static inline float native_half_load(unsigned short h) {
  union { unsigned int u; float f; } v;
  unsigned int mag=h&32767u, exp=mag>>10;
  v.u=((unsigned int)(h&32768u)<<16) | (exp==0 ? 0u : exp==31 ? 2139095040u|((mag&1023u)<<13) : (mag<<13)+939524096u);
  return v.f;
}
static inline float native_half_clamp(float x) {
  union { unsigned int u; float f; } v; v.f=x;
  unsigned int mag=v.u&2147483647u;
  if(mag>1199562752u && mag<=2139095040u) v.u=(v.u&2147483648u)|2139095040u;
  return v.f;
}
static inline unsigned short native_half_store(float x) {
  union { unsigned int u; float f; } v; v.f=native_half_clamp(x);
  unsigned int mag=v.u&2147483647u, sign=(v.u>>16)&32768u, exp=mag>>23;
  unsigned int r=(mag>>13)+(((mag>>12)&1u)&(((mag&4095u)!=0u)|((mag>>13)&1u)));
  unsigned int result=exp==255u ? 31744u|((mag>>13)&1023u) : exp<113u ? 0u : r-114688u;
  return (unsigned short)(sign|result);
}
"#;
