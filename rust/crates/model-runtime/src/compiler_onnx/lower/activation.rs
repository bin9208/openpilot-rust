use super::*;
fn lit(dtype: u32, value: f64) -> Result<ScalarExpr, String> {
    Scalar::floating_literal(dtype, value)?;
    Ok(ScalarExpr::FloatLiteral { dtype, value })
}
fn cast(dtype: u32, value: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Cast {
        dtype,
        value: Box::new(value),
    }
}
fn bin(kind: Binary, dtype: u32, left: ScalarExpr, right: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Binary {
        kind,
        dtype,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn scale(dtype: u32, value: ScalarExpr, constant: f64) -> Result<ScalarExpr, String> {
    Ok(bin(Binary::Mul, dtype, value, lit(dtype, constant)?))
}
fn recip(dtype: u32, value: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Reciprocal {
        dtype,
        value: Box::new(value),
    }
}
fn sigmoid(dtype: u32, value: ScalarExpr) -> Result<ScalarExpr, String> {
    let x = scale(dtype, value, -1. / std::f64::consts::LN_2)?;
    Ok(recip(
        dtype,
        bin(
            Binary::Add,
            dtype,
            lit(dtype, 1.)?,
            ScalarExpr::Exp2 {
                dtype,
                value: Box::new(x),
            },
        ),
    ))
}
fn mad(dtype: u32, a: ScalarExpr, b: ScalarExpr, c: ScalarExpr) -> ScalarExpr {
    ScalarExpr::MulAcc {
        dtype,
        a: Box::new(a),
        b: Box::new(b),
        c: Box::new(c),
    }
}
pub(super) fn exp(dtype: u32, value: ScalarExpr) -> Result<ScalarExpr, String> {
    let wide = scalar::promote(dtype, 1)?;
    Ok(cast(
        dtype,
        ScalarExpr::Exp2 {
            dtype: wide,
            value: Box::new(scale(wide, cast(wide, value), 1. / std::f64::consts::LN_2)?),
        },
    ))
}
fn gelu_none(dtype: u32, value: ScalarExpr) -> Result<ScalarExpr, String> {
    // Original to_program algebraically combines scalar factors, introduces
    // MULACC in the Horner polynomial and reassociates the final half scale.
    // Preserve those operations rather than adding extra storage roundings.
    let inv = 1. / std::f64::consts::SQRT_2;
    let x = cast(dtype, value);
    let sign = ScalarExpr::Sign {
        dtype,
        value: Box::new(scale(dtype, x.clone(), inv)?),
    };
    let abs = bin(Binary::Mul, dtype, x.clone(), sign.clone());
    let t = recip(
        dtype,
        mad(dtype, abs, lit(dtype, 0.3275911 * inv)?, lit(dtype, 1.)?),
    );
    let mut polynomial = lit(dtype, 1.061405429)?;
    for c in [-1.453152027, 1.421413741, -0.284496736, 0.254829592] {
        polynomial = mad(dtype, polynomial, t.clone(), lit(dtype, c)?);
    }
    let squared = bin(Binary::Mul, dtype, x.clone(), x.clone());
    let factor = -(inv * inv);
    let exponential = if matches!(dtype, 1 | 11) {
        ScalarExpr::Exp2 {
            dtype,
            value: Box::new(scale(dtype, squared, factor / std::f64::consts::LN_2)?),
        }
    } else {
        exp(dtype, scale(dtype, squared, factor)?)?
    };
    let tail = bin(
        Binary::Mul,
        dtype,
        bin(Binary::Mul, dtype, t, polynomial),
        exponential,
    );
    let inner = bin(Binary::Sub, dtype, lit(dtype, 1.)?, tail);
    scale(
        dtype,
        bin(
            Binary::Mul,
            dtype,
            x,
            mad(dtype, sign, inner, lit(dtype, 1.)?),
        ),
        0.5,
    )
}
impl LoweredGraph {
    pub(super) fn lower_activation(
        &self,
        node: &Node,
        a: usize,
        input: &TensorType,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let dtype = if matches!(input.dtype, 1 | 10 | 11 | 16) {
            input.dtype
        } else {
            1
        };
        let value = ScalarExpr::Load {
            plan: a,
            index: IndexExpr::Index,
        };
        let expression = if node.op == "Sigmoid" {
            sigmoid(dtype, value)?
        } else {
            let approximate = match node.attributes.get("approximate") {
                None => "none",
                Some(Attribute::String(s)) => s.as_str(),
                _ => return Err("invalid Gelu approximate attribute".into()),
            };
            match approximate {
                "none" => gelu_none(dtype, value)?,
                "tanh" => {
                    let square = bin(Binary::Mul, input.dtype, value.clone(), value.clone());
                    let cube = bin(Binary::Mul, input.dtype, square, value.clone());
                    let inner = mad(
                        dtype,
                        lit(dtype, 0.044715)?,
                        cube,
                        cast(dtype, value.clone()),
                    );
                    let scaled = scale(
                        dtype,
                        inner,
                        -2. * (2. / std::f64::consts::PI).sqrt() / std::f64::consts::LN_2,
                    )?;
                    let gate = recip(
                        dtype,
                        bin(
                            Binary::Add,
                            dtype,
                            lit(dtype, 1.)?,
                            ScalarExpr::Exp2 {
                                dtype,
                                value: Box::new(scaled),
                            },
                        ),
                    );
                    bin(Binary::Mul, dtype, gate, cast(dtype, value))
                }
                _ => return Err("unsupported Gelu approximation".into()),
            }
        };
        Ok((
            TensorType {
                dtype,
                shape: input.shape.clone(),
            },
            expression,
        ))
    }
}
