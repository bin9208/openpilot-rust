//! Batched image_dot packing, derived from original tensor.py image_dot and
//! image_conv2d (tinygrad MIT license in tinygrad_repo/LICENSE).
//! Physical packing and parameterized schedules; ONNX admission is separate.
mod boundary;
mod branch;
pub mod chain;
pub mod gemm;
pub mod graph;
mod indexed;
mod render;
pub use boundary::MatrixBoundary;
pub use branch::artifact as concat_biased_artifact;
pub use indexed::artifact as indexed_normalization_artifact;
mod schedule;
pub use render::MatrixArtifact;
pub use schedule::MatrixSchedule;
#[derive(Clone, Debug)]
pub struct MatrixLayout {
    pub batches: usize,
    pub rows: usize,
    pub input_channels: usize,
    pub output_channels: usize,
    pub input_shape: Vec<usize>,
    pub weight_shape: Vec<usize>,
    pub input_row_stride: usize,
    pub weight_output_stride: usize,
    pub output_pixel_stride: usize,
    pub input_elements: usize,
    pub weight_elements: usize,
    pub output_elements: usize,
    pub cin_last: bool,
}
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
impl MatrixLayout {
    pub fn schedule(&self, max_workers: usize) -> Result<MatrixSchedule, String> {
        MatrixSchedule::new(self, max_workers)
    }
    pub fn new(x: &[usize], w: &[usize]) -> Result<Self, String> {
        if x.len() < 2
            || w.len() != 2
            || x.contains(&0)
            || w.contains(&0)
            || x[x.len() - 1] != w[0]
            || !w[0].is_multiple_of(4)
            || !w[1].is_multiple_of(4)
        {
            return Err(
                "matrix packing requires nonempty rank>=2 input and four-aligned rank2 weights"
                    .into(),
            );
        }
        let batches = x[..x.len() - 2]
            .iter()
            .try_fold(1usize, |n, v| n.checked_mul(*v))
            .ok_or("matrix batch overflow")?;
        let rows = x[x.len() - 2];
        let (k, n) = (w[0], w[1]);
        let logical = batches
            .checked_mul(rows)
            .and_then(|v| v.checked_mul(k))
            .ok_or("matrix input overflow")?;
        let weights = k.checked_mul(n).ok_or("matrix weight overflow")?;
        if logical > 1 << 26 || weights > 1 << 26 {
            return Err("matrix packing exceeds bounded storage".into());
        }
        let kb = k / 4;
        let bank = k >= 8 && kb.is_power_of_two();
        // pad_align uses the product before dim2 and the entire unpadded tensor
        // to choose its byte/pixel alignment; batches and rows both contribute.
        let align = if batches * rows == 1 || logical < 65536 {
            32
        } else {
            256
        };
        let input_shape = if bank {
            vec![batches, rows, 1, 1, (kb + 1).next_multiple_of(align / 4), 4]
        } else {
            vec![batches, rows, (align / gcd(k, align)), k]
        };
        let wa = if n / 4 == 1 || weights < 65536 {
            32
        } else {
            256
        };
        let weight_shape = if bank {
            vec![n / 4, 1, (kb + 1).next_multiple_of(wa / 16), 1, 4, 4]
        } else {
            vec![n / 4, wa / gcd(kb * 16, wa), kb, 1, 4, 4]
        };
        let input_elements = input_shape.iter().product::<usize>();
        let weight_elements = weight_shape.iter().product::<usize>();
        let output_pixel_stride = n * (32 / gcd(n, 32));
        let output_elements = batches
            .checked_mul(rows)
            .and_then(|v| v.checked_mul(output_pixel_stride))
            .ok_or("matrix output overflow")?;
        if input_elements > 1 << 27 || weight_elements > 1 << 27 || output_elements > 1 << 27 {
            return Err("matrix physical storage exceeds bound".into());
        }
        Ok(Self {
            batches,
            rows,
            input_channels: k,
            output_channels: n,
            input_row_stride: input_elements / (batches * rows),
            weight_output_stride: weight_elements / (n / 4),
            input_shape,
            weight_shape,
            output_pixel_stride,
            input_elements,
            weight_elements,
            output_elements,
            cin_last: rows == 1,
        })
    }
    pub fn pack_input_into(&self, logical: &[u8], packed: &mut [u8]) -> Result<(), String> {
        if logical.len() != self.batches * self.rows * self.input_channels * 4
            || packed.len() != self.input_elements * 2
        {
            return Err("matrix input packing byte count".into());
        }
        for row in 0..self.batches * self.rows {
            for channel in 0..self.input_channels {
                let from = (row * self.input_channels + channel) * 4;
                let bits = u32::from_le_bytes(logical[from..from + 4].try_into().unwrap());
                let value = super::image::half_storage(bits).to_le_bytes();
                let to = (row * self.input_row_stride + channel) * 2;
                packed[to..to + 2].copy_from_slice(&value);
            }
        }
        Ok(())
    }
    pub fn pack_weights_into(&self, logical: &[u8], packed: &mut [u8]) -> Result<(), String> {
        if logical.len() != self.input_channels * self.output_channels * 2
            || packed.len() != self.weight_elements * 2
        {
            return Err("matrix weight packing byte count".into());
        }
        for input in 0..self.input_channels {
            for output in 0..self.output_channels {
                let from = (input * self.output_channels + output) * 2;
                let lane = if self.cin_last {
                    (output % 4) * 4 + input % 4
                } else {
                    (input % 4) * 4 + output % 4
                };
                let to = ((output / 4) * self.weight_output_stride + (input / 4) * 16 + lane) * 2;
                // Original nonnative-half contiguous materialization passes
                // through float. Its bit conversion flushes half subnormals
                // to signed zero and preserves the stored NaN payload bits.
                let bits = u16::from_le_bytes(logical[from..from + 2].try_into().unwrap());
                let bits = if bits & 0x7c00 == 0 {
                    bits & 0x8000
                } else {
                    bits
                };
                packed[to..to + 2].copy_from_slice(&bits.to_le_bytes());
            }
        }
        Ok(())
    }
    /// Undo image_conv2d's padded output width, then image_dot's final reshape
    /// and transpose. Preserve the stored f32 bits, including NaN payloads.
    pub fn unpack_output_into(&self, physical: &[u8], logical: &mut [u8]) -> Result<(), String> {
        let row_bytes = self.output_channels * 4;
        if physical.len() != self.output_elements * 4
            || logical.len() != self.batches * self.rows * row_bytes
        {
            return Err("matrix output view byte count".into());
        }
        for row in 0..self.batches * self.rows {
            let from = row * self.output_pixel_stride * 4;
            logical[row * row_bytes..(row + 1) * row_bytes]
                .copy_from_slice(&physical[from..from + row_bytes]);
        }
        Ok(())
    }
}
