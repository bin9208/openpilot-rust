use super::*;
use crate::onroad::hud::style::{self, Anchor, Text};
#[derive(Clone, Default, PartialEq)]
pub(super) struct Parameters {
    top_left: String,
    top_left_long: String,
    custom_sr: f64,
    bottom_left: String,
    bottom_right: String,
}
impl Parameters {
    fn read(context: &Context) -> Result<Self, crate::Error> {
        let name = context.params.string("CarName")?;
        let scc = context.params.integer("HyundaiCameraSCC")? > 0;
        let nnff = !context.params.string("NNFFModelName")?.is_empty();
        let top = format!("{name}{}", if scc { "(CAMERA SCC)" } else { "" });
        let long = if scc {
            top.clone()
        } else {
            format!("{name} - OP Long")
        };
        let custom_sr = context.params.float("CustomSR")? / 10.0;
        Ok(Self {
            top_left: format!("{top}{}", if nnff { ",NNFF" } else { "" }),
            top_left_long: format!("{long}{}", if nnff { ",NNFF" } else { "" }),
            custom_sr,
            bottom_left: context.params.string("GitBranch")?,
            bottom_right: context.memory.string("NetworkAddress")?,
        })
    }
}
fn status(status: Status) -> u32 {
    match status {
        Status::Disengaged => color(0x12, 0x28, 0x39, 255),
        Status::Override => color(0x89, 0x92, 0x8d, 255),
        Status::Engaged => color(0x16, 0x7f, 0x40, 255),
    }
}
impl Road {
    pub(super) fn border(&mut self, draw: &mut dyn Draw, rect: Rect) -> Result<(), Error> {
        let context = self.context.clone();
        let sm = context.messages.borrow();
        if !sm
            .state
            .topic("carState")
            .map_err(crate::Error::from)?
            .alive
        {
            draw.rounded_outline(
                rect,
                RoundedOutline {
                    roundness: 0.0,
                    segments: 0,
                    thickness: 30.0,
                    color: color(0, 0, 0, 255),
                },
            )?;
            let inner = Rect {
                x: float(f64::from(rect.x) + 30.0),
                y: float(f64::from(rect.y) + 30.0),
                width: rect.width - 60.0,
                height: rect.height - 60.0,
            };
            draw.rounded_outline(
                inner,
                RoundedOutline {
                    roundness: 0.12,
                    segments: 10,
                    thickness: 30.0,
                    color: status(context.ui.borrow().status),
                },
            )?;
            return Ok(());
        }
        let params = self
            .border_params
            .refresh((context.now_monotonic)(), || Parameters::read(&context));
        let car = messages::car_state(&sm.state)?;
        let ui = context.ui.borrow();
        let top = status(if car.get_steering_pressed() {
            Status::Override
        } else if ui.lat_active {
            Status::Engaged
        } else {
            Status::Disengaged
        });
        let bottom = status(ui.status);
        drop(ui);
        let [x, y, w, h] = [rect.x, rect.y, rect.width, rect.height].map(f64::from);
        let middle = y + h / 2.0;
        let top_height = (middle - 100.0 - y).max(0.0);
        let bottom_y = middle + 100.0;
        let bottom_height = (y + h - bottom_y).max(0.0);
        for (values, tint) in [
            ([x, y, w, 30.0], top),
            ([x, y, 30.0, top_height], top),
            ([x + w - 30.0, y, 30.0, top_height], top),
            ([x, bottom_y, 30.0, bottom_height], bottom),
            ([x + w - 30.0, bottom_y, 30.0, bottom_height], bottom),
            ([x, y + h - 30.0, w, 30.0], bottom),
        ] {
            draw.rounded(
                Rect {
                    x: float(values[0].trunc()),
                    y: float(values[1].trunc()),
                    width: float(values[2].trunc()),
                    height: float(values[3].trunc()),
                },
                0.0,
                tint,
            )?;
        }
        for (blink_x, enabled) in [
            (x, car.get_left_blinker()),
            (x + w - 30.0, car.get_right_blinker()),
        ] {
            let rect = Rect {
                x: float(blink_x),
                y: float(middle - 100.0),
                width: 30.0,
                height: 200.0,
            };
            draw.rounded_segments(
                rect,
                0.18,
                10,
                if enabled {
                    color(255, 161, 0, 255)
                } else {
                    color(0, 0, 0, 255)
                },
                false,
            )?;
            draw.rounded_outline(
                rect,
                RoundedOutline {
                    roundness: 0.18,
                    segments: 10,
                    thickness: 1.0,
                    color: top,
                },
            )?;
        }
        let mut top_right = Vec::with_capacity(3);
        if sm
            .state
            .topic("liveDelay")
            .map_err(crate::Error::from)?
            .alive
        {
            let delay = messages::live_delay(&sm.state)?;
            top_right.push(format!(
                "LD[{}%,{:.2}]",
                delay.get_cal_perc(),
                delay.get_lateral_delay()
            ));
        }
        if sm
            .state
            .topic("liveTorqueParameters")
            .map_err(crate::Error::from)?
            .alive
        {
            let torque = messages::live_torque(&sm.state)?;
            top_right.push(format!(
                "LT[{}%,{}]({:.2}/{:.2})",
                torque.get_cal_perc(),
                if torque.get_live_valid() { "ON" } else { "OFF" },
                torque.get_lat_accel_factor_filtered(),
                torque.get_friction_coefficient_filtered()
            ));
        }
        if sm
            .state
            .topic("liveParameters")
            .map_err(crate::Error::from)?
            .alive
        {
            top_right.push(format!(
                "SR({:.1},{:.1})",
                messages::live_parameters(&sm.state)?.get_steer_ratio(),
                params.custom_sr
            ));
        }
        let top_right = top_right.join(", ");
        let log = car
            .get_log_carrot()
            .map_err(crate::Error::from)?
            .to_str()
            .map_err(crate::Error::from)?;
        let lateral = messages::lateral_plan(&sm.state)?;
        let bottom = if sm
            .state
            .topic("lateralPlan")
            .map_err(crate::Error::from)?
            .alive
        {
            lateral
                .get_lat_debug_text()
                .map_err(crate::Error::from)?
                .to_str()
                .map_err(crate::Error::from)?
        } else {
            ""
        };
        let params_topic = sm.state.topic("carParams").map_err(crate::Error::from)?;
        let long = if params_topic.alive {
            match params_topic
                .event()
                .map_err(crate::Error::from)?
                .which()
                .map_err(crate::Error::from)?
            {
                openpilot_cereal::log_capnp::event::Which::CarParams(cp) => cp
                    .map_err(crate::Error::from)?
                    .get_openpilot_longitudinal_control(),
                _ => false,
            }
        } else {
            false
        };
        for (value, position, anchor) in [
            (log, [x + w / 2.0, y + 2.0], Anchor::CenterTop),
            (
                if long {
                    params.top_left_long.as_str()
                } else {
                    params.top_left.as_str()
                },
                [x + 30.0, y + 2.0],
                Anchor::LeftTop,
            ),
            (&top_right, [x + w - 30.0, y + 2.0], Anchor::RightTop),
            (bottom, [x + w / 2.0, y + h - 32.0], Anchor::CenterTop),
            (
                &params.bottom_left,
                [x + 30.0, y + h - 32.0],
                Anchor::LeftTop,
            ),
            (
                &params.bottom_right,
                [x + w - 30.0, y + h - 32.0],
                Anchor::RightTop,
            ),
        ] {
            style::text(
                draw,
                Text {
                    value,
                    position,
                    size: 30.0,
                    font: Font::Display,
                    color: color(255, 255, 255, 255),
                    anchor,
                    border: 3.0,
                    shadow: 8.0,
                    y_offset: 0.0,
                },
            )?;
        }
        Ok(())
    }
}
