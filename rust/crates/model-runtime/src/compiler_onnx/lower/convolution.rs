use super::*;
fn list(node: &Node, name: &str, rank: usize, default: i64) -> Result<Vec<i64>, String> {
    let values = match node.attributes.get(name) {
        None => vec![default; rank],
        Some(Attribute::Integer(v)) => vec![*v; rank],
        Some(Attribute::Integers(v)) => v.clone(),
        _ => return Err(format!("invalid Conv {name}")),
    };
    if values.len() != rank {
        return Err(format!("invalid Conv {name} rank"));
    }
    Ok(values)
}
impl LoweredGraph {
    pub(super) fn lower_convolution(
        &mut self,
        node: &Node,
        a: usize,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let w = self.input(node, 1)?;
        let x = self.plans[a].ty.clone();
        let weight = self.plans[w].ty.clone();
        if x.shape.len() < 3 || weight.shape.len() != x.shape.len() {
            return Err("Conv requires matching spatial ranks".into());
        }
        let rank = x.shape.len() - 2;
        if self.offline_images && (x.dtype == 9 || weight.dtype == 9) {
            return Err("offline image bool packing is not implemented".into());
        }
        if self.offline_images && rank != 2 {
            return Err("original image convolution requires 2D spatial input".into());
        }
        let groups = match node.attributes.get("group") {
            None => 1,
            Some(Attribute::Integer(v)) if *v > 0 => *v as usize,
            _ => return Err("invalid Conv groups".into()),
        };
        if weight.shape[1].checked_mul(groups) != Some(x.shape[1])
            || !weight.shape[0].is_multiple_of(groups)
        {
            return Err("Conv channel/group mismatch".into());
        }
        let stride = list(node, "strides", rank, 1)?;
        let dilation = list(node, "dilations", rank, 1)?;
        if stride.iter().any(|v| *v <= 0)
            || dilation.iter().any(|v| *v < 0)
            || weight.shape[2..].contains(&0)
        {
            return Err("invalid Conv stride/dilation/kernel".into());
        }
        let auto = match node.attributes.get("auto_pad") {
            None => "NOTSET",
            Some(Attribute::String(s)) => s.as_str(),
            _ => return Err("invalid Conv auto_pad".into()),
        };
        let mut pads = match node.attributes.get("pads") {
            None => vec![0; rank * 2],
            Some(Attribute::Integer(v)) => vec![*v; rank * 2],
            Some(Attribute::Integers(v)) if v.len() == rank => v.iter().chain(v).copied().collect(),
            Some(Attribute::Integers(v)) if v.len() == rank * 2 => v.clone(),
            _ => return Err("invalid Conv pads".into()),
        };
        if auto == "VALID" {
            pads.fill(0);
        } else if auto != "NOTSET" {
            let kernel = match node.attributes.get("kernel_shape") {
                None => weight.shape[2..].iter().map(|v| *v as i64).collect(),
                _ => list(node, "kernel_shape", rank, 0)?,
            };
            for i in 0..rank {
                // Preserve the original helper: auto-padding does not multiply
                // kernel size by dilation, and any non-UPPER token uses LOWER.
                let input = x.shape[i + 2] as i64;
                let output = (input - 1).div_euclid(stride[i]) + 1;
                let total = (output - 1) * stride[i] + kernel[i] - input;
                pads[i] = if auto == "SAME_UPPER" {
                    total.div_euclid(2)
                } else {
                    (total + 1).div_euclid(2)
                };
                pads[i + rank] = total - pads[i];
            }
        }
        let mut shape = vec![x.shape[0], weight.shape[0]];
        for i in 0..rank {
            let padded = x.shape[i + 2] as i128 + pads[i] as i128 + pads[i + rank] as i128;
            let effective = (weight.shape[i + 2] as i128 - 1) * dilation[i] as i128 + 1;
            if padded < 0 || padded < effective {
                return Err("Conv kernel exceeds padded input".into());
            }
            shape.push(
                usize::try_from((padded - effective) / stride[i] as i128 + 1)
                    .map_err(|_| "Conv output overflow")?,
            );
        }
        let reduce_shape = weight.shape[1..].to_vec();
        let count = reduce_shape.iter().try_fold(1usize, |a, b| {
            a.checked_mul(*b).ok_or("Conv reduction overflow")
        })?;
        let output_channel = coordinate(&shape, 1);
        let input_channel = add(
            mul(
                IndexExpr::Div(Box::new(output_channel.clone()), weight.shape[0] / groups),
                weight.shape[1],
            ),
            reduction::reduce_coordinate(&reduce_shape, 0),
        );
        let mut coords = vec![
            (coordinate(&shape, 0), 0, x.shape[0]),
            (input_channel, 0, x.shape[1]),
        ];
        for i in 0..rank {
            let at = add(
                add(
                    mul(coordinate(&shape, i + 2), stride[i] as usize),
                    mul(
                        reduction::reduce_coordinate(&reduce_shape, i + 1),
                        dilation[i] as usize,
                    ),
                ),
                IndexExpr::Constant(-(pads[i] as i128)),
            );
            let low =
                usize::try_from((-(pads[i] as i128)).max(0)).map_err(|_| "Conv crop overflow")?;
            let high = x.shape[i + 2]
                .checked_sub(
                    usize::try_from((-(pads[i + rank] as i128)).max(0))
                        .map_err(|_| "Conv crop overflow")?,
                )
                .ok_or("Conv crop exceeds input")?;
            if low > high {
                return Err("Conv crop exceeds input".into());
            }
            coords.push((at, low, high));
        }
        let weight_index = add(mul(output_channel, count), IndexExpr::Reduction);
        let dtype = if self.offline_images {
            1
        } else {
            scalar::promote(x.dtype, weight.dtype)?
        };
        let accumulator = reduction::accumulate(dtype)?;
        let sum = ScalarExpr::ReduceSum {
            dtype: accumulator,
            count,
            value: Box::new(ScalarExpr::Cast {
                dtype: accumulator,
                value: Box::new(ScalarExpr::Binary {
                    kind: Binary::Mul,
                    dtype,
                    left: Box::new(image_load(
                        self.offline_images,
                        ScalarExpr::PaddedLoad {
                            plan: a,
                            coordinates: coords,
                        },
                    )),
                    right: Box::new(image_load(
                        self.offline_images,
                        ScalarExpr::Load {
                            plan: w,
                            index: weight_index,
                        },
                    )),
                }),
            }),
        };
        let result_dtype = if matches!(dtype, 10 | 16) {
            dtype
        } else {
            accumulator
        };
        let ty = TensorType {
            dtype: result_dtype,
            shape,
        };
        let expression = ScalarExpr::Cast {
            dtype: result_dtype,
            value: Box::new(sum),
        };
        if node.inputs.get(2).is_some_and(|v| !v.is_empty()) {
            let bias = self.input(node, 2)?;
            let bias_count = self.plans[bias].ty.count()?;
            if bias_count != 1 && bias_count != ty.shape[1] {
                return Err("Conv bias channel mismatch".into());
            }
            let temporary = self.temporary(ty.clone(), expression)?;
            let dtype = scalar::promote(ty.dtype, self.plans[bias].ty.dtype)?;
            let expression = ScalarExpr::Binary {
                kind: Binary::Add,
                dtype,
                left: Box::new(ScalarExpr::Load {
                    plan: temporary,
                    index: IndexExpr::Index,
                }),
                right: Box::new(ScalarExpr::Load {
                    plan: bias,
                    index: if bias_count == 1 {
                        IndexExpr::Constant(0)
                    } else {
                        coordinate(&ty.shape, 1)
                    },
                }),
            };
            Ok((TensorType { dtype, ..ty }, expression))
        } else {
            Ok((ty, expression))
        }
    }
}

fn image_load(image: bool, value: ScalarExpr) -> ScalarExpr {
    if image {
        ScalarExpr::Cast {
            dtype: 1,
            value: Box::new(ScalarExpr::StorageRound {
                dtype: 10,
                value: Box::new(value),
            }),
        }
    } else {
        value
    }
}
