use super::*;
use crate::onroad::model_renderer::math;
use crate::{
    onroad::hud::{
        common::{GREEN, WHITE},
        presentation::{ColorMode, Navigation, SetSpeed},
        style::{self, Anchor, Text},
    },
    paint::{color, Image},
    params::Read,
};
use openpilot_ui_framework::{
    draw::RoundedOutline,
    geometry::{Point, Rect},
    text::Font,
    text_layout,
};
impl Hud {
    pub(super) fn panel(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let rect = self.state.rect;
        let width = f64::from(self.speed_bg.width);
        let height = f64::from(self.speed_bg.height);
        let x = (f64::from(rect.x) + 10.0).trunc();
        let y = (f64::from(rect.y) + f64::from(rect.height) - height - 10.0).trunc();
        paint::image(
            draw,
            Image {
                texture: self.speed_bg,
                rect: Rect {
                    x: math::float(x),
                    y: math::float(y),
                    width: self.speed_bg.width,
                    height: self.speed_bg.height,
                },
                origin: Point::default(),
                rotation: 0.0,
                tint: WHITE,
            },
        )?;
        let speed = if self.debug_speed {
            "123".into()
        } else {
            format!("{:.0}", self.speed)
        };
        let measured = text_layout::measure(draw, Font::Display, &speed, 80.0, 0.0);
        style::text(
            draw,
            Text {
                value: &speed,
                position: [
                    x + 18.0,
                    (y + height * 0.48 - f64::from(measured.y) * 0.5).trunc() - 2.0,
                ],
                size: 80.0,
                font: Font::Display,
                color: WHITE,
                anchor: Anchor::LeftTop,
                border: 2.0,
                shadow: 3.0,
                y_offset: 0.0,
            },
        )?;
        let (mode, tint) = if self.debug_speed {
            ("safe".into(), GREEN)
        } else {
            common::driving_mode(&self.context)?
        };
        if !mode.is_empty() {
            let measured = text_layout::measure(draw, Font::SemiBold, &mode, 25.0, 0.0);
            common::styled(
                draw,
                &mode,
                [
                    x + 5.0,
                    (y + height * 0.05 - f64::from(measured.y) * 0.5 - 15.0).trunc(),
                ],
                25.0,
                tint,
                Anchor::LeftTop,
            )?;
        }
        let metric = self.context.ui.borrow().realtime.value.is_metric;
        let speed_scale = if metric { 1.0 } else { 0.621371 };
        let set = if self.debug_speed {
            "123".into()
        } else if self.engaged && self.cruise_set {
            format!("{:.0}", self.set_speed * speed_scale)
        } else {
            "--".into()
        };
        let measured = text_layout::measure(draw, Font::Display, &set, 40.0, 0.0);
        common::styled(
            draw,
            &set,
            [
                (x + width * 0.76 - f64::from(measured.x) * 0.5).trunc(),
                (y + height * 0.33 - f64::from(measured.y) * 0.5).trunc(),
            ],
            40.0,
            GREEN,
            Anchor::LeftTop,
        )?;
        let messages = self.context.messages.borrow();
        let man = messages::carrot_man(&messages.state)?;
        let source = man
            .get_desired_source()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let provider = man
            .get_decel_provider()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let override_ = SetSpeed {
            cruise_target: Some(f64::from(
                messages::longitudinal_plan(&messages.state)?.get_cruise_target(),
            )),
            desired_speed: Some(f64::from(man.get_desired_speed())),
            source,
            provider,
            set_speed_kph: self.set_speed,
            max_label: &self.context.tr("MAX"),
        }
        .compute();
        if override_.active {
            let (set, label, tint) = if self.debug_speed {
                ("111".into(), "vturn".into(), color(255, 165, 0, 230))
            } else {
                (
                    format!("{:.0}", override_.speed_kph * speed_scale),
                    override_.label,
                    match override_.speed_color_mode {
                        2 => color(255, 165, 0, 230),
                        3 => color(199, 125, 255, 230),
                        4 => color(244, 172, 54, 230),
                        _ => GREEN,
                    },
                )
            };
            for (text, size, ratio, shift) in [(&set, 40.0, 0.25, 0.0), (&label, 30.0, 0.10, -20.0)]
            {
                let measured = text_layout::measure(draw, Font::Display, text, size, 0.0);
                common::styled(
                    draw,
                    text,
                    [
                        (x + width * 0.90 - f64::from(measured.x) * 0.5 + 50.0).trunc(),
                        (y + height * ratio - f64::from(measured.y) * 0.5 + shift).trunc(),
                    ],
                    size,
                    tint,
                    Anchor::LeftTop,
                )?;
            }
        }
        let gap = match self.context.params.integer("LongitudinalPersonality") {
            Ok(value) => i64::from(value) + 1,
            Err(_) => 8,
        };
        common::styled(
            draw,
            &gap.to_string(),
            [(x + width * 0.90).trunc(), (y + height * 0.82).trunc()],
            28.0,
            WHITE,
            Anchor::Center,
        )?;
        let topic = messages
            .state
            .topic("carrotNavi")
            .map_err(crate::Error::from)?;
        let connected =
            topic.alive && topic.valid && messages::carrot_navi(&messages.state)?.get_connected();
        let remote = man
            .get_remote()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let navigation = Navigation {
            vehicle_available: man.get_vehicle_navi_available(),
            external_active: crate::onroad::hud::presentation::external_connected(
                remote, connected,
            ),
            owner: man
                .get_navi_owner()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?,
            lifecycle: man
                .get_navi_lifecycle()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?,
        };
        if let Some((text, mode)) = navigation.status() {
            style::text(
                draw,
                Text {
                    value: text,
                    position: [
                        (x + width * 0.60 - 26.0).trunc(),
                        (y + height * 0.82).trunc(),
                    ],
                    size: 26.0,
                    font: Font::Display,
                    color: if mode == ColorMode::Vehicle {
                        color(199, 125, 255, 230)
                    } else {
                        color(244, 172, 54, 230)
                    },
                    anchor: Anchor::LeftTop,
                    border: 1.0,
                    shadow: 8.0,
                    y_offset: 0.0,
                },
            )?;
        }
        drop(messages);
        let gear = common::gear(&self.context)?;
        let box_ = Rect {
            x: math::float((x + width - 44.0 - 14.0 + 70.0).trunc()),
            y: math::float((y + height * 0.50).trunc()),
            width: 44.0,
            height: 54.0,
        };
        draw.rounded_segments(box_, 0.2, 8, color(0, 0, 0, 120), false)?;
        draw.rounded_outline(
            box_,
            RoundedOutline {
                roundness: 0.2,
                segments: 8,
                thickness: 3.0,
                color: GREEN,
            },
        )?;
        let measured = text_layout::measure(draw, Font::Display, &gear, 44.0, 0.0);
        paint::text(
            draw,
            Point {
                x: box_.x + (box_.width - measured.x) * 0.5,
                y: box_.y + (box_.height - measured.y) * 0.5,
            },
            paint::Text {
                value: &gear,
                font: Font::Display,
                size: 44.0,
                color: WHITE,
                spacing: 0.0,
            },
        )
    }
}
