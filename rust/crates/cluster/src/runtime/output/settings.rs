use super::{
    super::{Error, Result},
    Geometry,
};
use crate::{
    h264,
    runtime_args::{OutputRate, RuntimeArgs},
};
use num_traits::ToPrimitive;

pub(super) fn software(args: &RuntimeArgs, rate: OutputRate) -> Result<h264::Config> {
    Ok(h264::Config {
        width: 2,
        height: 2,
        fps: u32::try_from(crate::rate::encoder_fps(
            rate.target_fps,
            args.usb_h264_fps,
        )?)
        .map_err(|error| Error::Invalid(error.to_string()))?,
        bitrate: crate::rate::bitrate(&args.usb_h264_bitrate, rate.target_fps, args.usb_h264_fps)?,
        gop: u32::try_from(args.usb_h264_gop).map_err(|error| Error::Invalid(error.to_string()))?,
        ffmpeg: args.usb_h264_ffmpeg.clone().into(),
        encoder: args.usb_h264_ffmpeg_encoder.clone(),
        requested_chunk_size: args.usb_h264_chunk_size,
        wait_for_ack: args.ack.usb_h264_wait_ack || args.ack.usb_h264_soft_ack,
        soft_ack: args.ack.usb_h264_soft_ack,
        dump: (!args.usb_h264_dump.is_empty()).then(|| args.usb_h264_dump.clone().into()),
    })
}
pub(super) fn configure(config: &mut h264::Config, geometry: Geometry) -> Result<()> {
    config.width =
        u32::try_from(geometry.logical[0]).map_err(|error| Error::Invalid(error.to_string()))?;
    config.height =
        u32::try_from(geometry.logical[1]).map_err(|error| Error::Invalid(error.to_string()))?;
    Ok(())
}

/// # Errors
/// Rejects a nonpositive bitrate or a value outside the native encoder integer ABI.
pub fn bitrate(text: &str) -> Result<i32> {
    let text = text.trim_matches(openpilot_startup_ui::text::whitespace);
    let (number, multiplier) = if let Some(number) = text.strip_suffix(['k', 'K']) {
        (number, 1000.0)
    } else if let Some(number) = text.strip_suffix(['m', 'M']) {
        (number, 1_000_000.0)
    } else {
        (text, 1.0)
    };
    let value = openpilot_runtime_core::python_float::parse(number)
        .and_then(|value| (value * multiplier + 0.5).to_i32())
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::Invalid(format!("invalid H264 bitrate: {text}")))?;
    Ok(value)
}

#[cfg(feature = "native-media")]
pub(super) fn hardware(args: &RuntimeArgs, config: &h264::Config) -> Result<h264::vidc::Config> {
    use crate::runtime_args::{InputFormat, RateControl};
    Ok(h264::vidc::Config {
        width: 2,
        height: 2,
        fps: i32::try_from(config.fps).map_err(|error| Error::Invalid(error.to_string()))?,
        bitrate: bitrate(&config.bitrate)?,
        gop: i32::try_from(config.gop).map_err(|error| Error::Invalid(error.to_string()))?,
        slice_max_bytes: i32::try_from(args.usb_h264_slice_max_bytes)
            .map_err(|error| Error::Invalid(error.to_string()))?,
        rate_control: match args.usb_h264_rate_control {
            RateControl::Off => 0,
            RateControl::VbrVfr => 1,
            RateControl::VbrCfr => 2,
            RateControl::CbrVfr => 3,
            RateControl::CbrCfr => 4,
            RateControl::MbrCfr => 5,
            RateControl::MbrVfr => 6,
            RateControl::Cq => 7,
        },
        realtime_priority: args.runtime.usb_h264_realtime_priority,
        debug: args.diagnostics.usb_h264_debug,
        input_format: match args.usb_h264_input_format {
            InputFormat::Auto => h264::vidc::InputFormat::Auto,
            InputFormat::Nv12 => h264::vidc::InputFormat::Nv12,
        },
        device_path: args.usb_h264_device.clone(),
    })
}
