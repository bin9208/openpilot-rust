use super::*;
impl Renderer<'_> {
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
            if !image_operand(a) || !image_operand(b) || !convolution_input(a) {
                self.reduction = None;
                return Err(
                    "CPU product reduction scheduling not implemented outside IMAGE convolution storage".into(),
                );
            }
            let a = self.expression(a)?;
            let b = self.expression(b)?;
            format!("{accumulator}=__builtin_fmaf((float)({a}),(float)({b}),{accumulator});")
        } else {
            let value = self.expression(value)?;
            format!("{accumulator}={accumulator}+({value});")
        };
        self.reduction = None;
        // Original <=32 full-unroll reductions start with their first value;
        // looped or IMAGE product reductions use an explicit zero accumulator.
        let first = product.is_none() && count <= 32;
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
