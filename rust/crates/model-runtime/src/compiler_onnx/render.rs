//! Bounded OpenCL preprocessing renderer and explicit CPU reduction renderer.
//! This is a bounded compiler primitive, not a complete graph backend. Image
//! packing, GPU reductions and fused scheduling remain separate pending work.
mod math;
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
    cpu_math: bool,
    shared_reciprocals: Vec<ScalarExpr>,
    branch_bounds: Vec<(IndexExpr, i128, i128)>,
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
    fn runtime_divisor_binding(&self, value: &ScalarExpr) -> Result<bool, String> {
        let ScalarExpr::Load { plan, .. } = value else {
            return Ok(false);
        };
        let mut id = *plan;
        let mut path = Vec::new();
        loop {
            let source = self
                .graph
                .plans
                .get(id)
                .ok_or("reciprocal view plan out of range")?;
            path.push(id);
            let parent = match &source.expression {
                ScalarExpr::Load { plan, .. } => Some(*plan),
                ScalarExpr::Cast { value, .. } => match value.as_ref() {
                    ScalarExpr::Load { plan, .. } => Some(*plan),
                    _ => None,
                },
                ScalarExpr::Input(_) => {
                    // Include every view's cardinality: Expand after a smaller
                    // Slice must not conceal reciprocal broadcast/reuse.
                    for id in path {
                        if elements(&self.graph.plans[id].ty.shape)? < self.count {
                            return Err("CPU constant-vector reciprocal dynamic broadcast context is not implemented".into());
                        }
                    }
                    return Ok(true);
                }
                _ => None,
            };
            let Some(parent) = parent else {
                return Ok(false);
            };
            if !self.graph.cpu_divisor_views.contains(&id) {
                return Ok(false);
            }
            if parent >= id {
                return Err("constant-vector reciprocal graph is not topological".into());
            }
            if elements(&self.graph.plans[parent].ty.shape)? < elements(&source.ty.shape)? {
                return Err("CPU constant-vector reciprocal broadcast-bearing view context is not implemented".into());
            }
            id = parent;
        }
    }
    fn normal_divisor_initializer(&self, value: &ScalarExpr) -> Result<bool, String> {
        let ScalarExpr::Load { plan, .. } = value else {
            return Ok(false);
        };
        let source = self
            .graph
            .plans
            .get(*plan)
            .ok_or("reciprocal initializer plan out of range")?;
        let ScalarExpr::Constant(bytes) = &source.expression else {
            return Ok(false);
        };
        if !matches!(source.ty.dtype, 1 | 10) || elements(&source.ty.shape)? > 4096 {
            return Ok(false);
        }
        let width = super::parse::width(source.ty.dtype)?;
        Ok(!bytes.is_empty()
            && bytes.len() % width == 0
            && bytes.chunks_exact(width).all(|raw| {
                super::scalar::Scalar::read(source.ty.dtype, raw).is_ok_and(|scalar| {
                    scalar.float().is_ok_and(|v| (v as f32).is_normal())
                        && (source.ty.dtype != 10 || (1..31).contains(&((scalar.bits >> 10) & 31)))
                })
            }))
    }
    // Original constant propagation precedes late dynamic FDIV recognition.
    // Fold only bounded normal finite initializer vectors; physical-half
    // specials and shared reciprocal contexts need separate original proof.
    fn constant_vector_reciprocal(&self, value: &ScalarExpr) -> Result<Option<String>, String> {
        let ScalarExpr::Load { plan, index } = value else {
            return Ok(None);
        };
        let source = self
            .graph
            .plans
            .get(*plan)
            .filter(|_| *plan < self.output)
            .ok_or("kernel load must precede output")?;
        let ScalarExpr::Constant(bytes) = &source.expression else {
            // Computed divisors need source simplification/materialization
            // semantics, not a growing list of neutral-expression spellings.
            // Admit bindings/views and the independently captured single C+w
            // form only; all deeper arithmetic remains explicitly unsupported.
            if self.runtime_divisor_binding(value)? {
                return Ok(None);
            }
            if let ScalarExpr::Binary {
                kind: Binary::Add,
                left,
                right,
                ..
            } = &source.expression
            {
                for (constant, binding) in [(left, right), (right, left)] {
                    if self.normal_divisor_initializer(constant)?
                        && self.runtime_divisor_binding(binding)?
                    {
                        return Ok(None);
                    }
                }
            }
            return Err(
                "CPU constant-vector reciprocal computed context is not implemented".into(),
            );
        };
        if source.ty.shape.is_empty() || !matches!(source.ty.dtype, 1 | 10) {
            return Ok(None);
        }
        let count = elements(&source.ty.shape)?;
        if count == 0 || count > 4096 {
            return Err("CPU constant-vector reciprocal size is not implemented".into());
        }
        let width = super::parse::width(source.ty.dtype)?;
        if bytes.len() != count * width {
            return Err("CPU constant-vector reciprocal storage mismatch".into());
        }
        let mut values = Vec::with_capacity(count);
        for raw in bytes.chunks_exact(width) {
            let scalar = super::scalar::Scalar::read(source.ty.dtype, raw)?;
            let value = scalar.float()? as f32;
            let reciprocal = 1.0f32 / value;
            if !value.is_finite()
                || !reciprocal.is_finite()
                || source.ty.dtype == 10 && !(1..31).contains(&((scalar.bits >> 10) & 31))
            {
                return Err(
                    "CPU constant-vector reciprocal special values are not implemented".into(),
                );
            }
            values.push(float(reciprocal as f64)?);
        }
        let (mapping, low, high) = self.index(index)?;
        if low < 0 || high >= count as i128 {
            return Err("CPU constant-vector reciprocal index is out of bounds".into());
        }
        Ok(Some(format!(
            "((const float[]){{{}}})[{mapping}]",
            values.join(",")
        )))
    }
    fn round(&self, dtype: u32, expression: String) -> Result<String, String> {
        if self.cpu_emulated_half && dtype == 10 {
            Ok(expression)
        } else {
            rounded(dtype, expression)
        }
    }
    fn index(&self, value: &IndexExpr) -> Result<(String, i128, i128), String> {
        let mut result = match value {
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
        for (expression, low, high) in &self.branch_bounds {
            if expression == value {
                result.1 = result.1.max(*low);
                result.2 = result.2.min(*high);
            }
        }
        if result.1 > result.2 {
            return Err("conditional index branch has an empty proven domain".into());
        }
        if result.1 < i64::MIN as i128 || result.2 > i64::MAX as i128 {
            return Err("renderer index exceeds signed64".into());
        }
        Ok(result)
    }

    fn predicate(&mut self, condition: &ScalarExpr) -> Result<String, String> {
        match condition {
            ScalarExpr::IndexLess { index, bound } => {
                if *bound < i64::MIN as i128 || *bound > i64::MAX as i128 {
                    return Err("conditional index bound exceeds signed64".into());
                }
                let (index, _, _) = self.index(index)?;
                let (bound, _, _) = self.index(&IndexExpr::Constant(*bound))?;
                Ok(format!("({index}<{bound})"))
            }
            ScalarExpr::GreaterZero(value) => Ok(format!("({}>0.0f)", self.expression(value)?)),
            ScalarExpr::BoolNot(value) => Ok(format!("(!{})", self.predicate(value)?)),
            _ => Err("CPU conditional predicate is not implemented".into()),
        }
    }
    fn conditional_branch(
        &mut self,
        condition: &ScalarExpr,
        expression: &ScalarExpr,
        yes: bool,
    ) -> Result<String, String> {
        let saved = self.branch_bounds.len();
        let before_statements = self.statements.len();
        if let ScalarExpr::IndexLess { index, bound } = condition {
            let (_, low, high) = self.index(index)?;
            let (low, high) = if yes {
                (
                    low,
                    high.min(bound.checked_sub(1).ok_or("conditional bound underflow")?),
                )
            } else {
                (low.max(*bound), high)
            };
            self.branch_bounds.push((index.clone(), low, high));
        }
        let result = self.expression(expression);
        self.branch_bounds.truncate(saved);
        if self.statements.len() != before_statements {
            return Err("CPU conditional branch cannot hoist reduction statements".into());
        }
        result
    }
    fn expression(&mut self, value: &ScalarExpr) -> Result<String, String> {
        match value {
            ScalarExpr::Select { condition, yes, no }
                if self.cpu_emulated_half
                    && matches!(self.graph.plans[self.output].ty.dtype, 1 | 10) =>
            {
                let predicate = self.predicate(condition)?;
                let yes = self.conditional_branch(condition, yes, true)?;
                let no = self.conditional_branch(condition, no, false)?;
                // C's conditional evaluates only the selected branch; rejected
                // maps never become unchecked or speculative pointer accesses.
                Ok(format!("({predicate}?{yes}:{no})"))
            }
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
            ScalarExpr::Exp2 { dtype: 1, value } if self.cpu_emulated_half => {
                self.cpu_math = true;
                Ok(format!("native_exp2({})", self.expression(value)?))
            }
            ScalarExpr::Sign { dtype: 1, value } if self.cpu_emulated_half => {
                self.cpu_math = true;
                Ok(format!("native_sign({})", self.expression(value)?))
            }
            ScalarExpr::MulAcc { dtype: 1, a, b, c } if self.cpu_emulated_half => Ok(format!(
                "__builtin_fmaf((float)({}),(float)({}),(float)({}))",
                self.expression(a)?,
                self.expression(b)?,
                self.expression(c)?
            )),
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
                if self.cpu_emulated_half {
                    if self.constant_vector_reciprocal(value)?.is_some() {
                        return Err("CPU shared or standalone constant-vector reciprocal is not implemented".into());
                    }
                    let infinite = match value.as_ref() {
                        ScalarExpr::Literal(v) => v.float()?.is_infinite(),
                        ScalarExpr::Load {
                            plan,
                            index: IndexExpr::Constant(0),
                        } if self.graph.plans[*plan].ty.shape.is_empty() => {
                            let source = &self.graph.plans[*plan];
                            match &source.expression {
                                ScalarExpr::Constant(bytes) => {
                                    super::scalar::Scalar::read(source.ty.dtype, bytes)?
                                        .float()?
                                        .is_infinite()
                                }
                                _ => false,
                            }
                        }
                        _ => false,
                    };
                    if infinite {
                        return Err("CPU shared or standalone infinite scalar reciprocal is not implemented".into());
                    }
                }
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
                    let division = match (left.as_ref(), right.as_ref()) {
                        (numerator, ScalarExpr::Reciprocal { dtype, value }) => {
                            Some((numerator, dtype, value.as_ref()))
                        }
                        (ScalarExpr::Reciprocal { dtype, value }, numerator) => {
                            Some((numerator, dtype, value.as_ref()))
                        }
                        _ => None,
                    };
                    if let Some((numerator, reciprocal_dtype, value)) =
                        division.filter(|(_, _, value)| {
                            !self.shared_reciprocals.iter().any(|v| v == *value)
                        })
                    {
                        if reciprocal_dtype != dtype {
                            return Err("division reciprocal dtype mismatch".into());
                        }
                        // Original late FDIV rewrite accepts reciprocal on either
                        // side after commutative canonicalization.
                        let a = self.expression(numerator)?;
                        if let Some(reciprocal) = self.constant_vector_reciprocal(value)? {
                            // Keep the constant buffer in the scheduler's stable ABI order.
                            self.expression(value)?;
                            return Ok(format!("({a}*{reciprocal})"));
                        }
                        // Original constant infinity division simplifies to positive
                        // zero even for NaN/Inf/-zero numerators. Other literals
                        // retain the existing finite reciprocal fold.
                        if let ScalarExpr::Literal(constant) = value {
                            let number = constant.float()? as f32;
                            if number.is_infinite() {
                                return Ok("0.0f".into());
                            }
                            let reciprocal = 1.0f32 / number;
                            if reciprocal.is_finite() {
                                return Ok(format!("({a}*{})", float(reciprocal as f64)?));
                            }
                        }
                        let scalar = match value {
                            ScalarExpr::Load {
                                plan,
                                index: IndexExpr::Constant(0),
                            } if self.graph.plans[*plan].ty.shape.is_empty() => {
                                let source = &self.graph.plans[*plan];
                                if let ScalarExpr::Constant(bytes) = &source.expression {
                                    let constant =
                                        super::scalar::Scalar::read(source.ty.dtype, bytes)?;
                                    // Keep physical half special-value/load semantics out of
                                    // this normal finite scalar constant-folding rule.
                                    let normal = source.ty.dtype != 10
                                        || (1..31).contains(&((constant.bits >> 10) & 31))
                                        || constant.bits & 32767 == 31744;
                                    normal
                                        .then(|| constant.float())
                                        .transpose()?
                                        .map(|v| v as f32)
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        let b = self.expression(value)?;
                        if let Some(number) = scalar {
                            if number.is_infinite() {
                                return Ok("0.0f".into());
                            }
                            let reciprocal = 1.0f32 / number;
                            if number.is_finite() && reciprocal.is_finite() {
                                return Ok(format!("({a}*{})", float(reciprocal as f64)?));
                            }
                        }
                        return Ok(format!("((float)({a})/(float)({b}))"));
                    }
                }
                if self.cpu_emulated_half && *dtype == 1 && *kind == Binary::Sub {
                    if let ScalarExpr::Binary {
                        kind: Binary::Mul,
                        dtype: 1,
                        left: a,
                        right: b,
                    } = right.as_ref()
                    {
                        let has_unshared_reciprocal = [a.as_ref(), b.as_ref()].iter().any(|term| matches!(term, ScalarExpr::Reciprocal { value, .. } if !self.shared_reciprocals.iter().any(|v| v == value.as_ref())));
                        if !has_unshared_reciprocal {
                            return Ok(format!(
                                "__builtin_fmaf(-(float)({}),(float)({}),(float)({}))",
                                self.expression(a)?,
                                self.expression(b)?,
                                self.expression(left)?
                            ));
                        }
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
    fn reciprocals(value: &ScalarExpr, found: &mut Vec<(ScalarExpr, usize)>) {
        match value {
            ScalarExpr::Reciprocal { value, .. } => {
                if let Some((_, count)) = found.iter_mut().find(|(v, _)| v == value.as_ref()) {
                    *count += 1;
                } else {
                    found.push((value.as_ref().clone(), 1));
                }
                reciprocals(value, found);
            }
            ScalarExpr::Cast { value, .. }
            | ScalarExpr::StorageRound { value, .. }
            | ScalarExpr::Exp2 { value, .. }
            | ScalarExpr::Sign { value, .. }
            | ScalarExpr::Sqrt { value, .. }
            | ScalarExpr::Trunc { value, .. }
            | ScalarExpr::ReduceSum { value, .. }
            | ScalarExpr::ReduceMax { value, .. }
            | ScalarExpr::GreaterZero(value)
            | ScalarExpr::BoolNot(value) => reciprocals(value, found),
            ScalarExpr::Binary { left, right, .. } => {
                reciprocals(left, found);
                reciprocals(right, found);
            }
            ScalarExpr::MulAcc { a, b, c, .. } => {
                reciprocals(a, found);
                reciprocals(b, found);
                reciprocals(c, found);
            }
            ScalarExpr::Select { condition, yes, no } => {
                reciprocals(condition, found);
                reciprocals(yes, found);
                reciprocals(no, found);
            }
            _ => (),
        }
    }
    let mut occurrences = Vec::new();
    reciprocals(&plan.expression, &mut occurrences);
    let shared_reciprocals = occurrences
        .into_iter()
        .filter_map(|(value, count)| (count > 1).then_some(value))
        .collect();
    let mut renderer = Renderer {
        graph,
        output,
        count,
        inputs: Vec::new(),
        cpu_emulated_half,
        preserve_half_storage,
        reduction: None,
        statements: Vec::new(),
        cpu_math: false,
        shared_reciprocals,
        branch_bounds: Vec::new(),
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
    let header = if renderer.cpu_math {
        format!("{header}{}", math::CPU_MATH)
    } else {
        header.to_string()
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
