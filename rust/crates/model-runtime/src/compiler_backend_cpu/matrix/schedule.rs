//! Affine matrix specialization of original hand_coded_optimizations.
use super::MatrixLayout;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Loop,
    Thread,
    Upcast,
    Reduce,
    Unroll,
}
#[derive(Clone, Debug)]
pub(super) struct Axis {
    pub size: usize,
    pub kind: Kind,
    pub id: usize,
    /// Weight, input, and output strides, in their respective scalar elements.
    pub strides: Vec<usize>,
    pub padding_stride: usize,
}
#[derive(Debug)]
pub struct MatrixSchedule {
    pub before: Vec<usize>,
    pub after: Vec<usize>,
    pub opts: Vec<String>,
    pub workers: usize,
    pub(super) axes: Vec<Axis>,
    next_id: usize,
}
impl MatrixSchedule {
    pub(super) fn new(layout: &MatrixLayout, max_workers: usize) -> Result<Self, String> {
        Self::with_residual(layout, max_workers, false)
    }
    pub(super) fn with_residual(
        layout: &MatrixLayout,
        max_workers: usize,
        residual: bool,
    ) -> Result<Self, String> {
        Self::profile(layout, max_workers, residual, false, false)
    }
    pub(super) fn biased(
        layout: &MatrixLayout,
        max_workers: usize,
        residual: bool,
        relu: bool,
    ) -> Result<Self, String> {
        Self::profile(layout, max_workers, residual, true, relu)
    }
    fn profile(
        layout: &MatrixLayout,
        max_workers: usize,
        residual: bool,
        biased: bool,
        relu: bool,
    ) -> Result<Self, String> {
        if max_workers == 0 || max_workers > 32 {
            return Err("matrix worker maximum must be explicitly1..32".into());
        }
        let n = layout.output_channels;
        let mut axes = vec![];
        let mut push = |size, kind, strides: [usize; 3], id| {
            if size > 1 {
                axes.push(Axis {
                    size,
                    kind,
                    id,
                    strides: strides.to_vec(),
                    padding_stride: 0,
                });
            }
        };
        // rangeify numbers the nontrivial channel reductions first. Adjacent
        // affine merges retain the first range ID and leave the second ID
        // unused; those gaps still affect the source DAG's tuple ordering.
        let mut output_id = if layout.input_channels / 4 > 1 { 2 } else { 1 };
        push(
            layout.batches * layout.rows,
            Kind::Loop,
            [0, layout.input_row_stride, layout.output_pixel_stride],
            output_id,
        );
        output_id += usize::from(layout.batches > 1) + usize::from(layout.rows > 1);
        push(
            layout.output_pixel_stride / n,
            Kind::Loop,
            [0, 0, n],
            output_id,
        );
        output_id += usize::from(layout.output_pixel_stride / n > 1);
        push(
            n / 4,
            Kind::Loop,
            [layout.weight_output_stride, 0, 4],
            output_id,
        );
        output_id += usize::from(n / 4 > 1);
        push(
            4,
            Kind::Loop,
            [if layout.cin_last { 4 } else { 1 }, 0, 1],
            output_id,
        );
        if layout.cin_last {
            push(layout.input_channels / 4, Kind::Reduce, [16, 4, 0], 0);
            push(
                4,
                Kind::Reduce,
                [1, 1, 0],
                usize::from(layout.input_channels / 4 > 1),
            );
        } else {
            push(layout.input_channels, Kind::Reduce, [4, 1, 0], 0);
        }
        if residual {
            // Optimizer buffer entries are affine accesses, including a second
            // access to the packed input at output coordinates.
            for axis in &mut axes {
                axis.strides.push(if axis.kind == Kind::Reduce {
                    0
                } else {
                    axis.strides[2] / layout.output_channels * layout.input_row_stride
                        + axis.strides[2] % layout.output_channels
                });
            }
        }
        if biased {
            for axis in &mut axes {
                // Bias and skip are distinct affine access entries even when
                // their index maps agree. They participate in invariance.
                axis.strides.push(if axis.kind == Kind::Reduce {
                    0
                } else {
                    axis.strides[2] % layout.output_channels
                });
            }
        }
        let mut result = Self {
            before: axes.iter().map(|a| a.size).collect(),
            after: vec![],
            opts: vec![],
            workers: 1,
            next_id: axes.iter().map(|a| a.id).max().unwrap() + 1,
            axes,
        };
        // Output width padding gates input values through WHERE. The source
        // heuristic fully expands small masked output axes before its affine
        // choices. The only such axis in image_dot is padded width.
        let padding = layout.output_pixel_stride / n;
        if padding > 1 {
            result.axes[usize::from(layout.batches * layout.rows > 1)].padding_stride = 1;
        }
        if padding > 1 && padding <= 7 {
            let axis = usize::from(layout.batches * layout.rows > 1);
            result.split(axis, 0, Kind::Upcast, "UPCAST", axis);
        }
        if relu {
            // Original data-dependent WHERE(0 < value) marks all its output
            // ranges as masked. Fully expand small masked axes in reverse.
            let mut masked = vec![];
            let mut expanded = 1;
            for (axis, a) in result
                .axes
                .iter()
                .enumerate()
                .filter(|(_, a)| a.kind == Kind::Loop)
            {
                if a.size <= 7 && expanded * a.size <= 49 {
                    masked.push(axis);
                    expanded *= a.size;
                }
            }
            for axis in masked.into_iter().rev() {
                result.split(axis, 0, Kind::Upcast, "UPCAST", axis);
            }
        }
        // The baseline LLVM target has no IMAGE_PITCH_ALIGNMENT, local groups
        // or tensor cores. Its remaining output-upcast choice is affine.
        let mut upcasted = std::collections::BTreeSet::new();
        while result.product(Kind::Loop) >= 1024 && result.upcast_size() < 32 {
            let mut choices = vec![];
            for (axis, a) in result
                .axes
                .iter()
                .enumerate()
                .filter(|(_, a)| a.kind == Kind::Loop)
            {
                for factor in [3, 4] {
                    if upcasted.contains(&axis) || !a.size.is_multiple_of(factor) {
                        continue;
                    }
                    let invariant = (0..a.strides.len()).any(|b| {
                        a.strides[b] == 0
                            && result
                                .axes
                                .iter()
                                .filter(|r| matches!(r.kind, Kind::Upcast | Kind::Unroll))
                                .all(|r| r.strides[b] != 0)
                    });
                    if invariant {
                        choices.push((
                            a.strides.iter().filter(|s| **s != 0).count(),
                            a.strides.iter().sum::<usize>(),
                            axis,
                            factor,
                        ));
                    }
                }
            }
            let Some((_, _, axis, factor)) = choices.into_iter().min() else {
                break;
            };
            result.split(axis, factor, Kind::Upcast, "UPCAST", axis);
            upcasted.insert(axis);
        }
        let reduces = result.of_kind(Kind::Reduce);
        if let Some(&axis) = reduces.last() {
            if (result.upcast_size() <= 4 || result.of_kind(Kind::Unroll).is_empty())
                && result.upcast_size() < 64
            {
                let size = result.axes[axis].size;
                if size <= 32 {
                    result.split(axis, 0, Kind::Unroll, "UNROLL", reduces.len() - 1);
                    if size <= 3 {
                        let reduces = result.of_kind(Kind::Reduce);
                        if let Some(&axis) = reduces.last() {
                            if result.axes[axis].size <= 3 {
                                result.split(axis, 0, Kind::Unroll, "UNROLL", reduces.len() - 1);
                            }
                        }
                    }
                } else if size.is_multiple_of(4) {
                    result.split(axis, 4, Kind::Unroll, "UNROLL", reduces.len() - 1);
                }
            }
        }
        if result.upcast_size() == 1 {
            if let Some(&axis) = result.of_kind(Kind::Loop).last() {
                if result.axes[axis].size.is_multiple_of(4) {
                    result.split(axis, 4, Kind::Upcast, "UPCAST", axis);
                }
            }
        }
        let work = result.axes.iter().map(|a| a.size).product::<usize>();
        for workers in [32, 16, 12, 8, 6, 5, 4, 3, 2] {
            if workers > max_workers || work / (128 << 10) < workers {
                continue;
            }
            if let Some(axis) = result
                .axes
                .iter()
                .position(|a| a.kind == Kind::Loop && a.size.is_multiple_of(workers))
            {
                result.split(axis, workers, Kind::Thread, "THREAD", axis);
                result.workers = workers;
                break;
            }
        }
        result.after = result.axes.iter().map(|a| a.size).collect();
        Ok(result)
    }
    fn product(&self, kind: Kind) -> usize {
        self.axes
            .iter()
            .filter(|a| a.kind == kind)
            .map(|a| a.size)
            .product()
    }
    fn upcast_size(&self) -> usize {
        self.axes
            .iter()
            .filter(|a| matches!(a.kind, Kind::Upcast | Kind::Unroll))
            .map(|a| a.size)
            .product()
    }
    fn of_kind(&self, kind: Kind) -> Vec<usize> {
        self.axes
            .iter()
            .enumerate()
            .filter(|(_, a)| a.kind == kind)
            .map(|(i, _)| i)
            .collect()
    }
    fn split(&mut self, axis: usize, argument: usize, kind: Kind, name: &str, opt_axis: usize) {
        let old = self.axes[axis].clone();
        let factor = if argument == 0 { old.size } else { argument };
        let remaining = old.size / factor;
        self.opts.push(format!(
            "Opt(op=OptOps.{name}, axis={opt_axis}, arg={argument})"
        ));
        self.axes[axis].size = remaining;
        let mut new = Axis {
            size: factor,
            kind,
            id: self.next_id,
            strides: old.strides.clone(),
            padding_stride: old.padding_stride,
        };
        self.next_id += 1;
        if kind == Kind::Thread {
            new.strides = old.strides.iter().map(|s| s * remaining).collect();
            new.padding_stride *= remaining;
        } else {
            self.axes[axis].strides = old.strides.iter().map(|s| s * factor).collect();
            self.axes[axis].padding_stride *= factor;
        }
        self.axes.push(new);
        self.axes.retain(|a| a.size > 1);
        self.axes.sort_by_key(|a| (a.kind, a.id));
    }
}
