use crate::{
    context::Context,
    paint::{self, color, Text},
    render_diagnostics::{Clock, SystemClock},
    state::messages,
    vision_status::{DisplayState, Packet, Status},
};
use openpilot_ui_framework::{
    draw::Draw,
    geometry::{Point, Rect},
    text::Font,
    text_layout::{float, measure},
    widget::{Frame, RenderResult, Widget, WidgetState},
    Error,
};
use std::rc::Rc;
const GRAY: u32 = color(160, 168, 177, 255);
const CYAN: u32 = color(100, 220, 255, 255);
const AMBER: u32 = color(255, 215, 0, 255);

pub struct VisionRenderer {
    state: WidgetState,
    context: Context,
    packet: Option<Packet>,
    now_nanos: Rc<dyn Fn() -> i128>,
}
impl VisionRenderer {
    pub fn new(context: Context) -> Self {
        Self::with_clock(context, Rc::new(|| SystemClock.monotonic_ns()))
    }
    pub fn with_clock(context: Context, now_nanos: Rc<dyn Fn() -> i128>) -> Self {
        Self {
            state: WidgetState::default(),
            context,
            packet: None,
            now_nanos,
        }
    }
    pub fn display_state(&self) -> DisplayState {
        DisplayState::at(self.packet.as_ref(), (self.now_nanos)())
    }
    fn text(
        draw: &mut dyn Draw,
        text: &str,
        x: f64,
        y: f64,
        color: u32,
        size: f64,
        width: f64,
    ) -> Result<(), Error> {
        let measured = measure(draw, Font::Display, text, size, 0.0);
        let fitted = size * (width / f64::from(measured.x).max(1.0)).min(1.0);
        paint::text(
            draw,
            Point {
                x: float(x),
                y: float(y),
            },
            Text {
                value: text,
                font: Font::Display,
                size: fitted,
                color,
                spacing: 0.0,
            },
        )
    }
    fn lane(draw: &mut dyn Draw, x: f64, y: f64, lane: i32) -> Result<(), Error> {
        if lane < 0 {
            Self::text(draw, "?", x + 4.0, y - 1.0, GRAY, 16.0, 110.0)
        } else if lane == 1 {
            draw.line(
                Point {
                    x: float(x),
                    y: float(y + 14.0),
                },
                Point {
                    x: float(x + 12.0),
                    y: float(y),
                },
                2.0,
                u32::MAX,
            )
        } else {
            for offset in [0.0, 6.0, 12.0] {
                draw.line(
                    Point {
                        x: float(x + offset * 0.85),
                        y: float(y + 14.0 - offset),
                    },
                    Point {
                        x: float(x + (offset + 3.0) * 0.85),
                        y: float(y + 11.0 - offset),
                    },
                    2.0,
                    u32::MAX,
                )?;
            }
            Ok(())
        }
    }
}
impl Widget for VisionRenderer {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let ui = self.context.ui.borrow();
        if !ui.slow.share_data || !ui.started {
            self.packet = None;
        } else {
            let messages = self.context.messages.borrow();
            let topic = messages
                .state
                .topic("customReservedRawData0")
                .map_err(crate::Error::from)?;
            if topic.updated {
                self.packet = if topic.valid {
                    match Packet::parse(messages::vision_data(&messages.state)?) {
                        Ok(packet) => Some(packet),
                        Err(error @ crate::vision_status::Error::LatencyOverflow) => {
                            return Err(Error::Io(std::io::Error::other(error)));
                        }
                        Err(_) => None,
                    }
                } else {
                    None
                };
            }
        }
        Ok(())
    }
    fn paint(&mut self, _: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        let ui = self.context.ui.borrow();
        if !ui.started {
            return Ok(RenderResult::None);
        }
        let messages = self.context.messages.borrow();
        let topic = messages
            .state
            .topic("carState")
            .map_err(crate::Error::from)?;
        let fresh = topic.valid && topic.alive && topic.receive_frame >= ui.started_frame;
        let car = messages::car_state(&messages.state)?;
        let left = fresh && car.get_left_blindspot();
        let right = fresh && car.get_right_blindspot();
        let rect = self.state.rect;
        for (active, is_left) in [(left, true), (right, false)] {
            if active {
                let x = if is_left {
                    f64::from(rect.x) + 6.0
                } else {
                    f64::from(rect.x) + f64::from(rect.width) - 12.0
                };
                draw.rounded_segments(
                    Rect {
                        x: float(x),
                        y: float(f64::from(rect.y) + 78.0),
                        width: 6.0,
                        height: 56.0,
                    },
                    0.8,
                    6,
                    AMBER,
                    false,
                )?;
                let x = if is_left {
                    f64::from(rect.x) + 16.0
                } else {
                    f64::from(rect.x) + f64::from(rect.width) - 45.0
                };
                Self::text(draw, "BSD", x, f64::from(rect.y) + 84.0, AMBER, 12.0, 110.0)?;
            }
        }
        if !ui.slow.share_data {
            return Ok(RenderResult::None);
        }
        let state = self.display_state();
        let card = Rect {
            x: float(f64::from(rect.x) + f64::from(rect.width) - 98.0),
            y: float(f64::from(rect.y) + f64::from(rect.height) - 96.0),
            width: 84.0,
            height: 78.0,
        };
        draw.rounded_segments(card, 0.14, 6, color(0, 0, 0, 190), false)?;
        let status_color = match state.state {
            Status::Running => CYAN,
            Status::Stale => AMBER,
            Status::Waiting => GRAY,
        };
        let x = f64::from(card.x);
        let y = f64::from(card.y);
        Self::text(draw, "VISION", x + 6.0, y + 5.0, status_color, 10.0, 40.0)?;
        let status = if let Some(latency) = &state.latency_ms {
            format!(
                "{:.1}s",
                latency
                    .seconds()
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?
            )
        } else {
            self.context.tr(match state.state {
                Status::Running => "ON",
                Status::Waiting => "WAIT",
                Status::Stale => "STALE",
            })
        };
        Self::text(draw, &status, x + 49.0, y + 5.0, status_color, 10.0, 29.0)?;
        Self::text(draw, "L", x + 6.0, y + 24.0, GRAY, 13.0, 110.0)?;
        Self::lane(draw, x + 22.0, y + 23.0, state.left_lane)?;
        Self::text(draw, "R", x + 44.0, y + 24.0, GRAY, 13.0, 110.0)?;
        Self::lane(draw, x + 60.0, y + 23.0, state.right_lane)?;
        let (label, text, color) = if left || right {
            (
                if left && right {
                    "L+R"
                } else if left {
                    "L"
                } else {
                    "R"
                },
                "DETECTED",
                AMBER,
            )
        } else if fresh && !state.clear_side.is_empty() {
            (
                if state.clear_side == "left" { "L" } else { "R" },
                "NO DETECTION",
                CYAN,
            )
        } else {
            ("", "STANDBY", GRAY)
        };
        Self::text(
            draw,
            format!("BSD {label}").trim_end(),
            x + 6.0,
            y + 44.0,
            color,
            11.0,
            72.0,
        )?;
        Self::text(
            draw,
            &self.context.tr(text),
            x + 6.0,
            y + 59.0,
            color,
            11.0,
            72.0,
        )?;
        Ok(RenderResult::None)
    }
}
