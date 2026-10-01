//! Source-compatible QR segmentation/mask selection over native qrcodegen encoding.
//! Encoder API: https://docs.rs/qrcodegen/1.8.0/qrcodegen/struct.QrCode.html
mod penalty;
mod segments;
pub mod texture;
use crate::Error;
use qrcodegen::{Mask, QrCode, QrCodeEcc, Version};
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Correction {
    Low,
    Medium,
}
pub struct Matrix {
    pub size: usize,
    pub modules: Vec<bool>,
    pub mask: u8,
}
pub fn encode(data: &str, correction: Correction) -> Result<Matrix, Error> {
    let segments = segments::source_segments(data);
    let ecc = match correction {
        Correction::Low => QrCodeEcc::Low,
        Correction::Medium => QrCodeEcc::Medium,
    };
    let mut best: Option<(u32, QrCode)> = None;
    for mask in 0..8 {
        let qr = QrCode::encode_segments_advanced(
            &segments,
            ecc,
            Version::MIN,
            Version::MAX,
            Some(Mask::new(mask)),
            false,
        )
        .map_err(|_| Error::Contract("QR data exceeds source capacity"))?;
        let score = penalty::source_score(&qr);
        if best.as_ref().is_none_or(|(previous, _)| score < *previous) {
            best = Some((score, qr));
        }
    }
    let (_, qr) = best.ok_or(Error::Contract("QR mask missing"))?;
    let size = usize::try_from(qr.size()).map_err(|_| Error::Contract("QR size invalid"))?;
    let mut modules = Vec::with_capacity(size * size);
    for y in 0..qr.size() {
        for x in 0..qr.size() {
            modules.push(qr.get_module(x, y));
        }
    }
    Ok(Matrix {
        size,
        modules,
        mask: qr.mask().value(),
    })
}
impl Matrix {
    pub fn rgba(&self) -> (usize, Vec<u8>) {
        let width = (self.size + 8) * 10;
        let mut output = vec![255; width * width * 4];
        for y in 0..self.size {
            for x in 0..self.size {
                if self.modules[y * self.size + x] {
                    for dy in 0..10 {
                        for dx in 0..10 {
                            let p = (((y + 4) * 10 + dy) * width + (x + 4) * 10 + dx) * 4;
                            output[p..p + 3].fill(0);
                        }
                    }
                }
            }
        }
        (width, output)
    }
}
