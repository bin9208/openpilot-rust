use super::super::{Error, Result};
use crate::runtime_args::{Codec, Orientation, RuntimeArgs};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Geometry {
    pub frame: [i32; 2],
    pub logical: [i32; 2],
    pub encoder: [i32; 2],
}
fn dimension(value: i64) -> Result<i32> {
    i32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::Invalid("cluster dimension exceeds native representation".into()))
}
fn align(value: i32, alignment: i64) -> Result<i32> {
    if alignment <= 1 {
        return Ok(value);
    }
    let value = i64::from(value)
        .checked_add(alignment - 1)
        .map(|value| value / alignment * alignment)
        .ok_or_else(|| Error::Invalid("cluster aligned dimension overflow".into()))?;
    dimension(value)
}
impl Geometry {
    /// # Errors
    /// Rejects native dimension overflow or odd logical H264 encoder dimensions.
    pub fn resolve(args: &RuntimeArgs, display: Option<[u32; 2]>, hardware: bool) -> Result<Self> {
        let display = display.unwrap_or([1920, 480]);
        let mut frame = [
            dimension(args.width.unwrap_or(i64::from(display[0])))?,
            dimension(args.height.unwrap_or(i64::from(display[1])))?,
        ];
        if args.usb_codec == Codec::H264 {
            frame = [
                align(frame[0], args.usb_h264_align)?,
                align(frame[1], args.usb_h264_align)?,
            ];
        }
        let logical = if args.usb_h264_orientation == Orientation::Portrait {
            [frame[1], frame[0]]
        } else {
            frame
        };
        if args.usb_codec == Codec::H264 && logical.iter().any(|value| value % 2 != 0) {
            return Err(Error::Invalid(format!(
                "H264 USB output requires even encoder dimensions, got {}x{}",
                logical[0], logical[1]
            )));
        }
        let encoder = if hardware && args.usb_codec == Codec::H264 {
            [
                align(logical[0], args.usb_h264_encoder_align)?,
                align(logical[1], args.usb_h264_encoder_align)?,
            ]
        } else {
            logical
        };
        Ok(Self {
            frame,
            logical,
            encoder,
        })
    }
}
