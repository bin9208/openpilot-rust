//! Source: selfdrive/ui/layouts/sidebar.py (MIT).
use crate::{
    context::Context,
    paint::{self, Text},
    state::messages,
};
use openpilot_ui_framework::{
    assets::Texture,
    callback::Callback,
    canvas::Canvas,
    draw::{Draw, RoundedOutline, BLACK, WHITE},
    geometry::{Point, Rect},
    text::Font,
    text_layout::float,
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};

pub const WIDTH: f32 = 300.0;
pub const SETTINGS: Rect = Rect {
    x: 50.0,
    y: 35.0,
    width: 200.0,
    height: 117.0,
};
pub const HOME: Rect = Rect {
    x: 60.0,
    y: 860.0,
    width: 180.0,
    height: 180.0,
};
const WARNING: u32 = paint::color(218, 202, 37, 255);
const DANGER: u32 = paint::color(201, 34, 49, 255);
#[derive(Clone, Debug, serde::Serialize)]
pub struct Metric {
    pub label: &'static str,
    pub value: &'static str,
    pub color: u32,
}
impl Metric {
    fn new(label: &'static str, value: &'static str, color: u32) -> Self {
        Self {
            label,
            value,
            color,
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Status {
    pub network: &'static str,
    pub strength: u16,
    pub temperature: Metric,
    pub panda: Metric,
    pub connection: Metric,
    pub recording: bool,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            network: "--",
            strength: 0,
            temperature: Metric::new("TEMP", "GOOD", WHITE),
            panda: Metric::new("VEHICLE", "ONLINE", WHITE),
            connection: Metric::new("CONNECT", "OFFLINE", WARNING),
            recording: false,
        }
    }
}
struct StatusInput {
    network: u16,
    strength: u16,
    thermal: u16,
    ping: u64,
    now_ns: u128,
    panda: u16,
    recording: bool,
}
impl Status {
    fn update(&mut self, input: StatusInput) {
        let StatusInput {
            network,
            strength,
            thermal,
            ping,
            now_ns,
            panda,
            recording,
        } = input;
        self.network = match network {
            0 => "--",
            1 => "Wi-Fi",
            2 => "2G",
            3 => "3G",
            4 => "LTE",
            5 => "5G",
            6 => "ETH",
            _ => "Unknown",
        };
        self.strength = if strength > 0 {
            strength.saturating_add(1).min(5)
        } else {
            0
        };
        self.temperature = match thermal {
            0 => Metric::new("TEMP", "GOOD", WHITE),
            1 => Metric::new("TEMP", "OK", WARNING),
            _ => Metric::new("TEMP", "HIGH", DANGER),
        };
        self.connection = if ping == 0 {
            Metric::new("CONNECT", "OFFLINE", WARNING)
        } else if now_ns < u128::from(ping) + 80_000_000_000 {
            Metric::new("CONNECT", "ONLINE", WHITE)
        } else {
            Metric::new("CONNECT", "ERROR", DANGER)
        };
        self.panda = if panda == 0 {
            Metric::new("NO", "PANDA", DANGER)
        } else {
            Metric::new("VEHICLE", "ONLINE", WHITE)
        };
        self.recording = recording;
    }
}
pub struct Sidebar {
    pub state: WidgetState,
    context: Context,
    pub status: Status,
    pub monotonic_ns: std::rc::Rc<dyn Fn() -> Result<u128, Error>>,
    settings: Texture,
    home: Texture,
    mic: Texture,
    pub microphone_rect: Rect,
    pub on_settings: Option<Callback<()>>,
    pub on_carrot_web: Option<Callback<()>>,
    pub open_settings: Option<Callback<()>>,
}
impl Sidebar {
    pub fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        Ok(Self {
            state: WidgetState::default(),
            context,
            status: Status::default(),
            monotonic_ns: std::rc::Rc::new(|| {
                let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                let seconds = u128::try_from(time.tv_sec)
                    .map_err(|_| Error::Contract("negative monotonic seconds"))?;
                let nanos = u128::try_from(time.tv_nsec)
                    .map_err(|_| Error::Contract("negative monotonic nanoseconds"))?;
                Ok(seconds * 1_000_000_000 + nanos)
            }),
            settings: paint::texture(canvas, "images/button_settings.png", (200, 117))?,
            home: paint::texture(canvas, "icons/carrot_web.png", (180, 180))?,
            mic: paint::texture(canvas, "icons/microphone.png", (30, 30))?,
            microphone_rect: Rect::default(),
            on_settings: None,
            on_carrot_web: None,
            open_settings: None,
        })
    }
    fn metric(&self, draw: &mut dyn Draw, metric: &Metric, y: f32) -> Result<(), Error> {
        let rect = Rect {
            x: self.state.rect.x + 30.0,
            y,
            width: 240.0,
            height: 126.0,
        };
        draw.scissor(Some(Rect {
            x: (rect.x + 4.0).trunc(),
            y: rect.y.trunc(),
            width: 18.0,
            height: rect.height,
        }))?;
        let result = draw.rounded_segments(
            Rect {
                x: rect.x + 4.0,
                y: rect.y + 4.0,
                width: 100.0,
                height: 118.0,
            },
            0.3,
            10,
            metric.color,
            false,
        );
        draw.scissor(None)?;
        result?;
        draw.rounded_outline(
            rect,
            RoundedOutline {
                roundness: 0.3,
                segments: 10,
                thickness: 2.0,
                color: paint::color(255, 255, 255, 85),
            },
        )?;
        let mut y = f64::from(rect.y) + (63.0 - 2.0 * 35.0 * draw.font_scale());
        for label in [metric.label, metric.value] {
            let text = self.context.tr(label);
            let size = openpilot_ui_framework::text_layout::measure(
                draw,
                Font::SemiBold,
                &text,
                35.0,
                0.0,
            );
            y += f64::from(size.y);
            paint::text(
                draw,
                Point {
                    x: float(f64::from(rect.x) + 22.0 + (240.0 - 22.0 - f64::from(size.x)) / 2.0),
                    y: float(y),
                },
                Text {
                    value: &text,
                    font: Font::SemiBold,
                    size: 35.0,
                    spacing: 0.0,
                    color: WHITE,
                },
            )?;
        }
        Ok(())
    }
}
impl Widget for Sidebar {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let sm = self.context.messages.borrow();
        let sm = &sm.state;
        let result = (|| -> Result<(), crate::Error> {
            if !sm.topic("deviceState")?.updated {
                return Ok(());
            }
            let ds = messages::device_state(sm)?;
            let ui = self.context.ui.borrow();
            self.status.update(StatusInput {
                network: ds
                    .get_network_type()
                    .map(u16::from)
                    .unwrap_or_else(|capnp::NotInSchema(raw)| raw),
                strength: ds
                    .get_network_strength()
                    .map(u16::from)
                    .unwrap_or_else(|capnp::NotInSchema(raw)| raw),
                thermal: ds
                    .get_thermal_status()
                    .map(u16::from)
                    .unwrap_or_else(|capnp::NotInSchema(raw)| raw),
                ping: ds.get_last_athena_ping_time(),
                now_ns: (self.monotonic_ns)()?,
                panda: ui.panda_type,
                recording: ui.recording_audio,
            });
            Ok(())
        })();
        result.map_err(Error::from)
    }
    fn mouse_release(
        &mut self,
        position: Point,
        _: &Frame<'_>,
        _: &mut dyn Draw,
    ) -> Result<(), Error> {
        let callback = if SETTINGS.contains(position) {
            &self.on_settings
        } else if HOME.contains(position) {
            &self.on_carrot_web
        } else if self.status.recording && self.microphone_rect.contains(position) {
            &self.open_settings
        } else {
            return Ok(());
        };
        if let Some(callback) = callback {
            callback.call(());
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let rect = self.state.rect;
        draw.rounded(rect, 0.0, BLACK)?;
        let down = self.state.is_pressed() && frame.last_event.down;
        for (texture, destination) in [(self.settings, SETTINGS), (self.home, HOME)] {
            let tint = if down && destination.contains(frame.cursor) {
                paint::color(255, 255, 255, 166)
            } else {
                WHITE
            };
            paint::image(
                draw,
                paint::Image {
                    texture,
                    rect: Rect {
                        width: texture.width,
                        height: texture.height,
                        ..destination
                    },
                    tint,
                    origin: Point::default(),
                    rotation: 0.0,
                },
            )?;
        }
        if self.status.recording {
            self.microphone_rect = Rect {
                x: rect.x + rect.width - 130.0,
                y: rect.y + 245.0,
                width: 75.0,
                height: 40.0,
            };
            let color = if down && self.microphone_rect.contains(frame.cursor) {
                paint::color(201, 34, 49, 165)
            } else {
                DANGER
            };
            draw.rounded_segments(self.microphone_rect, 1.0, 10, color, false)?;
            paint::image(
                draw,
                paint::Image {
                    texture: self.mic,
                    rect: Rect {
                        x: self.microphone_rect.x + (75.0 - self.mic.width) / 2.0,
                        y: self.microphone_rect.y + (40.0 - self.mic.height) / 2.0,
                        width: self.mic.width,
                        height: self.mic.height,
                    },
                    tint: WHITE,
                    origin: Point::default(),
                    rotation: 0.0,
                },
            )?;
        }
        for i in 0..5u16 {
            draw.circle(
                Point {
                    x: (rect.x + 58.0 + f32::from(i) * 37.0 + 13.0).trunc(),
                    y: (rect.y + 196.0 + 13.0).trunc(),
                },
                13.0,
                if i < self.status.strength {
                    WHITE
                } else {
                    paint::color(84, 84, 84, 255)
                },
            )?;
        }
        paint::text(
            draw,
            Point {
                x: rect.x + 58.0,
                y: rect.y + 247.0,
            },
            Text {
                value: &self.context.tr(self.status.network),
                font: Font::Normal,
                size: 35.0,
                spacing: 0.0,
                color: WHITE,
            },
        )?;
        for (metric, offset) in [
            (&self.status.temperature, 338.0),
            (&self.status.panda, 496.0),
            (&self.status.connection, 654.0),
        ] {
            self.metric(draw, metric, rect.y + offset)?;
        }
        Ok(RenderResult::None)
    }
}
