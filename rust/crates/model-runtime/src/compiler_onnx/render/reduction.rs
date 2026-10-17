use super::super::lower;
use super::*;
impl Renderer<'_> {
    pub(super) fn sqrt_value_supported(&self, value: &ScalarExpr) -> bool {
        // Primitive capture is a sole selected final output. Another selected
        // square/computed value can introduce source CSE/materialization, and a
        // downstream consumer can fuse with the norm; neither is proved here.
        if !self.graph.outputs.is_empty()
            && (self.graph.outputs.len() != 1
                || !self.graph.outputs.values().any(|id| *id == self.output))
        {
            return false;
        }
        match value {
            ScalarExpr::Load { plan, .. } => self.direct_norm_input(*plan),
            ScalarExpr::ReduceSum {
                dtype: 1,
                count,
                value,
            } => self.row_square(value, *count),
            _ => false,
        }
    }
    fn row_square(&self, value: &ScalarExpr, count: usize) -> bool {
        fn strip(value: &ScalarExpr) -> &ScalarExpr {
            match value {
                ScalarExpr::Cast { dtype: 1, value } => strip(value),
                _ => value,
            }
        }
        matches!(strip(value), ScalarExpr::Binary {kind: Binary::Mul, dtype:1, left, right}
            if left == right && matches!(strip(left), ScalarExpr::Load {plan,index}
                if self.contiguous_square_input(*plan,index,count)))
    }
    fn direct_norm_input(&self, id: usize) -> bool {
        self.graph
            .plans
            .get(id)
            .is_some_and(|p| p.ty.dtype == 1 && matches!(p.expression, ScalarExpr::Input(_)))
    }
    fn contiguous_square_input(&self, id: usize, index: &IndexExpr, count: usize) -> bool {
        // A single reduced scalar has no independent-output loop for LLVM to
        // vectorize across. Batched reductions need that scheduler port; their
        // SLP rounding cannot be inferred from the reduction width alone.
        if self.count != 1 || !self.direct_norm_input(id) {
            return false;
        }
        let input = &self.graph.plans[id].ty.shape;
        let output = &self.graph.plans[self.output].ty.shape;
        (0..=input.len()).any(|start| {
            input[start..]
                .iter()
                .try_fold(1usize, |n, d| n.checked_mul(*d))
                == Some(count)
                && [false, true].into_iter().any(|keep| {
                    let mut shape = input[..start].to_vec();
                    if keep {
                        shape.extend(std::iter::repeat_n(1, input.len() - start));
                    }
                    shape == *output
                        && *index == lower::trailing_reduction_index(input, start, keep)
                })
        })
    }
    pub(super) fn padded_load(
        &mut self,
        plan: usize,
        coordinates: &[(IndexExpr, usize, usize)],
    ) -> Result<String, String> {
        let input = self
            .graph
            .plans
            .get(plan)
            .ok_or("padded load plan absent")?;
        if coordinates.is_empty() || coordinates.len() != input.ty.shape.len() {
            return Err("padded load rank mismatch".into());
        }
        let dtype = input.ty.dtype;
        storage_type(dtype)?;
        if elements(&input.ty.shape)? > i64::MAX as usize
            || input.ty.shape.iter().any(|d| *d > i64::MAX as usize)
        {
            return Err("padded load storage exceeds signed64 index domain".into());
        }
        if plan >= self.output {
            return Err("padded load must precede output".into());
        }
        let mut terms = Vec::new();
        let mut guards = Vec::new();
        for ((at, low, high), &dimension) in coordinates.iter().zip(&input.ty.shape) {
            if low > high || *high > dimension {
                return Err("padded load crop exceeds storage".into());
            }
            let (at, _, _) = self.index(at)?;
            terms.push((at.clone(), dimension));
            guards.push(format!("({at}>={low}L && {at}<{high}L)"));
        }
        let mut mapping = String::from("0L");
        for (at, dimension) in terms {
            mapping = format!("({mapping}*{dimension}L+({at}))");
        }
        // The condition is evaluated before the subscript: an invalid coordinate
        // must never form an out-of-bounds pointer or perform a speculative load.
        let slot = if let Some(slot) = self.inputs.iter().position(|v| *v == plan) {
            slot
        } else {
            self.inputs.push(plan);
            self.inputs.len() - 1
        };
        let argument = if slot == 0 {
            "inp".into()
        } else {
            format!("inp{slot}")
        };
        let loaded = format!("{argument}[{mapping}]");
        let loaded = if self.cpu_emulated_half && dtype == 10 {
            format!("native_half_load({loaded})")
        } else {
            loaded
        };
        Ok(format!("({}?{loaded}:0.0f)", guards.join("&&")))
    }
    pub(super) fn reduce_sum(
        &mut self,
        count: usize,
        value: &ScalarExpr,
    ) -> Result<String, String> {
        if !self.cpu_emulated_half {
            return Err("OpenCL reduction scheduling not implemented".into());
        }
        if self.reduction.is_some()
            || count == 0
            || count > 1_048_576
            || count
                .checked_mul(self.count)
                .is_none_or(|v| v > 200_000_000)
        {
            return Err("CPU reduction exceeds bounded work or nesting contract".into());
        }
        let id = self.statements.len();
        let index = format!("r{id}");
        let accumulator = format!("sum{id}");
        self.reduction = Some((index.clone(), count));
        fn strip(value: &ScalarExpr) -> &ScalarExpr {
            if let ScalarExpr::Cast { dtype: 1, value } = value {
                strip(value)
            } else {
                value
            }
        }
        let product = match strip(value) {
            ScalarExpr::Binary {
                kind: Binary::Mul,
                dtype: 1,
                left,
                right,
            } => Some((left.as_ref(), right.as_ref())),
            _ => None,
        };
        // Original row squares use a left fold with packed-product rounding and
        // contracted scalar tails. A full unroll starts with its first square;
        // looped reductions start at zero. Other products retain IMAGE admission.
        let square = self.row_square(value, count);
        fn image_operand(value: &ScalarExpr) -> bool {
            match value {
                ScalarExpr::StorageRound { dtype: 10, .. } => true,
                ScalarExpr::Cast { dtype: 1, value } => image_operand(value),
                _ => false,
            }
        }
        let update = if let Some((a, b)) = product {
            fn convolution_input(value: &ScalarExpr) -> bool {
                match value {
                    ScalarExpr::Cast { dtype: 1, value } => convolution_input(value),
                    ScalarExpr::StorageRound { dtype: 10, value } => {
                        matches!(value.as_ref(), ScalarExpr::PaddedLoad { .. })
                    }
                    _ => false,
                }
            }
            fn matrix_input(value: &ScalarExpr) -> bool {
                match value {
                    ScalarExpr::Cast { dtype: 1, value } => matrix_input(value),
                    ScalarExpr::StorageRound { dtype: 10, value } => {
                        matches!(value.as_ref(), ScalarExpr::Load { .. })
                    }
                    _ => false,
                }
            }
            if !square
                && (!image_operand(a)
                    || !image_operand(b)
                    || !(convolution_input(a) || matrix_input(a) && matrix_input(b)))
            {
                self.reduction = None;
                return Err(
                    "CPU product reduction scheduling requires a direct contiguous float32 row square or supported IMAGE convolution or matrix storage".into(),
                );
            }
            let a = self.expression(a)?;
            let b = self.expression(b)?;
            let contracted =
                format!("{accumulator}=__builtin_fmaf((float)({a}),(float)({b}),{accumulator});");
            if square && (count <= 32 || count.is_multiple_of(4)) {
                // The pinned UNROLL/SLP schedule packs even groups of squares
                // before its scalar horizontal sum. Those products round before
                // addition. A full-unroll odd scalar tail remains contracted;
                // non-divisible looped reductions keep their scalar FMAs.
                let separate = format!("{accumulator}={accumulator}+((float)({a})*(float)({b}));");
                if count <= 32 && count % 2 == 1 {
                    format!(
                        "if({index}=={}L){{{contracted}}}else{{{separate}}}",
                        count - 1
                    )
                } else {
                    separate
                }
            } else {
                contracted
            }
        } else {
            let value = self.expression(value)?;
            format!("{accumulator}={accumulator}+({value});")
        };
        self.reduction = None;
        // Derive the identity fold from original IMAGE reduction strides. A
        // four-aligned channel count alone does not imply a fully expanded sum:
        // cin_last weights retain [block16,lane1] and a channel-block loop.
        fn expanded_image_reduction(
            value: &ScalarExpr,
            graph: &LoweredGraph,
            count: usize,
        ) -> bool {
            match value {
                ScalarExpr::Cast { dtype: 1, value } => {
                    expanded_image_reduction(value, graph, count)
                }
                ScalarExpr::StorageRound { dtype: 10, value } => match value.as_ref() {
                    ScalarExpr::PaddedLoad { plan, .. } => {
                        let shape = &graph.plans[*plan].ty.shape;
                        if shape.len() != 4 || !shape[1].is_multiple_of(4) || count != shape[1] {
                            return false;
                        }
                        let input_strides = [4usize, 1];
                        let weight_strides = if shape[2..].iter().product::<usize>() == 1 {
                            [16usize, 1]
                        } else {
                            [16usize, 4]
                        };
                        let merges = input_strides[0] == 4 * input_strides[1]
                            && weight_strides[0] == 4 * weight_strides[1];
                        // A singleton block axis disappears before unrolling.
                        (merges || shape[1] / 4 == 1) && count <= 32
                    }
                    _ => false,
                },
                _ => false,
            }
        }
        let first = (product.is_none()
            || square
            || product.is_some_and(|(a, _)| expanded_image_reduction(a, self.graph, count)))
            && count <= 32;
        let statement = if first {
            self.reduction = Some(("0L".into(), 1));
            let initial = self.expression(value)?;
            self.reduction = None;
            format!("float {accumulator}={initial}; for(long {index}=1;{index}<{count}L;{index}++){{{update}}} ")
        } else {
            format!("float {accumulator}=0.0f; for(long {index}=0;{index}<{count}L;{index}++){{{update}}} ")
        };
        self.statements.push(statement);
        Ok(accumulator)
    }
}
