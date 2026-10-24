use super::{render::identifier, MatrixLayout};
use crate::compiler_backend_cpu::image::Ir;
use std::fmt::Write;

pub struct MatrixBoundary {
    pub source: String,
    pub bytes: [usize; 2],
    pub scalars: usize,
    pub workers: usize,
}
fn preamble(entry: &str, output: &str, input: &str) -> Result<Ir, String> {
    if !identifier(entry) {
        return Err("invalid matrix boundary identifier".into());
    }
    let mut ir = Ir::default();
    writeln!(
        ir.text,
        "define void @{entry}(i8** %buffers, i32* %scalars) {{"
    )
    .unwrap();
    for (slot, (name, ty)) in [("out", output), ("input", input)].into_iter().enumerate() {
        writeln!(ir.text,"  %slot{slot} = getelementptr inbounds i8*, i8** %buffers, i32 {slot}\n  %raw{slot} = load i8*, i8** %slot{slot}\n  %{name} = bitcast i8* %raw{slot} to {ty}*").unwrap();
    }
    Ok(ir)
}
impl MatrixLayout {
    /// Decode the valid channels of a single-row packed-half epilogue into
    /// logical f32 storage. It introduces no arithmetic or second half store.
    pub fn packed_half_output_artifact(&self, entry: &str) -> Result<MatrixBoundary, String> {
        if self.batches != 1 || self.rows != 1 {
            return Err("packed-half output boundary requires single-row profile".into());
        }
        let mut ir = preamble(entry, "float", "i16")?;
        let channel = ir.begin_indexed_loop("half_channel", self.output_channels, vec![0, 0]);
        let value = ir.load_half("%input", &channel);
        let pointer = ir.op(format!(
            "getelementptr inbounds float, float* %out, i32 {channel}"
        ));
        ir.store_float(&pointer, &value);
        ir.end_loop("half_channel");
        ir.line("ret void");
        ir.text.push_str("}\n");
        let packed = MatrixLayout::new(&[1, self.output_channels], &[self.output_channels, 4])?;
        Ok(MatrixBoundary {
            source: ir.text,
            bytes: [self.output_channels * 4, packed.input_elements * 2],
            workers: 1,
            scalars: 0,
        })
    }
    /// Separate once-only materialization call. The reduction artifact never
    /// writes this scratch buffer, including when invoked for multiple cores.
    pub fn input_artifact(&self, entry: &str) -> Result<MatrixBoundary, String> {
        let mut ir = preamble(entry, "i16", "float")?;
        let row = ir.begin_indexed_loop("pack_row", self.batches * self.rows, vec![0, 0]);
        let channel = ir.begin_indexed_loop("pack_channel", self.input_channels, vec![1, 0]);
        let from = ir.op(format!("mul i32 {row}, {}", self.input_channels));
        let from = ir.op(format!("add nsw i32 {from}, {channel}"));
        let ptr = ir.op(format!(
            "getelementptr inbounds float, float* %input, i32 {from}"
        ));
        let value = ir.op(format!("load float, float* {ptr}"));
        let half = ir.store_half(&value);
        let to = ir.op(format!("mul i32 {row}, {}", self.input_row_stride));
        let to = ir.op(format!("add nsw i32 {to}, {channel}"));
        let ptr = ir.op(format!("getelementptr inbounds i16, i16* %out, i32 {to}"));
        ir.line(format!("store i16 {half}, i16* {ptr}"));
        ir.end_loop("pack_channel");
        ir.end_loop("pack_row");
        ir.line("ret void");
        ir.text.push_str("}\n");
        Ok(MatrixBoundary {
            source: ir.text,
            bytes: [
                self.input_elements * 2,
                self.batches * self.rows * self.input_channels * 4,
            ],
            scalars: 0,
            workers: 1,
        })
    }
    /// Preserve every f32 payload bit while undoing only the source padded
    /// output width. This is a separate once-only call after all core IDs.
    pub fn output_artifact(&self, entry: &str) -> Result<MatrixBoundary, String> {
        let mut ir = preamble(entry, "i32", "i32")?;
        let row = ir.begin_indexed_loop("undo_row", self.batches * self.rows, vec![0, 0]);
        let channel = ir.begin_indexed_loop("undo_channel", self.output_channels, vec![1, 0]);
        let from = ir.op(format!("mul i32 {row}, {}", self.output_pixel_stride));
        let from = ir.op(format!("add nsw i32 {from}, {channel}"));
        let ptr = ir.op(format!(
            "getelementptr inbounds i32, i32* %input, i32 {from}"
        ));
        let value = ir.op(format!("load i32, i32* {ptr}"));
        let to = ir.op(format!("mul i32 {row}, {}", self.output_channels));
        let to = ir.op(format!("add nsw i32 {to}, {channel}"));
        let ptr = ir.op(format!("getelementptr inbounds i32, i32* %out, i32 {to}"));
        ir.line(format!("store i32 {value}, i32* {ptr}"));
        ir.end_loop("undo_channel");
        ir.end_loop("undo_row");
        ir.line("ret void");
        ir.text.push_str("}\n");
        Ok(MatrixBoundary {
            source: ir.text,
            bytes: [
                self.batches * self.rows * self.output_channels * 4,
                self.output_elements * 4,
            ],
            scalars: 0,
            workers: 1,
        })
    }
}
