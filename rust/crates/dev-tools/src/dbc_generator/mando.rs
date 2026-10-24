// Source-derived port of hyundai_kia_mando_corner_radar.py (MIT).
// Copyright (c) 2020, Comma.ai, Inc.; see LICENSE.opendbc.
use super::procedural;
use std::io::{self, Write};
#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    Float(f64),
}
impl Num {
    fn value(self) -> f64 {
        match self {
            Self::Int(v) => v as f64,
            Self::Float(v) => v,
        }
    }
    fn upper(self, offset: Self, length: u32) -> Self {
        let max = (1_i64 << length) - 1;
        match (self, offset) {
            (Self::Int(a), Self::Int(b)) => Self::Int(max * a + b),
            _ => Self::Float(max as f64 * self.value() + offset.value()),
        }
    }
}
impl std::fmt::Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v:?}"),
        }
    }
}
struct Signal {
    name: &'static str,
    start: i64,
    length: u32,
    scale: Num,
    offset: Num,
}
include!("mando_signals.rs");
pub(super) fn write(out: &mut dyn Write) -> io::Result<()> {
    out.write_all(procedural::MANDO_HEADER.as_bytes())?;
    for address in [0x100, 0x200] {
        procedural::mando_metadata(out, address)?;
    }
    let bit_count: i64 = SIGNALS.iter().map(|s| i64::from(s.length)).sum();
    for address in [0x101, 0x201] {
        procedural::mando_points(out, address)?;
        let mut bit_idx = SIGNALS[0].start;
        for point in 0..5 {
            for (index, sig) in SIGNALS.iter().enumerate() {
                let name = if sig.name.is_empty() {
                    format!("POINT_{}_SIGNAL_{}", point + 1, index + 1)
                } else {
                    format!("POINT_{}_{}", point + 1, sig.name)
                };
                let start = point * bit_count + sig.start;
                assert_eq!(bit_idx, start, "original signal overlap or gap invariant");
                // The source scales/offsets are dyadic with <=9 fractional
                // decimal digits, so its round(...,10) is exact here. Retain
                // Python int versus float formatting, including .0 values.
                let min = sig.offset;
                let max = sig.scale.upper(sig.offset, sig.length);
                writeln!(
                    out,
                    " SG_ {name} : {start}|{}@1+ ({},{}) [{min}|{max}] \"\" XXX",
                    sig.length, sig.scale, sig.offset
                )?;
                bit_idx += i64::from(sig.length);
            }
        }
    }
    for address in [0x104, 0x204] {
        procedural::mando_checksum(out, address)?;
    }
    Ok(())
}
