use super::*;
use crate::onroad::model_renderer::math;
use crate::{
    onroad::hud::style::{self, Anchor, Text},
    paint::color,
    params::Read,
};
use chrono::Datelike;
use openpilot_ui_framework::{geometry::Point, text::Font, text_layout};
impl Hud {
    pub(super) fn side(
        &self,
        draw: &mut dyn Draw,
        wheel: Texture,
        position: [f64; 2],
    ) -> Result<(), Error> {
        let [x, y] = position;
        let time_x = x + f64::from(wheel.width) / 2.0 + 15.0;
        let mut right = time_x;
        let ui = self.context.ui.borrow();
        let date_mode = ui.slow.show_date_time;
        let debug = ui.slow.show_debug_ui;
        drop(ui);
        let now = (self.context.now_wall)();
        let weekdays = ["월", "화", "수", "목", "금", "토", "일"];
        let day = usize::try_from(now.weekday().num_days_from_monday())
            .map_err(|_| Error::Contract("HUD weekday"))?;
        let date = format!("{}({})", now.format("%m-%d"), weekdays[day]);
        let time = now.format("%H:%M").to_string();
        match date_mode {
            1 => {
                let time_font = (f64::from(wheel.height) * 1.05).trunc();
                let date_font = (time_font * 0.58).trunc().max(18.0);
                let time_size = text_layout::measure(draw, Font::Display, &time, time_font, 0.0);
                let date_size = text_layout::measure(draw, Font::Display, &date, date_font, 0.0);
                let gap = (time_font * 0.02).trunc().max(2.0);
                let total = f64::from(time_size.y) + gap + f64::from(date_size.y);
                let base_y = y - total / 2.0;
                let width = f64::from(time_size.x.max(date_size.x));
                common::styled(
                    draw,
                    &time,
                    [time_x + (width - f64::from(time_size.x)) / 2.0, base_y],
                    time_font,
                    color(255, 255, 255, 235),
                    Anchor::LeftTop,
                )?;
                common::styled(
                    draw,
                    &date,
                    [
                        time_x + (width - f64::from(date_size.x)) / 2.0,
                        base_y + f64::from(time_size.y) + gap,
                    ],
                    date_font,
                    color(255, 255, 255, 220),
                    Anchor::LeftTop,
                )?;
                right = time_x + width;
            }
            2 | 3 => {
                let (text, size, tint) = if date_mode == 2 {
                    (
                        &time,
                        (f64::from(wheel.height) * 1.1).trunc(),
                        color(255, 255, 255, 235),
                    )
                } else {
                    (
                        &date,
                        (f64::from(wheel.height) * 0.72).trunc(),
                        color(255, 255, 255, 220),
                    )
                };
                let measured = text_layout::measure(draw, Font::Display, text, size, 0.0);
                common::styled(
                    draw,
                    text,
                    [time_x, y - f64::from(measured.y) / 2.0],
                    size,
                    tint,
                    Anchor::LeftTop,
                )?;
                right = time_x + f64::from(measured.x);
            }
            _ => {}
        }
        if self.traffic(draw, [(right + 12.0).trunc(), y.trunc()])? || debug == 0 {
            return Ok(());
        }
        let messages = self.context.messages.borrow();
        let temps = messages::device_state(&messages.state)?
            .get_cpu_temp_c()
            .map_err(crate::Error::from)?;
        let cpu = if temps.is_empty() {
            "CPU: --".into()
        } else {
            format!(
                "CPU: {:.0}",
                temps.iter().map(f64::from).sum::<f64>() / f64::from(temps.len())
            )
        };
        let ratio = format!(
            "SR: {:.1}",
            messages::live_parameters(&messages.state)?.get_steer_ratio()
        );
        let road = messages::carrot_man(&messages.state)?
            .get_sz_pos_road_name()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let font = ((f64::from(wheel.height) * 1.1).trunc() * 0.33)
            .trunc()
            .max(18.0);
        let sizes = [&cpu, &ratio, road]
            .map(|text| text_layout::measure(draw, Font::Medium, text, font, 0.0));
        let gap = (font * 0.15).trunc().max(4.0);
        let total = f64::from(sizes[0].y)
            + gap
            + f64::from(sizes[1].y)
            + if road.is_empty() {
                0.0
            } else {
                gap + f64::from(sizes[2].y)
            };
        let mut line_y = y - total / 2.0;
        for (text, size) in [&cpu, &ratio, road].into_iter().zip(sizes) {
            if !text.is_empty() {
                style::text(
                    draw,
                    Text {
                        value: text,
                        position: [right + 25.0, line_y],
                        size: font,
                        font: Font::Display,
                        color: color(255, 255, 255, 210),
                        anchor: Anchor::LeftTop,
                        border: 1.0,
                        shadow: 8.0,
                        y_offset: 0.0,
                    },
                )?;
                line_y += f64::from(size.y) + gap;
            }
        }
        Ok(())
    }
    fn traffic_info(&self) -> Result<Option<(String, String)>, crate::Error> {
        let now = (self.context.now_monotonic)();
        if self.debug_traffic {
            let index = math::integer((now / 2.0).floor())?.rem_euclid(5);
            let (lamp, remain) = match index {
                0 => ("red", "13"),
                1 => ("green", "8"),
                2 => ("left", "7"),
                3 => ("right", "5"),
                _ => ("uturn", "4"),
            };
            return Ok(Some((lamp.into(), remain.into())));
        }
        let Some(raw) = self.context.memory.bytes("TrafficLight")? else {
            return Ok(None);
        };
        use openpilot_logmessaged::{JsonValue, JsonView};
        let Some(value) = std::str::from_utf8(&raw)
            .ok()
            .and_then(|raw| JsonValue::parse(raw).ok())
        else {
            return Ok(None);
        };
        let Some(lamp) = value.get("lamp").and_then(|value| value.to_utf8()) else {
            return Ok(None);
        };
        let lamp = openpilot_ui_framework::text::trim(&lamp);
        if !matches!(lamp, "red" | "green" | "left" | "right" | "uturn") {
            return Ok(None);
        }
        let remain = value
            .get("remain")
            .map_or(Some("0".into()), |value| match value.view() {
                JsonView::Text(_) => crate::params::typed::integer_text(&value.to_utf8()?),
                JsonView::Bool(value) => Some(i32::from(value).to_string()),
                JsonView::Integer(value) => Some(value.to_owned()),
                JsonView::Float(value) if value.is_finite() => {
                    Some(format!("{:.0}", value.trunc()))
                }
                JsonView::Null | JsonView::Float(_) | JsonView::Array(_) | JsonView::Object(_) => {
                    None
                }
            });
        let Some(remain) = remain.filter(|value| value != "0" && !value.starts_with('-')) else {
            return Ok(None);
        };
        let timestamp = value
            .get("ts")
            .map_or(Some(0.0), |value| match value.view() {
                JsonView::Text(_) => openpilot_hardware_info::parse_float(&value.to_utf8()?).ok(),
                JsonView::Bool(value) => Some(f64::from(value)),
                JsonView::Integer(value) => {
                    value.parse::<f64>().ok().filter(|value| value.is_finite())
                }
                JsonView::Float(value) => Some(value),
                JsonView::Null | JsonView::Array(_) | JsonView::Object(_) => None,
            });
        let Some(timestamp) = timestamp else {
            return Ok(None);
        };
        if timestamp > 0.0 && now - timestamp > 2.5 {
            return Ok(None);
        }
        Ok(Some((lamp.into(), remain.to_string())))
    }
    fn traffic(&self, draw: &mut dyn Draw, position: [f64; 2]) -> Result<bool, Error> {
        let Some((lamp, remain)) = self.traffic_info()? else {
            return Ok(false);
        };
        let [x, y] = position;
        let center = Point {
            x: math::float(x + 24.0),
            y: math::float(y),
        };
        match lamp.as_str() {
            "red" | "green" => {
                draw.circle(
                    center,
                    24.0,
                    if lamp == "red" {
                        color(255, 70, 70, 245)
                    } else {
                        color(0, 220, 80, 245)
                    },
                )?;
                draw.circle_lines(
                    (math::integer(x + 24.0)?, math::integer(y)?),
                    24.0,
                    color(255, 255, 255, 220),
                )?;
            }
            "left" | "right" | "uturn" => {
                let (text, tint) = match lamp.as_str() {
                    "left" => ("<-", color(0, 255, 100, 240)),
                    "right" => ("->", color(0, 255, 100, 240)),
                    _ => ("U", color(255, 220, 80, 240)),
                };
                style::text(
                    draw,
                    Text {
                        value: text,
                        position: [x + 24.0, y],
                        size: 48.0,
                        font: Font::Display,
                        color: tint,
                        anchor: Anchor::Center,
                        border: 1.0,
                        shadow: 8.0,
                        y_offset: 0.0,
                    },
                )?;
            }
            _ => return Ok(false),
        }
        let size = text_layout::measure(draw, Font::SemiBold, &remain, 28.0, 0.0);
        style::text(
            draw,
            Text {
                value: &remain,
                position: [x + 24.0 + 24.0 + 5.0, (y - f64::from(size.y) / 2.0).trunc()],
                size: 28.0,
                font: Font::Display,
                color: color(255, 255, 255, 235),
                anchor: Anchor::LeftTop,
                border: 1.0,
                shadow: 8.0,
                y_offset: 0.0,
            },
        )?;
        Ok(true)
    }
}
