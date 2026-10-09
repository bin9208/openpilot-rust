//! `main.py` and `cluster_config.py` rate/auto-bitrate calculations.
use crate::Error;
use num_traits::ToPrimitive;

/// Source Python round followed by max(1, ...).
///
/// # Errors
/// Rejects nonfinite/unrepresentable rates like Python's integer conversion.
pub fn encoder_fps(target: f64, fallback: i64) -> Result<i64, Error> {
    let rate = if target > 0.0 {
        target
    } else {
        fallback
            .to_f64()
            .ok_or(Error::Contract("encoder rate conversion failed"))?
    };
    rate.round_ties_even()
        .max(1.0)
        .to_i64()
        .ok_or(Error::Contract("encoder rate conversion failed"))
}

/// Source requested display FPS remains unclamped; only auto H264 FPS is clamped.
///
/// # Errors
/// Returns the source integer-conversion error for a nonfinite rate.
pub fn display_fps(
    requested: Option<i64>,
    h264: bool,
    target: f64,
    fallback: i64,
) -> Result<i64, Error> {
    if let Some(rate) = requested {
        return Ok(rate);
    }
    if !h264 {
        return Ok(0);
    }
    Ok(encoder_fps(target, fallback)?.clamp(1, 255))
}

/// `cluster_config.py` `resolved_usb_h264_bitrate`.
///
/// # Errors
/// Rejects a nonfinite/unrepresentable encoder rate.
pub fn bitrate(requested: &str, target: f64, fallback: i64) -> Result<String, Error> {
    let value = requested.trim();
    if !value.eq_ignore_ascii_case("auto") {
        return Ok(value.to_owned());
    }
    let rate = i128::from(encoder_fps(target, fallback)?);
    let bits = (((rate * 7_000_000 + 15_000) / 30_000) * 1_000).clamp(1_000_000, 14_000_000);
    Ok(if bits % 1_000_000 == 0 {
        format!("{}M", bits / 1_000_000)
    } else if bits % 1_000 == 0 {
        format!("{}k", bits / 1_000)
    } else {
        bits.to_string()
    })
}

#[must_use]
pub fn prefers_wide(mode: i64, speed_kph: f64, currently_wide: bool) -> bool {
    if mode == 3 {
        return true;
    }
    if mode != 4 {
        return false;
    }
    if currently_wide {
        speed_kph < 54.0
    } else {
        speed_kph <= 36.0
    }
}

#[must_use]
pub fn wide_zoom(speed_kph: f64) -> f64 {
    // Preserve Python max/min behavior for NaN as well as the multiplication order.
    let ratio = if speed_kph.is_nan() {
        1.0
    } else {
        (speed_kph / 54.0).clamp(0.0, 1.0)
    };
    let smooth = ratio * ratio * (3.0 - 2.0 * ratio);
    1.0 + smooth * (1.55 - 1.0)
}
