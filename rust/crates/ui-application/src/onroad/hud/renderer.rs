mod panels;
mod plot;
mod travel;
use super::{
    common,
    style::{self, Anchor, Text},
};
use crate::{
    context::Context,
    onroad::{exp_button::ExpButton, model_renderer::math},
    paint,
    params::Read,
    state::messages,
};
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::{Draw, RoundedOutline},
    geometry::{Point, Rect},
    text::Font,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
#[derive(serde::Serialize)]
pub struct Snapshot {
    pub cruise_set: bool,
    pub cruise_available: bool,
    pub set_speed: f64,
    pub speed: f64,
    pub cluster_seen: bool,
    pub engaged: bool,
    pub animation_text: String,
    pub animation_time: i32,
    pub blink: i32,
    pub display: i32,
    pub settings: [i32; 5],
    pub params_next: f64,
}
pub struct Hud {
    state: WidgetState,
    context: Context,
    pub exp: ExpButton,
    cruise_set: bool,
    cruise_available: bool,
    set_speed: f64,
    speed: f64,
    cluster_seen: bool,
    engaged: bool,
    debug_speed: bool,
    speed_bg: Texture,
    traffic_red: Texture,
    traffic_green: Texture,
    turns: [Texture; 5],
    blink: i32,
    display: i32,
    cpu_temp: f64,
    memory_usage: i8,
    free_space: f64,
    voltage: f64,
    device_frames: Option<(i64, i64)>,
    params_next: f64,
    settings: [i32; 5],
    cruise_text_last: String,
    animation_text: String,
    animation_time: i32,
    plot: plot::Plot,
}
impl Hud {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut turns = Vec::with_capacity(5);
        for path in [
            "turn_l",
            "turn_r",
            "lane_change_l",
            "lane_change_r",
            "turn_u",
        ] {
            turns.push(canvas.texture(&format!("images/{path}.png"), Default::default())?);
        }
        Ok(Self {
            state: WidgetState::default(),
            exp: ExpButton::new(context.clone(), canvas, 192, 144)?,
            context,
            cruise_set: false,
            cruise_available: true,
            set_speed: 255.0,
            speed: 0.0,
            cluster_seen: false,
            engaged: false,
            debug_speed: false,
            speed_bg: canvas.texture("images/speed_bg.png", Default::default())?,
            traffic_red: canvas.texture("images/traffic_red.png", Default::default())?,
            traffic_green: canvas.texture("images/traffic_green.png", Default::default())?,
            turns: turns
                .try_into()
                .map_err(|_| Error::Contract("HUD turn textures"))?,
            blink: 0,
            display: 0,
            cpu_temp: 0.0,
            memory_usage: 0,
            free_space: 0.0,
            voltage: 0.0,
            device_frames: None,
            params_next: 0.0,
            settings: [0, 0, 1, 0, 7],
            cruise_text_last: String::new(),
            animation_text: String::new(),
            animation_time: -1,
            plot: plot::Plot::default(),
        })
    }
    pub fn user_interacting(&self) -> bool {
        self.exp.state().is_pressed()
    }
    pub fn set_debug(&mut self, enabled: bool) {
        self.debug_speed = enabled;
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            cruise_set: self.cruise_set,
            cruise_available: self.cruise_available,
            set_speed: self.set_speed,
            speed: self.speed,
            cluster_seen: self.cluster_seen,
            engaged: self.engaged,
            animation_text: self.animation_text.clone(),
            animation_time: self.animation_time,
            blink: self.blink,
            display: self.display,
            settings: self.settings,
            params_next: self.params_next,
        }
    }
    fn refresh(&mut self, now: f64) {
        if now < self.params_next {
            return;
        }
        let values = [
            "ShowDeviceState",
            "ShowDateTime",
            "ShowTpms",
            "ShowPlotMode",
        ]
        .map(|key| self.context.params.integer(key));
        let [Ok(device), Ok(date), Ok(tpms), Ok(plot)] = values else {
            return;
        };
        let personality = self.context.params.integer("LongitudinalPersonality");
        self.params_next = if personality.is_err() { now } else { now + 1.0 };
        self.settings = [device, date, tpms, plot, personality.unwrap_or(7)];
    }
    fn text(
        draw: &mut dyn Draw,
        value: &str,
        position: [f64; 2],
        size: f64,
        tint: u32,
        edges: [f64; 2],
    ) -> Result<(), Error> {
        style::text(
            draw,
            Text {
                value,
                position,
                size,
                font: Font::Display,
                color: tint,
                anchor: Anchor::CenterBottom,
                border: edges[0],
                shadow: edges[1],
                y_offset: 6.0,
            },
        )
    }
    fn box_(
        draw: &mut dyn Draw,
        rect: Rect,
        tint: u32,
        shape: (f32, i32, f32, u32),
    ) -> Result<(), Error> {
        draw.rounded_segments(rect, shape.0, shape.1, tint, false)?;
        if shape.2 > 0.0 {
            draw.rounded_outline(
                rect,
                RoundedOutline {
                    roundness: shape.0,
                    segments: shape.1,
                    thickness: shape.2,
                    color: shape.3,
                },
            )?;
        }
        Ok(())
    }
    fn image(draw: &mut dyn Draw, texture: Texture, geometry: [f64; 4]) -> Result<(), Error> {
        paint::image(
            draw,
            paint::Image {
                texture,
                rect: Rect {
                    x: math::float(geometry[0]),
                    y: math::float(geometry[1]),
                    width: math::float(geometry[2]),
                    height: math::float(geometry[3]),
                },
                tint: common::WHITE,
                origin: Point::default(),
                rotation: 0.0,
            },
        )
    }
    fn animate(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        if self.animation_time <= 0 || self.animation_text.is_empty() {
            return Ok(());
        }
        self.animation_time -= 12;
        let t = f64::from(self.animation_time.min(100));
        let rect = self.state.rect;
        let [x, y, w, h] = [rect.x, rect.y, rect.width, rect.height].map(f64::from);
        let position = [
            ((x + w / 2.0) * t + (x + 310.0) * (100.0 - t)) / 100.0,
            ((y + h - 400.0) * t + (y + h - 215.0) * (100.0 - t)) / 100.0,
        ]
        .map(f64::trunc);
        Self::text(
            draw,
            &self.animation_text,
            position,
            ((300.0 * t + 60.0 * (100.0 - t)) / 100.0).trunc(),
            paint::color(0, 203, 0, 255),
            if self.animation_time >= 100 {
                [9.0, 8.0]
            } else {
                [3.0, 0.0]
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
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let sm = self.context.messages.borrow();
        let frames = (
            sm.state
                .topic("deviceState")
                .map_err(crate::Error::from)?
                .receive_frame,
            sm.state
                .topic("peripheralState")
                .map_err(crate::Error::from)?
                .receive_frame,
        );
        if self.device_frames != Some(frames) {
            let device = messages::device_state(&sm.state)?;
            self.free_space = f64::from(device.get_free_space_percent());
            self.memory_usage = device.get_memory_usage_percent();
            let temps = device.get_cpu_temp_c().map_err(crate::Error::from)?;
            self.cpu_temp = if temps.is_empty() {
                0.0
            } else {
                temps.iter().map(f64::from).sum::<f64>() / f64::from(temps.len())
            };
            self.voltage = f64::from(messages::peripheral_state(&sm.state)?.get_voltage()) / 1000.0;
            self.device_frames = Some(frames);
        }
        if sm
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
        let car = messages::car_state(&sm.state)?;
        let cluster = f64::from(car.get_v_cruise_cluster());
        self.set_speed = if cluster == 0.0 {
            f64::from(
                messages::controls_state(&sm.state)?
                    .get_deprecated()
                    .get_v_cruise(),
            )
        } else {
            cluster
        };
        self.cruise_set = self.set_speed > 0.0 && self.set_speed < 255.0;
        self.cruise_available = self.set_speed != -1.0;
        self.engaged = messages::selfdrive_state(&sm.state)?.get_enabled();
        let cluster = f64::from(car.get_v_ego_cluster());
        self.cluster_seen |= cluster != 0.0;
        let speed = if self.cluster_seen {
            cluster
        } else {
            f64::from(car.get_v_ego())
        };
        self.speed = (speed
            * if self.context.ui.borrow().realtime.value.is_metric {
                3.6
            } else {
                3.6 * (1.0 / 1.609344)
            })
        .max(0.0);
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.refresh((self.context.now_monotonic)());
        let rect = self.state.rect;
        let top = paint::color(0, 0, 0, 114);
        draw.gradient(
            Rect {
                x: rect.x.trunc(),
                y: rect.y.trunc(),
                width: rect.width.trunc(),
                height: 300.0,
            },
            [top, 0, 0, top],
        )?;
        if self.cruise_available {
            self.panels(draw)?;
        }
        self.exp.set_rect(Rect {
            x: math::float(f64::from(rect.x) + f64::from(rect.width) - 30.0 - 192.0),
            y: math::float(f64::from(rect.y) + 30.0),
            width: 192.0,
            height: 192.0,
        });
        self.exp.render(frame, draw)?;
        self.plot
            .draw(&self.context, draw, rect, self.settings[3])?;
        self.date(draw)?;
        self.tpms(draw)?;
        common::badge(&self.context, draw, rect)?;
        self.animate(draw)?;
        Ok(RenderResult::None)
    }
}
