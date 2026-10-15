use super::*;
pub(super) fn accumulate(dtype: u32) -> Result<u32, String> {
    Ok(match dtype {
        2 | 4 | 12 | 13 => scalar::promote(dtype, 12)?,
        3 | 5 | 6 | 7 | 9 => scalar::promote(dtype, 6)?,
        _ => scalar::promote(dtype, 1)?,
    })
}
fn float_dtype(dtype: u32) -> Result<u32, String> {
    if matches!(dtype, 1 | 10 | 11 | 16) {
        Ok(dtype)
    } else {
        scalar::promote(dtype, 1)
    }
}
fn cast(dtype: u32, value: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Cast {
        dtype,
        value: Box::new(value),
    }
}
fn binary(kind: Binary, dtype: u32, left: ScalarExpr, right: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Binary {
        kind,
        dtype,
        left: Box::new(left),
        right: Box::new(right),
    }
}
pub(super) fn reduce_coordinate(shape: &[usize], axis: usize) -> IndexExpr {
    IndexExpr::Mod(
        Box::new(IndexExpr::Div(
            Box::new(IndexExpr::Reduction),
            shape[axis + 1..].iter().product(),
        )),
        shape[axis],
    )
}
impl LoweredGraph {
    pub(super) fn lower_reduction(
        &self,
        model: &Model,
        node: &Node,
        a: usize,
        input: &TensorType,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let axes = self.control(model, node, 1, "axes")?;
        let noop = matches!(node.attributes.get("noop_with_empty_axes"),Some(Attribute::Integer(v))if *v!=0);
        let axes = axes.filter(|v| !v.is_empty()).unwrap_or_else(|| {
            if noop {
                Vec::new()
            } else {
                (0..input.shape.len() as i64).collect()
            }
        });
        let mut axes = axes
            .into_iter()
            .map(|v| {
                if input.shape.is_empty() && matches!(v, 0 | -1) {
                    Ok(0)
                } else {
                    axis(v, input.shape.len())
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        if input.shape.is_empty() {
            axes.clear();
        }
        axes.sort_unstable();
        // The pinned scheduler reduces repeated axes once; preserve its
        // accepted source behavior rather than imposing stricter ONNX rules.
        axes.dedup();
        let keep = !matches!(node.attributes.get("keepdims"), Some(Attribute::Integer(0)));
        let shape = input
            .shape
            .iter()
            .enumerate()
            .filter_map(|(i, &size)| {
                if axes.contains(&i) {
                    if keep {
                        Some(1)
                    } else {
                        None
                    }
                } else {
                    Some(size)
                }
            })
            .collect::<Vec<_>>();
        let reduced = axes.iter().map(|&i| input.shape[i]).collect::<Vec<_>>();
        let count = reduced.iter().try_fold(1usize, |n, &v| {
            n.checked_mul(v).ok_or("reduction size overflow")
        })?;
        let index = index_from_coordinates(
            &input.shape,
            (0..input.shape.len()).map(|i| {
                if let Some(j) = axes.iter().position(|&v| v == i) {
                    reduce_coordinate(&reduced, j)
                } else {
                    let axis = if keep {
                        i
                    } else {
                        (0..i).filter(|v| !axes.contains(v)).count()
                    };
                    coordinate(&shape, axis)
                }
            }),
        );
        let loaded = ScalarExpr::Load { plan: a, index };
        let expression = if node.op == "ReduceMean" {
            let dtype = accumulate(input.dtype)?;
            let sum = ScalarExpr::ReduceSum {
                dtype,
                count,
                value: Box::new(cast(dtype, loaded)),
            };
            let division_dtype = float_dtype(dtype)?;
            let reciprocal = ScalarExpr::Reciprocal {
                dtype: division_dtype,
                value: Box::new(ScalarExpr::Literal(Scalar::from_integer(
                    dtype,
                    count as i128,
                )?)),
            };
            cast(
                if matches!(input.dtype, 1 | 10 | 11 | 16) {
                    input.dtype
                } else {
                    1
                },
                binary(Binary::Mul, division_dtype, sum, reciprocal),
            )
        } else {
            let dtype = if matches!(input.dtype, 10 | 16) {
                1
            } else {
                input.dtype
            };
            let squared = binary(
                Binary::Mul,
                dtype,
                cast(dtype, loaded.clone()),
                cast(dtype, loaded),
            );
            let accumulator = accumulate(dtype)?;
            let sum = ScalarExpr::ReduceSum {
                dtype: accumulator,
                count,
                value: Box::new(cast(accumulator, squared)),
            };
            cast(
                input.dtype,
                ScalarExpr::Sqrt {
                    dtype: float_dtype(accumulator)?,
                    value: Box::new(sum),
                },
            )
        };
        let dtype = if node.op == "ReduceMean" && !matches!(input.dtype, 1 | 10 | 11 | 16) {
            1
        } else {
            input.dtype
        };
        Ok((TensorType { dtype, shape }, expression))
    }
    pub(super) fn lower_matmul(
        &self,
        a: usize,
        b: usize,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let x = &self.plans[a].ty;
        let y = &self.plans[b].ty;
        let (rx, ry) = (x.shape.len(), y.shape.len());
        if self.offline_images && (x.shape.contains(&0) || y.shape.contains(&0)) {
            return Err("original image_dot cannot reshape empty tensors".into());
        }
        if self.offline_images && (x.dtype == 9 || y.dtype == 9) {
            return Err("offline image bool packing is not implemented".into());
        }
        if rx == 0 || ry == 0 {
            return Err("both tensors need to be at least 1D".into());
        }
        if self.offline_images
            && (ry < 2
                || x.shape[..rx.saturating_sub(2)].iter().product::<usize>()
                    % y.shape[..ry - 2].iter().product::<usize>()
                    != 0)
        {
            return Err("original image_dot cannot reshape these batch dimensions".into());
        }
        let contracted_y = ry.saturating_sub(2);
        let k = x.shape[rx - 1];
        if k != y.shape[contracted_y] {
            return Err("cannot dot mismatched contracted dimensions".into());
        }
        let xb = if rx > 1 { &x.shape[..rx - 2] } else { &[] };
        let yb = if ry > 1 { &y.shape[..ry - 2] } else { &[] };
        let batch = broadcast(&[xb, yb])?;
        let mut shape = batch.clone();
        if rx > 1 {
            shape.push(x.shape[rx - 2]);
        }
        if ry > 1 {
            shape.push(y.shape[ry - 1]);
        }
        let xi = index_from_coordinates(
            &x.shape,
            (0..rx).map(|i| {
                if i == rx - 1 {
                    IndexExpr::Reduction
                } else if i == rx - 2 {
                    coordinate(&shape, batch.len())
                } else {
                    let axis = batch.len() - xb.len() + i;
                    if x.shape[i] == 1 {
                        IndexExpr::Constant(0)
                    } else {
                        coordinate(&shape, axis)
                    }
                }
            }),
        );
        let yi = index_from_coordinates(
            &y.shape,
            (0..ry).map(|i| {
                if i == contracted_y {
                    IndexExpr::Reduction
                } else if i == ry - 1 {
                    coordinate(&shape, batch.len() + usize::from(rx > 1))
                } else {
                    let axis = batch.len() - yb.len() + i;
                    if y.shape[i] == 1 {
                        IndexExpr::Constant(0)
                    } else {
                        coordinate(&shape, axis)
                    }
                }
            }),
        );
        let dtype = if self.offline_images {
            1
        } else {
            scalar::promote(x.dtype, y.dtype)?
        };
        let accumulator = accumulate(dtype)?;
        let product = binary(
            Binary::Mul,
            dtype,
            image_load(self.offline_images, ScalarExpr::Load { plan: a, index: xi }),
            image_load(self.offline_images, ScalarExpr::Load { plan: b, index: yi }),
        );
        let expression = cast(
            dtype,
            ScalarExpr::ReduceSum {
                dtype: accumulator,
                count: k,
                value: Box::new(cast(accumulator, product)),
            },
        );
        Ok((TensorType { dtype, shape }, expression))
    }
    pub(super) fn lower_gemm(
        &mut self,
        node: &Node,
        mut a: usize,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let mut b = self.input(node, 1)?;
        for (key, id) in [("transA", &mut a), ("transB", &mut b)] {
            let dim = match node.attributes.get(key) {
                None => 0,
                Some(Attribute::Integer(v)) => *v,
                _ => return Err("invalid Gemm transpose".into()),
            };
            let input = &self.plans[*id].ty;
            let rank = input.shape.len();
            if rank < 2 {
                return Err("Gemm transpose requires rank at least two".into());
            }
            let mut order = (0..rank).collect::<Vec<_>>();
            let d = axis(dim, rank)?;
            order.swap(d, 0);
            let shape = order.iter().map(|&i| input.shape[i]).collect::<Vec<_>>();
            let index = index_from_coordinates(
                &input.shape,
                (0..rank).map(|i| coordinate(&shape, order.iter().position(|&v| v == i).unwrap())),
            );
            *id = self.temporary(
                TensorType {
                    dtype: input.dtype,
                    shape,
                },
                ScalarExpr::Load { plan: *id, index },
            )?;
        }
        let (mut ty, mut expression) = self.lower_matmul(a, b)?;
        let scale = |key: &str, dtype: u32| -> Result<(u32, ScalarExpr), String> {
            match node.attributes.get(key) {
                Some(Attribute::Integer(v)) => {
                    let dtype = if dtype == 9 { 6 } else { dtype };
                    Ok((
                        dtype,
                        ScalarExpr::Literal(Scalar::from_integer(dtype, *v as i128)?),
                    ))
                }
                None | Some(Attribute::Float(_)) => {
                    let dtype = if matches!(dtype, 1 | 10 | 11 | 16) {
                        dtype
                    } else {
                        1
                    };
                    let value = match node.attributes.get(key) {
                        Some(Attribute::Float(v)) => *v as f64,
                        _ => 1.,
                    };
                    Ok((dtype, ScalarExpr::FloatLiteral { dtype, value }))
                }
                _ => Err("invalid Gemm scale".into()),
            }
        };
        let (scalar_dtype, alpha) = scale("alpha", ty.dtype)?;
        ty.dtype = scalar::promote(ty.dtype, scalar_dtype)?;
        expression = binary(Binary::Mul, ty.dtype, alpha, expression);
        if node.inputs.get(2).is_some_and(|v| !v.is_empty()) {
            let c = self.input(node, 2)?;
            let ct = self.plans[c].ty.clone();
            let (scaled_dtype, beta) = scale("beta", ct.dtype)?;
            let mut shape = ct.shape.clone();
            if matches!(node.attributes.get("broadcast"),Some(Attribute::Integer(v))if *v!=0) {
                let rank = ty.shape.len();
                let mut request = (0..rank)
                    .map(|i| if i < ct.shape.len() { -1 } else { 1 })
                    .collect::<Vec<_>>();
                request.reverse();
                if request.iter().filter(|&&v| v == -1).count() > 1 {
                    return Err("Gemm legacy bias reshape has multiple inferred dimensions".into());
                }
                shape = request
                    .into_iter()
                    .map(|v| if v == -1 { ct.count().unwrap() } else { 1 })
                    .collect();
            }
            let bias_dtype = scalar::promote(ct.dtype, scaled_dtype)?;
            let output = broadcast(&[&ty.shape, &shape])?;
            // Materialize the dot result before broadcasting its output into the bias.
            let result = self.temporary(ty, expression)?;
            let loaded = ScalarExpr::Load {
                plan: c,
                index: broadcast_index(&shape, &output),
            };
            let bias = binary(Binary::Mul, bias_dtype, beta, loaded);
            let dtype = scalar::promote(self.plans[result].ty.dtype, bias_dtype)?;
            expression = binary(Binary::Add, dtype, self.load(result, &output), bias);
            ty = TensorType {
                dtype,
                shape: output,
            };
        }
        Ok((ty, expression))
    }
}

fn image_load(image: bool, value: ScalarExpr) -> ScalarExpr {
    if image {
        cast(1, cast(10, value))
    } else {
        value
    }
}
