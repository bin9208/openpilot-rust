use super::{
    schedule::{Axis, Kind},
    MatrixLayout,
};
use crate::compiler_backend_cpu::image::Ir;
use std::{collections::BTreeMap, fmt::Write};

pub struct MatrixArtifact<const N: usize = 3> {
    pub source: String,
    pub bytes: [usize; N],
    pub scalars: usize,
    pub workers: usize,
}
#[derive(Clone, Copy)]
enum Epilogue {
    Plain,
    InputResidual,
    BiasedResidual { relu_half: bool },
    BiasedRelu,
    BiasedF32,
    BiasedSilu,
    PackedSilu { row_stride: usize, elements: usize },
}
impl Epilogue {
    fn biased(self) -> bool {
        matches!(
            self,
            Self::BiasedResidual { .. }
                | Self::BiasedRelu
                | Self::BiasedF32
                | Self::BiasedSilu
                | Self::PackedSilu { .. }
        )
    }
    fn relu_half(self) -> bool {
        matches!(
            self,
            Self::BiasedResidual { relu_half: true } | Self::BiasedRelu
        )
    }
    fn half_output(self) -> bool {
        self.relu_half() || matches!(self, Self::PackedSilu { .. })
    }
    fn separate_residual(self) -> bool {
        matches!(self, Self::BiasedResidual { .. })
    }
    fn bias_only(self) -> bool {
        matches!(
            self,
            Self::BiasedRelu | Self::BiasedF32 | Self::BiasedSilu | Self::PackedSilu { .. }
        )
    }
}
pub(super) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || i > 0 && b.is_ascii_digit())
}
fn expanded(axes: &[Axis], kind: Kind) -> Vec<BTreeMap<usize, String>> {
    let mut result = vec![BTreeMap::new()];
    for a in axes.iter().filter(|a| a.kind == kind) {
        result = result
            .into_iter()
            .flat_map(|v| {
                (0..a.size).map(move |n| {
                    let mut v = v.clone();
                    v.insert(a.id, n.to_string());
                    v
                })
            })
            .collect();
    }
    result
}
fn index(ir: &mut Ir, axes: &[Axis], values: &BTreeMap<usize, String>, buffer: usize) -> String {
    let mut value = "0".to_owned();
    for a in axes {
        if a.strides[buffer] == 0 {
            continue;
        }
        let term = ir.op(format!(
            "mul i32 {}, {}",
            values.get(&a.id).unwrap(),
            a.strides[buffer]
        ));
        value = ir.op(format!("add nsw i32 {term}, {value}"));
    }
    value
}
fn padding(ir: &mut Ir, axes: &[Axis], values: &BTreeMap<usize, String>) -> String {
    let mut value = "0".to_owned();
    for a in axes.iter().filter(|a| a.padding_stride != 0) {
        let term = ir.op(format!("mul i32 {}, {}", values[&a.id], a.padding_stride));
        value = ir.op(format!("add nsw i32 {term}, {value}"));
    }
    value
}
fn load(
    ir: &mut Ir,
    axes: &[Axis],
    values: &BTreeMap<usize, String>,
    buffer: usize,
    groups: &mut BTreeMap<(usize, String), Vec<String>>,
    biased: bool,
) -> String {
    let name = match buffer {
        0 => "%weights",
        1 => "%input",
        3 if biased => "%bias",
        4 => "%skip",
        _ => "%input",
    };
    // Original devectorization forms float4 STACK/GEP groups only when an
    // expanded axis supplies four adjacent scalar addresses. Other expanded
    // coordinates remain invariant across the group.
    if let Some(axis) = axes.iter().find(|a| {
        matches!(a.kind, Kind::Upcast | Kind::Unroll)
            && a.strides[buffer] == 1
            && a.size.is_multiple_of(4)
    }) {
        let lane = values[&axis.id].parse::<usize>().unwrap();
        let mut base = values.clone();
        base.insert(axis.id, (lane / 4 * 4).to_string());
        let offset = index(ir, axes, &base, buffer);
        let values = groups
            .entry((buffer, offset.clone()))
            .or_insert_with(|| ir.half_values(name, &offset, 4, 1, true));
        values[lane % 4].clone()
    } else {
        let offset = index(ir, axes, values, buffer);
        ir.load_half(name, &offset)
    }
}
impl MatrixLayout {
    pub fn artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact, String> {
        self.render_artifact(function, entrypoint, max_workers, Epilogue::Plain)
    }
    /// Fuse a residual read through the original packed input alias. The
    /// final output is dense; no padded matrix intermediate is materialized.
    pub fn residual_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact, String> {
        if self.input_channels != self.output_channels {
            return Err("matrix residual requires identical logical input/output shapes".into());
        }
        let mut dense = self.clone();
        dense.output_pixel_stride = dense.output_channels;
        dense.output_elements = dense.batches * dense.rows * dense.output_channels;
        dense.render_artifact(function, entrypoint, max_workers, Epilogue::InputResidual)
    }
    /// Independent bias and packed residual, with an optional fused Relu and
    /// half materialization. Single-row source profile; ONNX selection is separate.
    pub fn biased_residual_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
        relu_half: bool,
    ) -> Result<MatrixArtifact<5>, String> {
        if self.batches != 1 || self.rows != 1 {
            return Err("biased matrix epilogue requires single-row profile".into());
        }
        let mut dense = self.clone();
        dense.output_pixel_stride = dense.output_channels;
        dense.output_elements = dense.output_channels;
        dense.render_artifact(
            function,
            entrypoint,
            max_workers,
            Epilogue::BiasedResidual { relu_half },
        )
    }
    /// Gemm bias and Relu fused into original packed-half materialization.
    /// Padding is capacity only; valid channels are the producer's stores.
    pub fn biased_relu_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact<4>, String> {
        if self.batches != 1 || self.rows != 1 {
            return Err("biased matrix producer requires single-row profile".into());
        }
        let mut dense = self.clone();
        dense.output_pixel_stride = dense.output_channels;
        dense.output_elements = dense.output_channels;
        dense.render_artifact(function, entrypoint, max_workers, Epilogue::BiasedRelu)
    }
    /// Source terminal Gemm+bias with dense f32 output. Bias participates in
    /// the affine scheduling and arithmetic DAG, not a later C epilogue.
    pub fn biased_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact<4>, String> {
        if self.batches != 1 || self.rows != 1 {
            return Err("biased matrix terminal requires single-row profile".into());
        }
        self.broadcast_bias_artifact(function, entrypoint, max_workers)
    }
    /// Final-f32 MatMul plus a channel bias broadcast across every batch/row.
    /// The bias is an affine source operand, never a separately rounded result.
    pub fn broadcast_bias_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact<4>, String> {
        let mut dense = self.clone();
        dense.output_pixel_stride = dense.output_channels;
        dense.output_elements = dense.batches * dense.rows * dense.output_channels;
        dense.render_artifact(function, entrypoint, max_workers, Epilogue::BiasedF32)
    }
    /// Final-only MatMul/bias/SiLU without an intermediate storage barrier.
    /// The affine accesses equal the bias profile; only the arithmetic DAG
    /// changes, retaining original xexp2 decomposition and contract flags.
    pub fn broadcast_silu_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact<4>, String> {
        let mut dense = self.clone();
        dense.output_pixel_stride = dense.output_channels;
        dense.output_elements = dense.batches * dense.rows * dense.output_channels;
        dense.render_artifact(function, entrypoint, max_workers, Epilogue::BiasedSilu)
    }
    /// Internal matrix/bias/SiLU fused directly into the next matrix's half
    /// input layout. Logical channels retain their source coordinates; row
    /// padding is capacity and receives no producer stores.
    pub fn packed_silu_artifact(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
    ) -> Result<MatrixArtifact<4>, String> {
        let packed = MatrixLayout::new(
            &[self.batches, self.rows, self.output_channels],
            &[self.output_channels, 4],
        )?;
        let mut logical = self.clone();
        logical.output_pixel_stride = logical.output_channels;
        logical.output_elements = logical.batches * logical.rows * logical.output_channels;
        logical.render_artifact(
            function,
            entrypoint,
            max_workers,
            Epilogue::PackedSilu {
                row_stride: packed.input_row_stride,
                elements: packed.input_elements,
            },
        )
    }
    fn render_artifact<const N: usize>(
        &self,
        function: &str,
        entrypoint: &str,
        max_workers: usize,
        epilogue: Epilogue,
    ) -> Result<MatrixArtifact<N>, String> {
        if !identifier(function) || !identifier(entrypoint) || function == entrypoint {
            return Err("invalid matrix LLVM identifiers".into());
        }
        let biased = epilogue.biased();
        let residual = matches!(epilogue, Epilogue::InputResidual);
        let schedule = if let Epilogue::PackedSilu { row_stride, .. } = epilogue {
            super::schedule::MatrixSchedule::biased_packed(self, max_workers, row_stride)?
        } else if biased {
            super::schedule::MatrixSchedule::biased(
                self,
                max_workers,
                epilogue.separate_residual(),
                epilogue.relu_half(),
            )?
        } else {
            super::schedule::MatrixSchedule::with_residual(self, max_workers, residual)?
        };
        let axes = &schedule.axes;
        let outputs = expanded(axes, Kind::Upcast);
        let reductions = expanded(axes, Kind::Unroll);
        let loops = axes
            .iter()
            .filter(|a| a.kind == Kind::Loop)
            .collect::<Vec<_>>();
        let reduce_loops = axes
            .iter()
            .filter(|a| a.kind == Kind::Reduce)
            .collect::<Vec<_>>();
        let mut ir = Ir::default();
        let output_ty = if epilogue.half_output() {
            "i16"
        } else {
            "float"
        };
        if epilogue.bias_only() {
            ir.parameters(&[
                ("%out", 6, 0),
                ("%input", 6, 1),
                ("%weights", 6, 2),
                ("%bias", 6, 3),
                ("%core", 6, 4),
            ]);
            writeln!(ir.text,"define void @{function}({output_ty}* noalias align 32 %out, i16* noalias align 32 %input, i16* noalias align 32 %weights, i16* noalias align 32 %bias, i32 %core) #0 {{").unwrap();
        } else if biased {
            ir.parameters(&[
                ("%out", 6, 0),
                ("%skip", 6, 1),
                ("%input", 6, 2),
                ("%weights", 6, 3),
                ("%bias", 6, 4),
                ("%core", 6, 5),
            ]);
            writeln!(ir.text,"define void @{function}({output_ty}* noalias align 32 %out, i16* noalias align 32 %skip, i16* noalias align 32 %input, i16* noalias align 32 %weights, i16* noalias align 32 %bias, i32 %core) #0 {{").unwrap();
        } else {
            writeln!(ir.text,"define void @{function}(float* noalias align 32 %out, i16* noalias align 32 %input, i16* noalias align 32 %weights, i32 %core) #0 {{").unwrap();
        }
        if !reduce_loops.is_empty() {
            ir.line(format!("%acc = alloca [{0} x float]", outputs.len()));
        }
        let mut base = BTreeMap::new();
        for a in axes.iter().filter(|a| a.kind == Kind::Thread) {
            base.insert(a.id, "%core".into());
        }
        for a in &loops {
            let value =
                ir.begin_indexed_loop(&format!("matrix_{}", a.id), a.size, vec![a.id as i64, 0]);
            base.insert(a.id, value);
        }
        let mut acc = vec![None; outputs.len()];
        if !reduce_loops.is_empty() {
            for slot in 0..outputs.len() {
                let ptr = ir.op(format!(
                    "getelementptr inbounds float, float* %acc, i32 {slot}"
                ));
                ir.store_float(&ptr, "0.0");
            }
        }
        for a in &reduce_loops {
            let value = ir.begin_indexed_loop(
                &format!("matrix_reduce_{}", a.id),
                a.size,
                vec![a.id as i64, 1],
            );
            base.insert(a.id, value);
        }
        if !reduce_loops.is_empty() {
            for (slot, value) in acc.iter_mut().enumerate() {
                let ptr = ir.op(format!(
                    "getelementptr inbounds float, float* %acc, i32 {slot}"
                ));
                *value = Some(ir.op(format!("load float, float* {ptr}")));
            }
        }
        // Expand reductions outside independent outputs, preserving the source
        // REDUCE chain and sharing invariant loads through the semantic DAG.
        let mut groups = BTreeMap::new();
        for reduction in &reductions {
            for (slot, output) in outputs.iter().enumerate() {
                let mut values = base.clone();
                values.extend(output.clone());
                values.extend(reduction.clone());
                let mut x = load(&mut ir, axes, &values, 1, &mut groups, biased);
                if self.output_pixel_stride != self.output_channels {
                    let pad = padding(&mut ir, axes, &values);
                    let valid = ir.op(format!("icmp eq i32 {pad}, 0"));
                    x = ir.op(format!("select i1 {valid}, float {x}, float 0.0"));
                }
                let w = load(&mut ir, axes, &values, 0, &mut groups, biased);
                let product = ir.op(format!("fmul nsz arcp contract afn float {x}, {w}"));
                acc[slot] = Some(match acc[slot].take() {
                    Some(previous) => ir.op(format!(
                        "fadd nsz arcp contract afn float {previous}, {product}"
                    )),
                    None => product,
                });
            }
        }
        if !reduce_loops.is_empty() {
            for (slot, value) in acc.iter().enumerate() {
                let ptr = ir.op(format!(
                    "getelementptr inbounds float, float* %acc, i32 {slot}"
                ));
                ir.store_float(&ptr, value.as_ref().unwrap());
            }
            for a in reduce_loops.iter().rev() {
                ir.end_loop(&format!("matrix_reduce_{}", a.id));
            }
            for (slot, value) in acc.iter_mut().enumerate() {
                let ptr = ir.op(format!(
                    "getelementptr inbounds float, float* %acc, i32 {slot}"
                ));
                *value = Some(ir.op(format!("load float, float* {ptr}")));
            }
        }
        if residual {
            for (slot, output) in outputs.iter().enumerate() {
                let mut values = base.clone();
                values.extend(output.clone());
                for axis in axes
                    .iter()
                    .filter(|a| matches!(a.kind, Kind::Reduce | Kind::Unroll))
                {
                    values.insert(axis.id, "0".into());
                }
                let skip = load(&mut ir, axes, &values, 3, &mut groups, false);
                acc[slot] = Some(ir.op(format!(
                    "fadd nsz arcp contract afn float {}, {skip}",
                    acc[slot].as_ref().unwrap()
                )));
            }
        }
        if biased {
            for (slot, output) in outputs.iter().enumerate() {
                let mut values = base.clone();
                values.extend(output.clone());
                for axis in axes
                    .iter()
                    .filter(|a| matches!(a.kind, Kind::Reduce | Kind::Unroll))
                {
                    values.insert(axis.id, "0".into());
                }
                let bias = load(&mut ir, axes, &values, 3, &mut groups, true);
                let value = ir.op(format!(
                    "fadd nsz arcp contract afn float {}, {bias}",
                    acc[slot].as_ref().unwrap()
                ));
                acc[slot] = Some(if epilogue.separate_residual() {
                    let skip = load(&mut ir, axes, &values, 4, &mut groups, true);
                    ir.op(format!("fadd nsz arcp contract afn float {skip}, {value}"))
                } else {
                    value
                });
            }
        }
        if matches!(epilogue, Epilogue::BiasedSilu | Epilogue::PackedSilu { .. }) {
            for value in &mut acc {
                *value = Some(ir.matrix_silu(value.as_ref().unwrap()));
            }
        }
        if epilogue.half_output() {
            for (slot, output) in outputs.iter().enumerate() {
                let mut values = base.clone();
                values.extend(output.clone());
                for axis in axes
                    .iter()
                    .filter(|a| matches!(a.kind, Kind::Reduce | Kind::Unroll))
                {
                    values.insert(axis.id, "0".into());
                }
                let offset = index(&mut ir, axes, &values, 2);
                let pointer = ir.op(format!(
                    "getelementptr inbounds i16, i16* %out, i32 {offset}"
                ));
                let value = if epilogue.relu_half() {
                    ir.relu_half(acc[slot].as_ref().unwrap())
                } else {
                    ir.plain_half(acc[slot].as_ref().unwrap())
                };
                ir.store_i16(&pointer, &value);
            }
        } else {
            // Original late symbolic simplification removes the entire product
            // tree for a statically false padding lane and stores literal zero.
            // Retaining a separate zero-multiply reduction changes LLVM's DAG
            // (including NaN operand ownership in neighboring valid lanes).
            let stored_value = |slot: usize| {
                let padding =
                    axes.iter()
                        .filter(|a| a.padding_stride != 0)
                        .try_fold(0usize, |sum, a| {
                            let coordinate = outputs[slot].get(&a.id)?.parse::<usize>().ok()?;
                            sum.checked_add(coordinate.checked_mul(a.padding_stride)?)
                        });
                if padding.is_some_and(|p| p != 0) {
                    "0.0".to_owned()
                } else {
                    acc[slot].as_ref().unwrap().clone()
                }
            };
            let vector_axis = axes
                .iter()
                .find(|a| a.kind == Kind::Upcast && a.strides[2] == 1 && a.size.is_multiple_of(4));
            let mut output_groups = BTreeMap::<Vec<(usize, String)>, Vec<(usize, usize)>>::new();
            for (slot, output) in outputs.iter().enumerate() {
                let mut key = output
                    .iter()
                    .map(|(k, v)| (*k, v.clone()))
                    .collect::<Vec<_>>();
                let lane = if let Some(axis) = vector_axis {
                    let n = output[&axis.id].parse::<usize>().unwrap();
                    key.iter_mut().find(|(id, _)| *id == axis.id).unwrap().1 =
                        (n / 4 * 4).to_string();
                    n % 4
                } else {
                    0
                };
                output_groups.entry(key).or_default().push((lane, slot));
            }
            for group in output_groups.values_mut() {
                group.sort_by_key(|v| v.0);
                let slot = group[0].1;
                let output = &outputs[slot];
                let mut values = base.clone();
                values.extend(output.clone());
                // Reduction coordinates do not contribute to the output address.
                for a in axes
                    .iter()
                    .filter(|a| matches!(a.kind, Kind::Reduce | Kind::Unroll))
                {
                    values.insert(a.id, "0".into());
                }
                let oi = index(&mut ir, axes, &values, 2);
                let ptr = ir.op(format!(
                    "getelementptr inbounds float, float* %out, i32 {oi}"
                ));
                if vector_axis.is_some() {
                    let values = group
                        .iter()
                        .map(|(_, slot)| stored_value(*slot))
                        .collect::<Vec<_>>();
                    ir.store_float4(&ptr, &values);
                } else {
                    ir.store_float(&ptr, &stored_value(slot));
                }
            }
        }
        for a in loops.iter().rev() {
            ir.end_loop(&format!("matrix_{}", a.id));
        }
        ir.line("ret void");
        // Keep the independently compiled original terminal core boundary;
        // indirect-wrapper inlining can change LLVM NaN operand ownership.
        let boundary = if matches!(
            epilogue,
            Epilogue::BiasedF32 | Epilogue::BiasedSilu | Epilogue::PackedSilu { .. }
        ) {
            "noinline"
        } else {
            "alwaysinline"
        };
        writeln!(ir.text,"}}\nattributes #0 = {{ {boundary} nounwind \"no-builtins\" \"no-trapping-math\"=\"true\" }}").unwrap();
        writeln!(
            ir.text,
            "define void @{entrypoint}(i8** %buffers, i32* %scalars) {{"
        )
        .unwrap();
        let types = if epilogue.bias_only() {
            vec![output_ty, "i16", "i16", "i16"]
        } else if biased {
            vec![output_ty, "i16", "i16", "i16", "i16"]
        } else {
            vec!["float", "i16", "i16"]
        };
        for (slot, ty) in types.into_iter().enumerate() {
            writeln!(ir.text,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %ptr{slot} = bitcast i8* %raw{slot} to {ty}*").unwrap();
        }
        if epilogue.bias_only() {
            writeln!(ir.text,"  %core_id = load i32, i32* %scalars\n  call void @{function}({output_ty}* %ptr0, i16* %ptr1, i16* %ptr2, i16* %ptr3, i32 %core_id)\n  ret void\n}}\n").unwrap();
        } else if biased {
            writeln!(ir.text,"  %core_id = load i32, i32* %scalars\n  call void @{function}({output_ty}* %ptr0, i16* %ptr1, i16* %ptr2, i16* %ptr3, i16* %ptr4, i32 %core_id)\n  ret void\n}}\n").unwrap();
        } else {
            writeln!(ir.text,"  %core_id = load i32, i32* %scalars\n  call void @{function}(float* %ptr0, i16* %ptr1, i16* %ptr2, i32 %core_id)\n  ret void\n}}\n").unwrap();
        }
        let bytes = if biased {
            let half = epilogue.relu_half();
            let skip = MatrixLayout::new(&[1, self.output_channels], &[self.output_channels, 4])?
                .input_elements
                * 2;
            let output_bytes = if let Epilogue::PackedSilu { elements, .. } = epilogue {
                elements * 2
            } else if half {
                skip
            } else {
                self.output_elements * 4
            };
            let mut sizes = vec![output_bytes];
            if epilogue.separate_residual() {
                sizes.push(skip);
            }
            sizes.extend([
                self.input_elements * 2,
                self.weight_elements * 2,
                self.output_channels * 2,
            ]);
            sizes
        } else {
            vec![
                self.output_elements * 4,
                self.input_elements * 2,
                self.weight_elements * 2,
            ]
        };
        Ok(MatrixArtifact {
            source: ir.text,
            bytes: bytes
                .try_into()
                .map_err(|_| "matrix typed argument count mismatch")?,
            scalars: 1,
            workers: schedule.workers,
        })
    }
}
