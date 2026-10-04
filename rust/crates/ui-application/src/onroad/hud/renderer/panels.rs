use super::*;
use crate::{
    onroad::hud::presentation::{Navigation, SetSpeed},
    paint::color,
};
impl Hud {
    pub(super) fn panels(&mut self, draw: &mut dyn Draw) -> Result<(), Error> {
        self.blink = (self.blink + 1) % 16;
        self.display = (self.display + 1) % 64;
        let rect = self.state.rect;
        let x = (f64::from(rect.x) + 140.0).trunc();
        let y = (f64::from(rect.y) + f64::from(rect.height) - 230.0).trunc();
        let context = self.context.clone();
        let sm = context.messages.borrow();
        let man = messages::carrot_man(&sm.state)?;
        let limit = man.get_x_spd_limit();
        let sign = 0;
        let road_limit = man.get_n_road_limit_speed();
        let cam_detected = limit > 0 && !matches!(sign, 22 | 4);
        let bg = if cam_detected && self.blink > 8 {
            color(255, 0, 0, 180)
        } else {
            color(0, 0, 0, 90)
        };
        let device = self.settings[0] > 0;
        Self::box_(
            draw,
            Rect {
                x: math::float(x - 120.0),
                y: math::float(y - if device { 270.0 } else { 130.0 }),
                width: 475.0,
                height: if device { 495.0 } else { 355.0 },
            },
            bg,
            (
                if device { 30.0 / 495.0 } else { 30.0 / 355.0 },
                12,
                2.0,
                common::WHITE,
            ),
        )?;
        let red = man.get_traffic_state() == 1;
        let green = man.get_traffic_state() == 2;
        let (red_size, green_size) = (64.0, 64.0);
        if red {
            Self::image(
                draw,
                self.traffic_red,
                [
                    x - red_size / 2.0,
                    y + 270.0 - red_size / 2.0,
                    red_size,
                    red_size,
                ],
            )?;
        } else if green {
            Self::image(
                draw,
                self.traffic_green,
                [
                    x - green_size / 2.0,
                    y + 270.0 - green_size / 2.0,
                    green_size,
                    green_size,
                ],
            )?;
        }
        Self::image(draw, self.speed_bg, [x - 100.0, y - 60.0, 350.0, 150.0])?;
        let speed = if self.debug_speed {
            "123".into()
        } else {
            format!("{:.0}", self.speed)
        };
        Self::text(
            draw,
            &speed,
            [x, y + 50.0],
            120.0,
            common::WHITE,
            [3.0, 8.0],
        )?;
        let metric = self.context.ui.borrow().realtime.value.is_metric;
        let scale = if metric { 1.0 } else { 0.621371 };
        let cruise = if self.engaged && self.cruise_set {
            format!("{:.0}", self.set_speed * scale)
        } else {
            "--".into()
        };
        if cruise != self.cruise_text_last {
            self.cruise_text_last = cruise.clone();
            if cruise != "--" {
                self.animation_text = cruise.clone();
                self.animation_time = 120;
            }
        }
        Self::text(
            draw,
            &cruise,
            [x + 170.0, y + 15.0],
            60.0,
            color(0, 203, 0, 255),
            [1.0, 5.0],
        )?;
        let override_ = SetSpeed {
            cruise_target: Some(f64::from(
                messages::longitudinal_plan(&sm.state)?.get_cruise_target(),
            )),
            desired_speed: Some(f64::from(man.get_desired_speed())),
            source: man
                .get_desired_source()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?,
            provider: man
                .get_decel_provider()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?,
            set_speed_kph: self.set_speed,
            max_label: &self.context.tr("MAX"),
        }
        .compute();
        if override_.active {
            let tint = match override_.speed_color_mode {
                2 => color(255, 165, 0, 230),
                3 => color(199, 125, 255, 230),
                4 => color(244, 172, 54, 230),
                _ => color(0, 228, 48, 255),
            };
            let (value, label) = if self.debug_speed {
                ("111".into(), "vturn".into())
            } else {
                (
                    format!("{:.0}", override_.speed_kph * scale),
                    override_.label,
                )
            };
            Self::text(draw, &value, [x + 250.0, y - 45.0], 50.0, tint, [1.0, 5.0])?;
            Self::text(draw, &label, [x + 250.0, y - 100.0], 30.0, tint, [1.0, 5.0])?;
        }
        let (mode, tint) = if self.debug_speed {
            ("safe".into(), color(255, 165, 0, 230))
        } else {
            let (text, tint) = match messages::longitudinal_plan(&sm.state)?.get_my_driving_mode() {
                1 => ("eco", color(0, 255, 0, 200)),
                2 => ("safe", color(255, 165, 0, 200)),
                3 => ("norm", color(255, 255, 255, 200)),
                4 => ("high", color(255, 0, 0, 200)),
                _ => ("", color(255, 255, 255, 200)),
            };
            (self.context.tr(text), tint)
        };
        if !mode.is_empty() {
            Self::box_(
                draw,
                Rect {
                    x: math::float(x - 105.0),
                    y: math::float(y + 137.0),
                    width: 110.0,
                    height: 48.0,
                },
                tint,
                (0.25, 8, 2.0, common::WHITE),
            )?;
            Self::text(
                draw,
                &mode,
                [x - 50.0, y + 173.0],
                32.0,
                common::WHITE,
                [2.0, 4.0],
            )?;
            if messages::gps_location(&sm.state)?.get_has_fix() {
                Self::text(
                    draw,
                    "GPS",
                    [x - 50.0, y + 130.0],
                    30.0,
                    color(0, 228, 48, 255),
                    [2.0, 4.0],
                )?;
            }
        }
        let gap = i64::from(self.settings[4]) + 1;
        Self::text(
            draw,
            &gap.to_string(),
            [x + 220.0, y + 77.0],
            40.0,
            common::WHITE,
            [2.0, 4.0],
        )?;
        for i in 0..gap.clamp(0, 4) {
            let i = i32::try_from(i).map_err(|_| Error::Contract("HUD gap"))?;
            Self::box_(
                draw,
                Rect {
                    x: math::float(x + 270.0),
                    y: math::float(y + 185.0 - 20.0 * f64::from(i + 1) + 2.0),
                    width: 70.0,
                    height: 18.0,
                },
                color(0, 255, 0, 210),
                (0.12, 4, 2.0, common::WHITE),
            )?;
        }
        Self::box_(
            draw,
            Rect {
                x: math::float(x + 270.0),
                y: math::float(y - 10.0),
                width: 70.0,
                height: 80.0,
            },
            color(0, 255, 0, 210),
            (0.2, 8, 3.0, common::WHITE),
        )?;
        Self::text(
            draw,
            &common::gear(&self.context)?,
            [x + 305.0, y + 65.0],
            70.0,
            common::WHITE,
            [2.0, 4.0],
        )?;
        let lifecycle = man
            .get_navi_lifecycle()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let status = if lifecycle.is_empty() {
            None
        } else {
            Navigation {
                vehicle_available: man.get_vehicle_navi_available(),
                external_active: false,
                owner: man
                    .get_navi_owner()
                    .map_err(crate::Error::from)?
                    .to_str()
                    .map_err(crate::Error::from)?,
                lifecycle,
            }
            .status()
        };
        let active = if lifecycle.is_empty() {
            man.get_active_carrot()
        } else {
            0
        };
        if status.is_some() || active >= 1 {
            let (label, size, tint) = if let Some((label, _)) = status {
                (label, 29.0, color(0, 228, 48, 255))
            } else if active >= 2 {
                ("APN", 40.0, color(0, 228, 48, 255))
            } else {
                ("APM", 40.0, color(0, 120, 255, 210))
            };
            Self::box_(
                draw,
                Rect {
                    x: math::float(x + 145.0),
                    y: math::float(y + 137.0),
                    width: 110.0,
                    height: 48.0,
                },
                tint,
                (0.25, 8, 2.0, common::WHITE),
            )?;
            Self::text(
                draw,
                label,
                [x + 200.0, y + 175.0],
                size,
                common::WHITE,
                [2.0, 4.0],
            )?;
        }
        let (label, display, tint) = if limit > 0 && sign != 22 {
            (
                "CAM",
                Some(if metric {
                    limit
                } else {
                    math::integer(f64::from(limit) * 0.621371 + 0.5)?
                }),
                if self.blink <= 8 {
                    color(255, 0, 0, 210)
                } else {
                    color(255, 255, 0, 210)
                },
            )
        } else if road_limit > 0 {
            let limit = if metric {
                road_limit
            } else {
                math::integer(f64::from(road_limit) * 0.621371 + 0.5)?
            };
            (
                "LIMIT",
                Some(limit),
                if self.speed > f64::from(limit) + 2.0 {
                    color(255, 0, 0, 210)
                } else {
                    color(255, 255, 255, 210)
                },
            )
        } else {
            ("LIMIT", None, color(255, 255, 255, 210))
        };
        Self::text(
            draw,
            label,
            [x + 75.0, y + 130.0],
            30.0,
            common::WHITE,
            [2.0, 4.0],
        )?;
        Self::box_(
            draw,
            Rect {
                x: math::float(x + 20.0),
                y: math::float(y + 137.0),
                width: 110.0,
                height: 48.0,
            },
            tint,
            (0.25, 8, 2.0, common::WHITE),
        )?;
        Self::text(
            draw,
            &display.map_or_else(|| "--".into(), |value| value.to_string()),
            [x + 75.0, y + 175.0],
            40.0,
            common::WHITE,
            [2.0, 4.0],
        )?;
        if device {
            self.device_panels(draw, [x, y])?;
        }
        drop(sm);
        self.travel(draw)
    }
    fn device_panels(&self, draw: &mut dyn Draw, position: [f64; 2]) -> Result<(), Error> {
        let [x, y] = position;
        let disk = self.display < 32;
        let cards = [
            (
                "CPU",
                format!("{:.0}°C", self.cpu_temp),
                self.cpu_temp > 80.0,
            ),
            (
                "MEM",
                format!("{}%", self.memory_usage),
                self.memory_usage > 85,
            ),
            (
                if disk { "DISK" } else { "VOLT" },
                if disk {
                    format!("{:.0}%", 100.0 - self.free_space)
                } else {
                    format!("{:.1}V", self.voltage)
                },
                false,
            ),
        ];
        for (i, (label, value, hot)) in cards.iter().enumerate() {
            let offset =
                f64::from(i32::try_from(i).map_err(|_| Error::Contract("HUD device card index"))?)
                    * 150.0;
            let dx = x - 35.0 + offset;
            let dy = y - 200.0;
            Self::box_(
                draw,
                Rect {
                    x: math::float(dx - 65.0),
                    y: math::float(dy - 38.0),
                    width: 130.0,
                    height: 90.0,
                },
                if *hot && self.blink <= 8 {
                    color(255, 0, 0, 255)
                } else {
                    color(0, 255, 0, 190)
                },
                (0.16, 8, 2.0, common::WHITE),
            )?;
            Self::text(draw, label, [dx, dy - 5.0], 25.0, common::WHITE, [1.0, 4.0])?;
            Self::text(
                draw,
                value,
                [dx, dy + 40.0],
                40.0,
                common::WHITE,
                [1.0, 4.0],
            )?;
        }
        Ok(())
    }
}
