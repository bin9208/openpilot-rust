//! Active compact HUD composition from mici/onroad/hud_renderer.py (MIT).
mod panel;
mod side;
mod wheel;
use super::turn_intent::TurnIntent;
use crate::{context::Context, onroad::hud::common, paint, state::messages};
use openpilot_ui_framework::{
    animation::Filter,
    assets::Texture,
    canvas::Canvas,
    draw::Draw,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

#[derive(serde::Serialize)]
pub struct Snapshot {
    pub cruise_set: bool,
    pub cruise_available: bool,
    pub set_speed: f64,
    pub changed: f64,
    pub speed: f64,
    pub cluster_seen: bool,
    pub engaged: bool,
    pub torque: f64,
    pub wheel_alpha: f64,
    pub animation_text: String,
    pub animation_time: f64,
    pub turn: super::turn_intent::Snapshot,
}
type TrafficInfo = (String, String, Option<f64>);
type OverlayKey = (
    Option<crate::overlay_state::Key>,
    Option<TrafficInfo>,
    bool,
    bool,
);
pub struct Hud {
    overlay: openpilot_ui_framework::overlay::PaintLayer<OverlayKey>,
    overlay_state: Option<crate::overlay_state::Shared>,
    traffic: Option<TrafficInfo>,
    state: WidgetState,
    context: Context,
    cruise_set: bool,
    cruise_available: bool,
    set_speed: f64,
    changed: f64,
    speed: f64,
    cluster_seen: bool,
    engaged: bool,
    top_icons: bool,
    critical: bool,
    debug_speed: bool,
    debug_traffic: bool,
    torque: Filter,
    wheel_alpha: Filter,
    wheel_y: Filter,
    set_alpha: Filter,
    turn: TurnIntent,
    wheel: Texture,
    wheel_critical: Texture,
    wheel_lane: Texture,
    wheel_cap: Texture,
    exclamation: Texture,
    speed_bg: Texture,
    cruise_text_last: String,
    animation_text: String,
    animation_time: f64,
    animation_started: f64,
}
impl Hud {
    pub fn new(context: Context, canvas: &mut Canvas, fps: f64) -> Result<Self, Error> {
        Ok(Self {
            overlay: openpilot_ui_framework::overlay::PaintLayer::new(1),
            overlay_state: None,
            traffic: None,
            state: WidgetState::default(),
            turn: TurnIntent::new(context.clone(), canvas, fps)?,
            context,
            cruise_set: false,
            cruise_available: true,
            set_speed: 255.0,
            changed: 0.0,
            speed: 0.0,
            cluster_seen: false,
            engaged: false,
            top_icons: true,
            critical: false,
            debug_speed: false,
            debug_traffic: false,
            torque: Filter::new(0.0, 0.1, fps),
            wheel_alpha: Filter::new(0.0, 0.05, fps),
            wheel_y: Filter::new(0.0, 0.1, fps),
            set_alpha: Filter::new(0.0, 0.1, fps),
            wheel: paint::texture(canvas, "icons_mici/carrot_wheel.png", (50, 50))?,
            wheel_critical: paint::texture(
                canvas,
                "icons_mici/carrot_wheel_critical.png",
                (50, 50),
            )?,
            wheel_lane: paint::texture(canvas, "icons_mici/carrot_wheel_lane.png", (100, 50))?,
            wheel_cap: paint::texture(canvas, "icons_mici/carrot_wheel_cap.png", (50, 50))?,
            exclamation: paint::texture(canvas, "icons_mici/exclamation_point.png", (44, 44))?,
            speed_bg: paint::texture(canvas, "images/speed_bg.png", (307, 115))?,
            cruise_text_last: String::new(),
            animation_text: String::new(),
            animation_time: -1.,
            animation_started: 0.,
        })
    }
    pub fn set_overlay_state(&mut self, state: crate::overlay_state::Shared) {
        self.overlay_state = Some(state);
    }
    pub fn set_wheel_critical_icon(&mut self, critical: bool) {
        self.critical = critical;
    }
    pub fn set_can_draw_top_icons(&mut self, enabled: bool) {
        self.top_icons = enabled;
    }
    pub fn drawing_top_icons(&self) -> bool {
        self.set_alpha.x > 1e-2
    }
    pub fn set_debug(&mut self, speed: bool, traffic: bool) {
        self.debug_speed = speed;
        self.debug_traffic = traffic;
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            cruise_set: self.cruise_set,
            cruise_available: self.cruise_available,
            set_speed: self.set_speed,
            changed: self.changed,
            speed: self.speed,
            cluster_seen: self.cluster_seen,
            engaged: self.engaged,
            torque: self.torque.x,
            wheel_alpha: self.wheel_alpha.x,
            animation_text: self.animation_text.clone(),
            animation_time: self.animation_time,
            turn: self.turn.snapshot(),
        }
    }
    fn animate(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        use crate::onroad::hud::style::{self, Anchor, Text};
        use openpilot_ui_framework::text::Font;
        if self.animation_time <= 0. || self.animation_text.is_empty() {
            return Ok(());
        }
        let elapsed = (self.context.now_monotonic)() - self.animation_started;
        self.animation_time = (120. - 12. * (1. + elapsed * 20.)).max(0.);
        if elapsed >= 0.5 {
            return Ok(());
        }
        let time = self.animation_time.min(100.);
        let rect = self.state.rect;
        let x = f64::from(rect.x);
        let y = f64::from(rect.y);
        let panel_y = y + f64::from(rect.height) - f64::from(self.speed_bg.height) - 10.0;
        let start = [
            x + f64::from(rect.width) * 0.5,
            y + f64::from(rect.height) * 0.45,
        ];
        let target = [
            x + 10.0 + f64::from(self.speed_bg.width) * 0.76,
            panel_y + f64::from(self.speed_bg.height) * 0.33,
        ];
        let position = std::array::from_fn(|i| {
            ((start[i] * time + target[i] * (100.0 - time)) / 100.0).trunc()
        });
        style::text(
            draw,
            Text {
                value: &self.animation_text,
                position,
                size: ((96.0 * time + 40.0 * (100.0 - time)) / 100.0).trunc(),
                font: Font::Display,
                color: common::GREEN,
                anchor: Anchor::Center,
                border: if self.animation_time >= 100. {
                    3.0
                } else {
                    1.0
                },
                shadow: if self.animation_time >= 100. {
                    3.0
                } else {
                    0.0
                },
                y_offset: 0.0,
            },
        )
    }
}
impl Widget for Hud {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, frame: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        self.traffic = self.traffic_info()?;
        self.wheel_alpha.update(255.0 * 0.95);
        self.wheel_y.update(0.0);
        let messages = self.context.messages.borrow();
        if messages
            .state
            .topic("carState")
            .map_err(crate::Error::from)?
            .receive_frame
            < self.context.ui.borrow().started_frame
        {
            self.cruise_set = false;
            self.set_speed = 255.0;
            self.speed = 0.0;
            return Ok(());
        }
        let car = messages::car_state(&messages.state)?;
        let controls = messages::controls_state(&messages.state)?;
        let cruise_cluster = f64::from(car.get_v_cruise_cluster());
        let set_speed = if cruise_cluster == 0.0 {
            f64::from(controls.get_deprecated().get_v_cruise())
        } else {
            cruise_cluster
        };
        let engaged = messages::selfdrive_state(&messages.state)?.get_enabled();
        if engaged && (set_speed != self.set_speed || !self.engaged) {
            self.changed = frame.now;
        }
        self.engaged = engaged;
        self.set_speed = set_speed;
        self.cruise_set = set_speed > 0.0 && set_speed < 255.0;
        self.cruise_available = set_speed != -1.0;
        let metric = self.context.ui.borrow().realtime.value.is_metric;
        let text = if self.engaged && self.cruise_set {
            format!(
                "{:.0}",
                self.set_speed * if metric { 1.0 } else { 0.621371 }
            )
        } else {
            "--".into()
        };
        if text != self.cruise_text_last {
            self.cruise_text_last = text.clone();
            if text != "--" {
                self.animation_text = text;
                self.animation_time = 120.;
                self.animation_started = (self.context.now_monotonic)();
            }
        }
        let cluster = f64::from(car.get_v_ego_cluster());
        self.cluster_seen |= cluster != 0.0;
        let speed = if self.cluster_seen {
            cluster
        } else {
            f64::from(car.get_v_ego())
        };
        self.speed = (speed * if metric { 3.6 } else { 3.6 * (1.0 / 1.609344) }).max(0.0);
        drop(messages);
        self.torque.update(common::torque(&self.context)?);
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let state = self
            .overlay_state
            .as_ref()
            .map(|s| s.borrow().clone())
            .unwrap_or_default();
        let key = (
            state.key.clone(),
            self.traffic.clone(),
            self.top_icons,
            self.critical,
        );
        let mut overlay = std::mem::replace(
            &mut self.overlay,
            openpilot_ui_framework::overlay::PaintLayer::new(1),
        );
        let clock = self.context.now_monotonic.clone();
        let result = overlay.paint(
            draw,
            self.state.rect,
            key,
            state.control(frame.index),
            || clock(),
            |draw| {
                self.panel(draw)?;
                self.draw_wheel(frame, draw)?;
                common::badge(&self.context, draw, self.state.rect)
            },
        );
        self.overlay = overlay;
        result?;
        self.animate(draw)?;
        Ok(RenderResult::None)
    }
}
