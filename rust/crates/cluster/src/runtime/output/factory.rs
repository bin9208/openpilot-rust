use super::super::{Error, Result};
use crate::{
    autorun,
    runtime_args::{OutputRate, RuntimeArgs},
    runtime_preferences::Snapshot,
    usb::{Display, Options, ParamsRuntime},
};
use std::{io, path::Path};

#[derive(Clone)]
pub(super) struct Spec {
    options: Options,
    orientation: i32,
    profile: bool,
}
impl Spec {
    pub fn new(args: &RuntimeArgs, preferences: &Snapshot, rate: OutputRate) -> Result<Self> {
        let brightness = args
            .usb_brightness
            .unwrap_or(preferences.brightness)
            .clamp(0, 100);
        let brightness = if brightness == 0 && !args.rate_presence.usb_brightness_from_cli {
            80
        } else {
            brightness
        };
        let convert =
            |value: i64| i32::try_from(value).map_err(|error| Error::Invalid(error.to_string()));
        Ok(Self {
            options: Options {
                brightness: convert(brightness)?,
                display_fps: convert(rate.usb_display_fps)?,
                fast_write: args.transport.usb_fast,
                wait_for_frame_ack: args.transport.usb_wait_frame_ack,
                frame_drain_attempts: convert(args.usb_frame_drain_attempts)?,
                frame_drain_timeout_ms: convert(args.usb_frame_drain_timeout_ms)?,
                fast_frame_drain_attempts: convert(args.usb_fast_drain_attempts)?,
                fast_frame_drain_timeout_ms: convert(args.usb_fast_drain_timeout_ms)?,
                expected_product_id: args.cluster_hud_mode.and_then(autorun::product_id),
            },
            orientation: i32::from(preferences.orientation == 2) * 2,
            profile: args.diagnostics.profile_render,
        })
    }
    pub fn open(&self) -> io::Result<Display<ParamsRuntime>> {
        let mut display = Display::new(
            self.options.clone(),
            ParamsRuntime::default(),
            Path::new("/tmp/carrot_usbgpu_bus.lock"),
        )
        .map_err(io::Error::other)?;
        display.set_orientation(self.orientation, false);
        display.set_profile_enabled(self.profile);
        display.open().map_err(io::Error::other)?;
        Ok(display)
    }
}

pub(super) fn classify(error: io::Error) -> Error {
    if error
        .get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<Error>)
    {
        let kind = error.kind();
        match error.into_inner() {
            Some(cause) => match cause.downcast::<Error>() {
                Ok(error) => *error,
                Err(cause) => Error::Io(io::Error::new(kind, cause)),
            },
            None => Error::Io(io::Error::from(kind)),
        }
    } else if error
        .get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<crate::Error>)
    {
        Error::Io(error)
    } else {
        Error::H264Initialization(error)
    }
}
