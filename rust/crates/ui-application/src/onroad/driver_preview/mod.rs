pub mod policy;
use super::camera::{CameraView, Config};
use crate::{context::Context, paint::color, state::messages};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::driver_monitoring_state::MonitoringPolicy;
use openpilot_msgq::VisionStream;
use openpilot_ui_framework::{
    canvas::Canvas,
    draw::{Draw, TextDraw},
    geometry::{Point, Rect},
    text::Font,
    text_layout::float,
    Error,
};

/// Read-only onroad inset. It neither writes IsDriverViewEnabled nor handles touches.
pub struct DriverPreview {
    context: Context,
    camera: CameraView,
    compact: bool,
    was_visible: bool,
    next_poll: f64,
    started_frame: i64,
}
impl DriverPreview {
    pub fn new(context: Context, canvas: &mut Canvas, name: &str) -> Result<Self, Error> {
        let compact = !context.big;
        let camera = CameraView::new(
            context.clone(),
            Config {
                name: name.into(),
                stream: VisionStream::Driver,
                // Both source inset layouts use onroad/CameraView's unenhanced shader.
                compact: false,
            },
            canvas,
        )?;
        Ok(Self {
            context,
            camera,
            compact,
            was_visible: false,
            next_poll: 0.0,
            started_frame: -1,
        })
    }
    pub fn draw_onroad(
        &mut self,
        parent: Rect,
        alert_visible: bool,
        now: f64,
        draw: &mut dyn Draw,
    ) -> Result<bool, crate::Error> {
        let (visible, started_frame) = {
            let ui = self.context.ui.borrow();
            let sm = self.context.messages.borrow();
            (
                ui.started
                    && (messages::selfdrive_state(&sm.state)?.get_enabled()
                        || ui.realtime.value.always_on_dm)
                    && !alert_visible,
                ui.started_frame,
            )
        };
        if (!visible && self.was_visible) || (visible && self.started_frame != started_frame) {
            self.camera.reset_stream(draw)?;
            self.next_poll = 0.0;
        }
        self.was_visible = visible;
        self.started_frame = started_frame;
        if !visible {
            return Ok(false);
        }
        let sm = self.context.messages.borrow();
        let fresh = |service| -> Result<bool, crate::Error> {
            let topic = sm.state.topic(service)?;
            Ok(topic.valid
                && topic.alive
                && topic.receive_frame > started_frame
                && policy::sample_fresh(now, topic.log_mono_time.to_f64().unwrap_or(0.0) / 1e9))
        };
        let monitoring_fresh = fresh("driverMonitoringState")?;
        let driver_fresh = fresh("driverStateV2")?;
        let dm = messages::driver_monitoring_state(&sm.state)?;
        let vision = dm.get_vision_policy_state()?;
        let camera_allowed = monitoring_fresh && !dm.get_camera_unavailable() && driver_fresh;
        if camera_allowed && now >= self.next_poll {
            self.next_poll = now + 0.1;
            self.camera.poll(now, draw)?;
        }
        let metadata = self.camera.frame();
        let frame_fresh = camera_allowed
            && metadata.is_some_and(|frame| {
                policy::sample_fresh(now, frame.timestamp_sof.to_f64().unwrap_or(0.0) / 1e9)
            });
        if !frame_fresh {
            self.camera.discard_frame();
        }
        let (kind, label) = policy::state(policy::Input {
            monitoring_fresh,
            camera_unavailable: dm.get_camera_unavailable(),
            driver_fresh,
            frame_fresh,
            face_detected: vision.get_face_detected(),
            distracted: vision.get_is_distracted(),
            lockout: dm.get_lockout() || dm.get_always_on_lockout(),
            wheel_policy: dm.get_active_policy()? == MonitoringPolicy::Wheeltouch,
        });
        let tint = match kind {
            "tracking" => color(70, 220, 140, 255),
            "wheel" => color(110, 190, 255, 255),
            "warning" => color(255, 90, 60, 255),
            _ => color(255, 195, 70, 255),
        };
        let [x, y, width, height] = policy::rect(
            f64::from(parent.x),
            f64::from(parent.y),
            f64::from(parent.width),
            f64::from(parent.height),
            self.compact,
        )
        .map(float);
        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        draw.rounded(rect, 0.0, color(12, 18, 24, 245))?;
        let label_h = if self.compact { 14.0 } else { 32.0 };
        let viewport = Rect {
            x: x + 2.0,
            y: y + 2.0,
            width: width - 4.0,
            height: height - label_h - 4.0,
        };
        if frame_fresh {
            let metadata = metadata.ok_or(Error::Contract("fresh driver frame absent"))?;
            let drivers = messages::driver_state(&sm.state)?;
            let driver = if dm.get_is_r_h_d() {
                drivers.get_right_driver_data()?
            } else {
                drivers.get_left_driver_data()?
            };
            let position: Vec<_> = driver.get_face_position()?.iter().map(f64::from).collect();
            let w = metadata
                .width
                .to_f64()
                .ok_or(Error::Contract("driver width overflow"))?;
            let h = metadata
                .height
                .to_f64()
                .ok_or(Error::Contract("driver height overflow"))?;
            let crop = if vision.get_face_detected() {
                policy::face_crop(
                    w,
                    h,
                    &position,
                    f64::from(viewport.width) / f64::from(viewport.height),
                )
            } else {
                None
            };
            let [sx, sy, sw, sh] = crop.unwrap_or([0.0, 0.0, w, h]);
            let scale = (f64::from(viewport.width) / sw).min(f64::from(viewport.height) / sh);
            self.camera.draw_region(
                draw,
                Rect {
                    x: float(sx),
                    y: float(sy),
                    width: float(-sw),
                    height: float(sh),
                },
                Rect {
                    x: float(
                        f64::from(viewport.x) + (f64::from(viewport.width) - sw * scale) / 2.0,
                    ),
                    y: float(
                        f64::from(viewport.y) + (f64::from(viewport.height) - sh * scale) / 2.0,
                    ),
                    width: float(sw * scale),
                    height: float(sh * scale),
                },
            )?;
        } else if kind == "wheel" {
            let center = Point {
                x: viewport.x + viewport.width / 2.0,
                y: viewport.y + viewport.height / 2.0,
            };
            let radius = viewport.width.min(viewport.height) * 0.32;
            draw.circle_lines(
                (
                    center.x.to_i32().unwrap_or(0),
                    center.y.to_i32().unwrap_or(0),
                ),
                radius,
                tint,
            )?;
            for (dx, dy) in [(-0.9, -0.2), (0.9, -0.2), (0.0, 0.9)] {
                draw.line(
                    center,
                    Point {
                        x: center.x + dx * radius,
                        y: center.y + dy * radius,
                    },
                    2.0,
                    tint,
                )?;
            }
        } else {
            let size = if self.compact { 20.0 } else { 48.0 };
            draw.text(TextDraw {
                font: Font::Bold,
                text: "DM",
                position: Point {
                    x: viewport.x + 8.0,
                    y: viewport.y + viewport.height / 2.0 - size / 2.0,
                },
                size: float(f64::from(size) * draw.font_scale()),
                spacing: 0.0,
                color: tint,
            })?;
        }
        draw.rectangle_lines_ex(rect, if self.compact { 1.0 } else { 2.0 }, tint)?;
        draw.text(TextDraw {
            font: Font::Bold,
            text: label,
            position: Point {
                x: x + 3.0,
                y: y + height - label_h + 1.0,
            },
            size: float(if self.compact { 11.0 } else { 21.0 } * draw.font_scale()),
            spacing: 0.0,
            color: tint,
        })?;
        Ok(true)
    }
}
