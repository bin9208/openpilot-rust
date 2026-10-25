use super::*;
fn bin(kind: Binary, dtype: u32, left: ScalarExpr, right: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Binary {
        kind,
        dtype,
        left: Box::new(left),
        right: Box::new(right),
    }
}
fn cast(dtype: u32, value: ScalarExpr) -> ScalarExpr {
    ScalarExpr::Cast {
        dtype,
        value: Box::new(value),
    }
}
impl LoweredGraph {
    fn statistic(&mut self, model: &Model, id: usize, axes: Vec<i64>) -> Result<usize, String> {
        let node = Node {
            name: String::new(),
            op: "ReduceMean".into(),
            domain: String::new(),
            opset: 20,
            inputs: Vec::new(),
            outputs: Vec::new(),
            attributes: BTreeMap::from([
                ("axes".into(), Attribute::Integers(axes)),
                ("keepdims".into(), Attribute::Integer(1)),
                ("noop_with_empty_axes".into(), Attribute::Integer(1)),
            ]),
        };
        let (ty, expression) = self.lower_reduction(model, &node, id, &self.plans[id].ty)?;
        self.temporary(ty, expression)
    }
    pub(super) fn lower_normalization(&mut self, model: &Model, node: &Node) -> Result<(), String> {
        if node.outputs.is_empty() || node.outputs.len() > 3 {
            return Err("LayerNormalization requires one to three outputs".into());
        }
        if matches!(node.attributes.get("stash_type"),Some(Attribute::Integer(v))if *v!=1) {
            return Err("only float32 LayerNormalization stash_type is supported".into());
        }
        let a = self.input(node, 0)?;
        let scale = self.input(node, 1)?;
        let input = self.plans[a].ty.clone();
        let axis = match node.attributes.get("axis") {
            None => -1,
            Some(Attribute::Integer(v)) => *v,
            _ => return Err("invalid LayerNormalization axis".into()),
        };
        let start = if axis < 0 {
            input.shape.len() as i64 + axis
        } else {
            axis
        };
        if input.shape.is_empty()
            || start >= input.shape.len() as i64
            || start < -(input.shape.len() as i64)
        {
            return Err("LayerNormalization axis outside original executable contract".into());
        }
        // The source builds range(start,rank) before normalizing each axis.
        // Negative entries may therefore repeat an axis, which its scheduler
        // accepts and reduces once (for example rank2 axis=-3).
        let axes = (start..input.shape.len() as i64).collect::<Vec<_>>();
        let epsilon = match node.attributes.get("epsilon") {
            None => 1e-5,
            Some(Attribute::Float(v)) => *v as f64,
            _ => return Err("invalid LayerNormalization epsilon".into()),
        };
        let x = self.temporary(
            TensorType {
                dtype: 1,
                shape: input.shape.clone(),
            },
            cast(
                1,
                ScalarExpr::Load {
                    plan: a,
                    index: IndexExpr::Index,
                },
            ),
        )?;
        let mean = self.statistic(model, x, axes.clone())?;
        let centered = bin(
            Binary::Sub,
            1,
            self.load(x, &input.shape),
            self.load(mean, &input.shape),
        );
        let square = self.temporary(
            TensorType {
                dtype: 1,
                shape: input.shape.clone(),
            },
            bin(Binary::Mul, 1, centered.clone(), centered.clone()),
        )?;
        let variance = self.statistic(model, square, axes)?;
        let stat_ty = self.plans[variance].ty.clone();
        let added = bin(
            Binary::Add,
            1,
            ScalarExpr::Load {
                plan: variance,
                index: IndexExpr::Index,
            },
            ScalarExpr::FloatLiteral {
                dtype: 1,
                value: epsilon,
            },
        );
        let inverse = self.temporary(
            stat_ty,
            ScalarExpr::Reciprocal {
                dtype: 1,
                value: Box::new(ScalarExpr::Sqrt {
                    dtype: 1,
                    value: Box::new(added),
                }),
            },
        )?;
        let normalized = self.temporary(
            input.clone(),
            cast(
                input.dtype,
                bin(Binary::Mul, 1, centered, self.load(inverse, &input.shape)),
            ),
        )?;
        let mut ty = TensorType {
            dtype: scalar::promote(input.dtype, self.plans[scale].ty.dtype)?,
            shape: broadcast(&[&input.shape, &self.plans[scale].ty.shape])?,
        };
        let mut expression = bin(
            Binary::Mul,
            ty.dtype,
            self.load(normalized, &ty.shape),
            self.load(scale, &ty.shape),
        );
        if node.inputs.get(2).is_some_and(|s| !s.is_empty()) {
            let bias = self.input(node, 2)?;
            let result = self.temporary(ty.clone(), expression)?;
            ty.shape = broadcast(&[&ty.shape, &self.plans[bias].ty.shape])?;
            ty.dtype = scalar::promote(ty.dtype, self.plans[bias].ty.dtype)?;
            expression = bin(
                Binary::Add,
                ty.dtype,
                self.load(result, &ty.shape),
                self.load(bias, &ty.shape),
            );
        }
        self.push(&node.outputs[0], ty, expression)?;
        if let Some(name) = node.outputs.get(1) {
            if !name.is_empty() {
                self.names.insert(name.clone(), mean);
            }
        }
        if let Some(name) = node.outputs.get(2) {
            if !name.is_empty() {
                self.names.insert(name.clone(), inverse);
            }
        }
        Ok(())
    }
    pub(super) fn lower_softmax(
        &mut self,
        node: &Node,
        a: usize,
        input: &TensorType,
    ) -> Result<(TensorType, ScalarExpr), String> {
        if !matches!(node.domain.as_str(), "" | "ai.onnx") {
            return Err("Softmax unsupported operator domain".into());
        }
        let dimension = match node.attributes.get("axis") {
            None => {
                if node.opset < 13 {
                    1
                } else {
                    -1
                }
            }
            Some(Attribute::Integer(v)) => *v,
            _ => return Err("invalid Softmax axis".into()),
        };
        let dimension = if input.shape.is_empty() && matches!(dimension, -1 | 0) {
            None
        } else {
            Some(axis(dimension, input.shape.len())?)
        };
        let mut stats = input.shape.clone();
        if let Some(i) = dimension {
            stats[i] = 1;
        }
        let index = index_from_coordinates(
            &input.shape,
            (0..input.shape.len()).map(|i| {
                if dimension == Some(i) {
                    IndexExpr::Reduction
                } else {
                    coordinate(&stats, i)
                }
            }),
        );
        let count = dimension.map(|i| input.shape[i]).unwrap_or(1);
        let maximum = self.temporary(
            TensorType {
                dtype: input.dtype,
                shape: stats.clone(),
            },
            ScalarExpr::ReduceMax {
                dtype: input.dtype,
                count,
                value: Box::new(ScalarExpr::Load { plan: a, index }),
            },
        )?;
        let left = self.load(a, &input.shape);
        let mut right = self.load(maximum, &input.shape);
        let kind = if input.dtype == 9 {
            right = ScalarExpr::BoolNot(Box::new(right));
            Binary::Add
        } else {
            Binary::Sub
        };
        let center = bin(kind, input.dtype, left, right);
        let dtype = if matches!(input.dtype, 1 | 10 | 11 | 16) {
            input.dtype
        } else {
            1
        };
        let exponential = self.temporary(
            TensorType {
                dtype,
                shape: input.shape.clone(),
            },
            super::activation::exp(dtype, center)?,
        )?;
        let index = index_from_coordinates(
            &input.shape,
            (0..input.shape.len()).map(|i| {
                if dimension == Some(i) {
                    IndexExpr::Reduction
                } else {
                    coordinate(&stats, i)
                }
            }),
        );
        let accumulator = scalar::promote(dtype, 1)?;
        let sum = cast(
            dtype,
            ScalarExpr::ReduceSum {
                dtype: accumulator,
                count,
                value: Box::new(cast(
                    accumulator,
                    ScalarExpr::Load {
                        plan: exponential,
                        index,
                    },
                )),
            },
        );
        let denominator = self.temporary(
            TensorType {
                dtype,
                shape: stats,
            },
            sum,
        )?;
        let expression = bin(
            Binary::Mul,
            dtype,
            self.load(exponential, &input.shape),
            ScalarExpr::Reciprocal {
                dtype,
                value: Box::new(self.load(denominator, &input.shape)),
            },
        );
        Ok((
            TensorType {
                dtype,
                shape: input.shape.clone(),
            },
            expression,
        ))
    }
}
