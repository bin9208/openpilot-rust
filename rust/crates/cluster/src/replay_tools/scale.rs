// SPDX-License-Identifier: LGPL-2.0-or-later
// Pygame 2.6.1 transform.c/scale_mmx64.c translated to safe Rust.
// Copyright (C) 2000-2001 Pete Shinners; see provenance.json.
use super::{Error, Result};
use openpilot_sdl2::gui::Image;
#[derive(Clone, Copy, Debug)]
pub enum ScaleBackend {
    Generic,
    Sse,
}
impl Default for ScaleBackend {
    fn default() -> Self {
        if cfg!(target_arch = "x86_64") {
            Self::Sse
        } else {
            Self::Generic
        }
    }
}
/// # Errors
/// Rejects invalid dimensions, byte count or allocation overflow.
pub fn smooth_scale(
    bytes: &[u8],
    width: usize,
    height: usize,
    target: (usize, usize),
    backend: ScaleBackend,
) -> Result<Image> {
    if width == 0
        || height == 0
        || target.0 == 0
        || target.1 == 0
        || width.checked_mul(height).and_then(|s| s.checked_mul(4)) != Some(bytes.len())
    {
        return Err(Error::Contract("scale dimensions invalid"));
    }
    let horizontal = axis(bytes, width, height, target.0, false, backend)?;
    let output = axis(&horizontal, target.0, height, target.1, true, backend)?;
    Ok(Image {
        width: i32::try_from(target.0).map_err(|_| Error::Contract("scale width overflow"))?,
        height: i32::try_from(target.1).map_err(|_| Error::Contract("scale height overflow"))?,
        rgba: output,
    })
}
fn axis(
    input: &[u8],
    width: usize,
    height: usize,
    target: usize,
    vertical: bool,
    backend: ScaleBackend,
) -> Result<Vec<u8>> {
    let source = if vertical { height } else { width };
    if source == target {
        return Ok(input.to_vec());
    }
    let lanes = if vertical { width * 4 } else { height * 4 };
    let count = lanes
        .checked_mul(target)
        .ok_or(Error::Contract("scale size overflow"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| Error::Contract("scale allocation failed"))?;
    output.resize(count, 0);
    let at = |step: usize, lane: usize| {
        if vertical {
            step * width * 4 + lane
        } else {
            (lane / 4 * width + step) * 4 + lane % 4
        }
    };
    let out = |step: usize, lane: usize| {
        if vertical {
            step * width * 4 + lane
        } else {
            (lane / 4 * target + step) * 4 + lane % 4
        }
    };
    if source < target {
        let bits = match backend {
            ScaleBackend::Generic => 16,
            ScaleBackend::Sse => 8,
        };
        let unit = 1_u64 << bits;
        for x in 0..target {
            let product = x * (source - 1);
            let index = product / target;
            let b = unit
                * u64::try_from(product % target)
                    .map_err(|_| Error::Contract("scale factor overflow"))?
                / u64::try_from(target).map_err(|_| Error::Contract("scale factor overflow"))?;
            for lane in 0..lanes {
                let value = (u64::from(input[at(index, lane)]) * (unit - b)
                    + u64::from(input[at((index + 1).min(source - 1), lane)]) * b)
                    >> bits;
                output[out(x, lane)] =
                    u8::try_from(value).map_err(|_| Error::Contract("scale channel overflow"))?;
            }
        }
    } else {
        let bits = match backend {
            ScaleBackend::Generic => 16,
            ScaleBackend::Sse => 14,
        };
        let unit = 1_u64 << bits;
        let space = unit
            * u64::try_from(source).map_err(|_| Error::Contract("scale factor overflow"))?
            / u64::try_from(target).map_err(|_| Error::Contract("scale factor overflow"))?;
        let reciprocal = match backend {
            ScaleBackend::Generic => (1_u64 << 32) / space,
            ScaleBackend::Sse => (1_u64 << 30) / space,
        };
        for lane in 0..lanes {
            let mut accumulated = 0_u16;
            let mut remaining = space;
            let mut destination = 0;
            for step in 0..source {
                let channel = u64::from(input[at(step, lane)]);
                if remaining > unit {
                    accumulated = accumulated.wrapping_add(u16::from(input[at(step, lane)]));
                    remaining -= unit;
                } else {
                    let fraction = unit - remaining;
                    let value = ((u64::from(accumulated) + ((channel * remaining) >> bits))
                        * reciprocal)
                        >> 16;
                    if destination < target {
                        output[out(destination, lane)] = u8::try_from(value.min(255))
                            .map_err(|_| Error::Contract("scale channel overflow"))?;
                    }
                    destination += 1;
                    accumulated = u16::try_from((channel * fraction) >> bits)
                        .map_err(|_| Error::Contract("scale accumulator overflow"))?;
                    remaining = space - fraction;
                }
            }
        }
    }
    Ok(output)
}
