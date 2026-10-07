use super::*;
use crate::paint::color;
use chrono::Datelike;
use openpilot_ui_framework::text_layout;
impl Hud {
    pub(super) fn date(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        if self.settings[1] <= 0 {
            return Ok(());
        }
        let now = (self.context.now_wall)();
        let weekdays = ["월", "화", "수", "목", "금", "토", "일"];
        let day = usize::try_from(now.weekday().num_days_from_monday())
            .map_err(|_| Error::Contract("HUD weekday"))?;
        let date = format!("{}({})", now.format("%m-%d"), weekdays[day]);
        let rect = self.state.rect;
        let x = (f64::from(rect.x) + 170.0).trunc();
        let y = (f64::from(rect.y) + 120.0).trunc();
        if matches!(self.settings[1], 1 | 2) {
            Self::text(
                draw,
                &now.format("%H:%M").to_string(),
                [x, y],
                100.0,
                common::WHITE,
                [3.0, 8.0],
            )?;
        }
        if matches!(self.settings[1], 1 | 3) {
            Self::text(draw, &date, [x, y + 70.0], 60.0, common::WHITE, [3.0, 8.0])?;
        }
        Ok(())
    }
    pub(super) fn tpms(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        if !matches!(self.settings[2], 1..=3) {
            return Ok(());
        }
        let sm = self.context.messages.borrow();
        let tpms = messages::car_state(&sm.state)?
            .get_tpms()
            .map_err(crate::Error::from)?;
        let rect = self.state.rect;
        let x = f64::from(rect.x) + f64::from(rect.width) - 125.0;
        let mut rows = Vec::with_capacity(2);
        if matches!(self.settings[2], 1 | 3) {
            rows.push(f64::from(rect.y) + 130.0);
        }
        if matches!(self.settings[2], 2 | 3) {
            rows.push(f64::from(rect.y) + f64::from(rect.height) - 125.0);
        }
        for y in rows {
            for (value, pos) in [
                (tpms.get_fl(), [x - 80.0, y - 55.0]),
                (tpms.get_fr(), [x + 80.0, y - 55.0]),
                (tpms.get_rl(), [x - 80.0, y + 70.0]),
                (tpms.get_rr(), [x + 80.0, y + 70.0]),
            ] {
                let value = f64::from(value);
                let missing = !(5.0..=60.0).contains(&value);
                let tint = if !missing && value < 31.0 {
                    color(255, 90, 90, 220)
                } else {
                    color(255, 255, 255, 220)
                };
                let text = if missing {
                    "  -".into()
                } else {
                    format!("{value:.0}")
                };
                Self::text(draw, &text, pos, 40.0, tint, [1.0, 4.0])?;
            }
        }
        Ok(())
    }
    pub(super) fn travel(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let rect = self.state.rect;
        if rect.width < 1200.0 {
            return Ok(());
        }
        let sm = self.context.messages.borrow();
        let man = messages::carrot_man(&sm.state)?;
        let distance = man.get_n_go_pos_dist();
        let remaining = man.get_n_go_pos_time();
        if distance <= 0 || remaining <= 0 {
            return Ok(());
        }
        let x = (f64::from(rect.x) + f64::from(rect.width) - 800.0).trunc();
        let y = (f64::from(rect.y) + f64::from(rect.height) - 250.0).trunc();
        Self::box_(
            draw,
            Rect {
                x: math::float(x),
                y: math::float(y - 60.0),
                width: 790.0,
                height: 300.0,
            },
            color(0, 0, 0, 120),
            (30.0 / 300.0, 12, 2.0, common::WHITE),
        )?;
        let tbt = man
            .get_sz_t_b_t_main_text()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        Self::left(
            draw,
            Text {
                value: tbt,
                position: [x + 20.0, y - 15.0],
                size: 40.0,
                font: Font::Bold,
                color: common::WHITE,
                anchor: Anchor::LeftTop,
                border: 2.0,
                shadow: 4.0,
                y_offset: 6.0,
            },
        )?;
        let turn = man.get_x_turn_info();
        if turn > 0 {
            let bx = x + 100.0;
            let by = y + 85.0;
            let atc = man
                .get_atc_type()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?;
            if !atc.is_empty() {
                Self::box_(
                    draw,
                    Rect {
                        x: math::float(bx - 80.0),
                        y: math::float(by - 90.0),
                        width: 160.0,
                        height: 230.0,
                    },
                    if atc.contains("prepare") {
                        color(0, 255, 0, 100)
                    } else {
                        color(0, 228, 48, 255)
                    },
                    (15.0 / 230.0, 8, 1.0, color(0, 0, 0, 255)),
                )?;
            }
            match turn {
                1..=4 | 7 => {
                    let index = usize::try_from(if turn == 7 { 4 } else { turn - 1 })
                        .map_err(|_| Error::Contract("HUD turn index"))?;
                    Self::image(
                        draw,
                        self.turns[index],
                        [bx - 70.0, by - 70.0, 140.0, 140.0],
                    )?;
                }
                _ => {
                    let text = match turn {
                        6 => "TG".into(),
                        8 => "목적지".into(),
                        _ => format!("감속:{turn}"),
                    };
                    Self::text(
                        draw,
                        &text,
                        [bx, by + 20.0],
                        35.0,
                        common::WHITE,
                        [2.0, 4.0],
                    )?;
                }
            }
            let turn_distance = man.get_x_dist_to_turn();
            if turn_distance > 0 {
                let metric = self.context.ui.borrow().realtime.value.is_metric;
                let text = if metric {
                    if turn_distance < 1000 {
                        format!("{turn_distance} m")
                    } else {
                        format!("{:.1} km", f64::from(turn_distance) / 1000.0)
                    }
                } else if turn_distance < 1609 {
                    format!("{} ft", math::integer(f64::from(turn_distance) * 3.28084)?)
                } else {
                    format!("{:.1} mi", f64::from(turn_distance) / 1609.344)
                };
                style::text(
                    draw,
                    Text {
                        value: &text,
                        position: [bx, by + 120.0],
                        size: 40.0,
                        font: Font::Bold,
                        color: common::WHITE,
                        anchor: Anchor::CenterBottom,
                        border: 2.0,
                        shadow: 4.0,
                        y_offset: 6.0,
                    },
                )?;
            }
        }
        let sdi = man
            .get_sz_sdi_descr()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let road = man
            .get_sz_pos_road_name()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        if !sdi.is_empty() {
            let size = text_layout::measure(draw, Font::Bold, sdi, 40.0, 0.0);
            let height = (f64::from(size.y) + 13.0).trunc().max(48.0);
            Self::box_(
                draw,
                Rect {
                    x: math::float(x + 190.0),
                    y: math::float(y + 200.0 - f64::from(size.y).trunc() - 2.0),
                    width: size.x.trunc() + 20.0,
                    height: math::float(height),
                },
                color(0, 228, 48, 255),
                (math::float(10.0 / height), 8, 0.0, common::WHITE),
            )?;
        }
        Self::left(
            draw,
            Text {
                value: if sdi.is_empty() { road } else { sdi },
                position: [x + 200.0, y + 200.0],
                size: 40.0,
                font: Font::Bold,
                color: common::WHITE,
                anchor: Anchor::LeftTop,
                border: 1.5,
                shadow: 3.0,
                y_offset: 6.0,
            },
        )?;
        let now = (self.context.now_wall)() + chrono::Duration::seconds(i64::from(remaining));
        let eta = format!(
            "도착: {:.1}분({})",
            f64::from(remaining) / 60.0,
            now.format("%H:%M")
        );
        let distance = if self.context.ui.borrow().realtime.value.is_metric {
            format!("{:.1}km", f64::from(distance) / 1000.0)
        } else {
            format!("{:.1}mile", f64::from(distance) / 1000.0 * 0.621371)
        };
        for (value, position) in [
            (&eta, [x + 190.0, y + 80.0]),
            (&distance, [x + 310.0, y + 130.0]),
        ] {
            Self::left(
                draw,
                Text {
                    value,
                    position,
                    size: 50.0,
                    font: Font::Bold,
                    color: common::WHITE,
                    anchor: Anchor::LeftTop,
                    border: 2.0,
                    shadow: 4.0,
                    y_offset: 6.0,
                },
            )?;
        }
        Ok(())
    }
    fn left(draw: &mut dyn Draw, text: Text<'_>) -> Result<(), Error> {
        style::text(draw, text)
    }
}
