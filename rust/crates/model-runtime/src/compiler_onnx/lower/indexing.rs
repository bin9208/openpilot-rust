use super::*;
impl LoweredGraph {
    pub(super) fn lower_gather(
        &self,
        model: &Model,
        node: &Node,
        a: usize,
        input: &TensorType,
    ) -> Result<(TensorType, ScalarExpr), String> {
        let b = self.input(node, 1)?;
        let indices = &self.plans[b].ty;
        let constant = node
            .inputs
            .get(1)
            .and_then(|name| model.graph.initializers.get(name));
        if !(matches!(indices.dtype, 2 | 3 | 4 | 5 | 6 | 7 | 12 | 13)
            || node.op == "Gather" && constant.is_some() && indices.dtype == 9)
        {
            return Err("gather indices require integer tensors".into());
        }
        if node.op == "Gather" {
            let dimension = match node.attributes.get("axis") {
                None => 0,
                Some(Attribute::Integer(v)) => *v,
                _ => return Err("invalid Gather axis".into()),
            };
            let dimension = axis(dimension, input.shape.len())?;
            let size = input.shape[dimension];
            if let Some(tensor) = constant {
                if indices.shape.len() > 1 || indices.count()? == 0 {
                    return Err(
                        "original constant Gather requires scalar or nonempty vector indices"
                            .into(),
                    );
                }
                let width = super::super::parse::width(tensor.dtype)?;
                for raw in tensor.data.chunks_exact(width) {
                    let mut index = Scalar::read(tensor.dtype, raw)?.integer()?;
                    if index < 0 {
                        index += size as i128;
                    }
                    if index < 0 || index >= size as i128 {
                        return Err("constant Gather shrink index outside tensor".into());
                    }
                }
            }
            let mut shape = input.shape[..dimension].to_vec();
            shape.extend(&indices.shape);
            shape.extend(&input.shape[dimension + 1..]);
            let base = index_from_coordinates(
                &input.shape,
                (0..input.shape.len()).map(|i| {
                    if i == dimension {
                        IndexExpr::Constant(0)
                    } else {
                        coordinate(
                            &shape,
                            if i < dimension {
                                i
                            } else {
                                i + indices.shape.len() - 1
                            },
                        )
                    }
                }),
            );
            let index = index_from_coordinates(
                &indices.shape,
                (0..indices.shape.len()).map(|i| coordinate(&shape, dimension + i)),
            );
            let value = ScalarExpr::Load { plan: b, index };
            let stride = input.shape[dimension + 1..].iter().product();
            return Ok((
                TensorType {
                    dtype: input.dtype,
                    shape,
                },
                ScalarExpr::GatherLoad {
                    plan: a,
                    base,
                    axes: vec![(value, size, stride)],
                },
            ));
        }
        let batch = match node.attributes.get("batch_dims") {
            None => 0,
            Some(Attribute::Integer(v)) => {
                usize::try_from(*v).map_err(|_| "negative GatherND batch_dims")?
            }
            _ => return Err("invalid GatherND batch_dims".into()),
        };
        let rank = indices.shape.len();
        if rank == 0 || batch >= rank || batch > input.shape.len() {
            return Err("invalid GatherND rank or batch_dims".into());
        }
        let k = indices.shape[rank - 1];
        if k > input.shape.len() - batch {
            return Err("GatherND index tuple exceeds tensor rank".into());
        }
        if k == 0 && batch == 0 {
            if input.shape.is_empty() {
                return Err("empty GatherND tuple cannot index scalar data".into());
            }
            // The original split(1) on a zero-length last dimension yields
            // one empty index tensor, rather than an empty index tuple.
            let mut shape = indices.shape.clone();
            shape.extend(&input.shape[1..]);
            return Ok((
                TensorType {
                    dtype: input.dtype,
                    shape,
                },
                ScalarExpr::Load {
                    plan: a,
                    index: IndexExpr::Index,
                },
            ));
        }
        let batches = input.shape[..batch].iter().product::<usize>();
        if indices.shape[..batch].iter().product::<usize>() != batches {
            return Err("GatherND batch size mismatch".into());
        }
        let extra = &indices.shape[batch..rank - 1];
        let mut shape = input.shape[..batch].to_vec();
        shape.extend(extra);
        shape.extend(&input.shape[batch + k..]);
        let base = index_from_coordinates(
            &input.shape,
            (0..input.shape.len()).map(|i| {
                if i < batch {
                    coordinate(&shape, i)
                } else if i < batch + k {
                    IndexExpr::Constant(0)
                } else {
                    coordinate(&shape, batch + extra.len() + i - batch - k)
                }
            }),
        );
        let batch_index = index_from_coordinates(
            &input.shape[..batch],
            (0..batch).map(|i| coordinate(&shape, i)),
        );
        let extra_index = index_from_coordinates(
            extra,
            (0..extra.len()).map(|i| coordinate(&shape, batch + i)),
        );
        let prefix = add(mul(batch_index, extra.iter().product()), extra_index);
        let mut axes = Vec::new();
        for i in 0..k {
            let index = add(mul(prefix.clone(), k), IndexExpr::Constant(i as i128));
            let loaded = ScalarExpr::Load { plan: b, index };
            axes.push((
                loaded,
                input.shape[batch + i],
                input.shape[batch + i + 1..].iter().product(),
            ));
        }
        Ok((
            TensorType {
                dtype: input.dtype,
                shape,
            },
            ScalarExpr::GatherLoad {
                plan: a,
                base,
                axes,
            },
        ))
    }
}
