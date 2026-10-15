use super::{
    output::Output,
    resources::{self, Resources},
    Error, Result,
};
use crate::{
    h264::Control,
    runtime_args::{InputSource, OutputRate, RuntimeArgs},
    runtime_preferences::Reader,
};
use num_traits::ToPrimitive;

pub(super) struct Poll {
    preferences: f64,
    brightness: f64,
    mirror: f64,
    fps: f64,
}
impl Poll {
    pub fn new(now: f64) -> Self {
        Self {
            preferences: now,
            brightness: now,
            mirror: now + 1.0,
            fps: now + 1.0,
        }
    }
    fn mirror(&mut self, now: f64, reader: &Reader, resources: &mut Resources) {
        if now >= self.mirror {
            resources.active.mirror = reader.mirror();
            self.mirror = now + 1.0;
        }
    }
    pub fn update(
        &mut self,
        now: f64,
        args: &RuntimeArgs,
        reader: &Reader,
        resources: &mut Resources,
        output: &mut Output,
        rate: &mut OutputRate,
    ) -> Result<bool> {
        if args.cluster_hud_mode.is_some() && now >= self.brightness && !reader.gate().allowed {
            return Ok(false);
        }
        self.mirror(now, reader, resources);
        if now >= self.preferences {
            let next = reader.snapshot();
            if args.cluster_hud_mode.is_some()
                && next.hud_mode.is_some()
                && next.hud_mode != args.cluster_hud_mode
                || args.cluster_hud_encoder.is_some()
                    && next.encoder.is_some()
                    && next.encoder != args.cluster_hud_encoder
            {
                return Ok(false);
            }
            let options = resources::renderer_options(args, &next, output.geometry, *rate)?;
            if args.theme.is_none() && resources.active.theme != next.theme {
                resources.renderer.set_theme_mode(next.theme);
            }
            if resources.renderer.language != options.language
                || resources.renderer.is_metric != options.is_metric
            {
                resources
                    .renderer
                    .set_display_preferences(&options.language, options.is_metric);
            }
            if resources.renderer.screen_mode != options.screen_mode {
                resources.renderer.set_screen_mode(options.screen_mode);
            }
            if resources.renderer.panel_layout != options.panel_layout {
                resources.renderer.set_panel_layout(options.panel_layout);
            }
            resources
                .input
                .configure_live(options.screen_mode, next.gate.mode);
            let old_mirror = resources.active.mirror;
            let old_orientation = resources.active.orientation;
            let old_camera = resources.active.camera_view_mode;
            resources.active = next;
            resources.active.mirror = old_mirror;
            resources.active.orientation = old_orientation;
            if args.camera_view_mode.is_some() {
                resources.active.camera_view_mode = old_camera;
            }
            self.preferences = now + 1.0;
        }
        if args.usb_output() && now >= self.brightness {
            let orientation = reader.orientation();
            let setting = args.usb_brightness.unwrap_or_else(|| reader.brightness());
            let brightness = if setting == 0 && !args.rate_presence.usb_brightness_from_cli {
                i64::from(resources.input.brightness()?.unwrap_or(80))
            } else {
                setting
            };
            output.control(Control::Brightness(
                i32::try_from(brightness.clamp(0, 100))
                    .map_err(|error| Error::Invalid(error.to_string()))?,
            ))?;
            if matches!(orientation, 0 | 2) && resources.active.orientation != orientation {
                if output.fps().is_some() && args.cluster_hud_mode.is_some() {
                    return Ok(false);
                }
                output.control(Control::Orientation(
                    i32::try_from(orientation)
                        .map_err(|error| Error::Invalid(error.to_string()))?,
                ))?;
                resources.active.orientation = orientation;
            }
            self.brightness = now + 0.1;
        } else if now >= self.brightness {
            self.brightness = now + 0.1;
        }
        if rate.fixed_usb_policy && now >= self.fps {
            let active = reader.usb_fps().to_bits() == 5.0_f64.to_bits();
            let target = args.output_rate(true, active)?.target_fps;
            if target.to_bits() != rate.target_fps.to_bits() {
                let encoder = u32::try_from(crate::rate::encoder_fps(target, args.usb_h264_fps)?)
                    .map_err(|error| Error::Invalid(error.to_string()))?;
                if output.fps().is_some_and(|fps| fps != encoder) {
                    return Ok(false);
                }
                rate.target_fps = target;
                resources.renderer.set_target_fps(
                    target
                        .round_ties_even()
                        .to_i32()
                        .ok_or_else(|| Error::Invalid("render FPS out of range".into()))?,
                );
                if rate.usb_display_fps_auto {
                    output.control(Control::DisplayFps(
                        target
                            .to_i32()
                            .ok_or_else(|| Error::Invalid("display FPS out of range".into()))?,
                    ))?;
                }
            }
            self.fps = now + 1.0;
        }
        Ok(true)
    }
}
pub(super) fn camera(resources: &Resources) -> i64 {
    resources.active.camera_view_mode
}
pub(super) fn live(args: &RuntimeArgs) -> bool {
    args.input == InputSource::Live
}
