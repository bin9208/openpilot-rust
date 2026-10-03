use super::{
    flags as f,
    parser_inputs::Channel,
    state::State,
    state_fields::{self, float32, int16},
    wire::get,
    Error,
};
use num_traits::ToPrimitive;
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};

pub(super) fn pt(state: &mut State, input: (&str, &str), now: u64) -> Result<f64, Error> {
    state.inputs.signal(Channel::Pt, input.0, input.1, now)
}

pub fn update(state: &mut State, mut ret: car_state::Builder<'_>, now: u64) -> Result<(), Error> {
    let flags = state.config.flags;
    let brake = super::fd_powertrain::update(state, ret.reborrow(), now)?;
    state_fields::wheels(
        state,
        ret.reborrow(),
        (
            "WHEEL_SPEEDS",
            [
                "WHEEL_SPEED_1",
                "WHEEL_SPEED_2",
                "WHEEL_SPEED_3",
                "WHEEL_SPEED_4",
            ],
        ),
        now,
    )?;
    ret.set_brake_lights(
        brake
            || pt(state, ("TCS", "BrakeLight"), now)? == 1.
            || ret.reborrow_as_reader().get_a_ego() < -0.5,
    );
    ret.set_steering_rate_deg(float32(pt(
        state,
        ("STEERING_SENSORS", "STEERING_RATE"),
        now,
    )?)?);
    let angle = if flags & f::ANGLE_CONTROL != 0 {
        pt(state, ("MDPS", "STEERING_ANGLE_2"), now)?
    } else {
        pt(state, ("STEERING_SENSORS", "STEERING_ANGLE"), now)?
    };
    ret.set_steering_angle_deg(float32(-angle)?);
    let previous_trailer = state.trailer_connected;
    let trailer = state
        .inputs
        .captured("trailer_status")?
        .is_some_and(|data| {
            data.get("TRAILER_CONNECTED")
                .is_some_and(|value| *value != 0.)
        });
    if trailer {
        state.trailer_count = 0;
        state.trailer_connected = true;
    } else if state.trailer_connected {
        state.trailer_count += 1;
        if state.trailer_count > 500 {
            state.trailer_connected = false;
        }
    } else {
        state.trailer_count = 0;
    }
    ret.set_trailer_connected(state.trailer_connected);
    if state.trailer_connected != previous_trailer {
        state.inputs.diagnostics.prints.push(format!(
            "[TRAILER_DEBUG] connected={} timeout={}",
            if state.trailer_connected {
                "True"
            } else {
                "False"
            },
            state.trailer_count
        ));
    }
    ret.set_steering_torque(float32(pt(state, ("MDPS", "STEERING_COL_TORQUE"), now)?)?);
    ret.set_steering_torque_eps(float32(pt(state, ("MDPS", "STEERING_OUT_TORQUE"), now)?)?);
    let pressed =
        f64::from(ret.reborrow_as_reader().get_steering_torque()).abs() > state.limits.threshold;
    ret.set_steering_pressed(state.steering_pressed.update(pressed, 5));
    ret.set_steer_fault_temporary(
        pt(state, ("MDPS", "LKA_FAULT"), now)? != 0.
            || pt(state, ("MDPS", "LFA2_FAULT"), now)? != 0.,
    );
    let blink = state
        .inputs
        .captured("blinkers")?
        .or(state.inputs.captured("blinkers_alt")?);
    if let Some(data) = blink {
        let lamps = [
            get(&data, "LEFT_LAMP")? != 0. || get(&data, "LEFT_LAMP_ALT")? != 0.,
            get(&data, "RIGHT_LAMP")? != 0. || get(&data, "RIGHT_LAMP_ALT")? != 0.,
        ];
        let blink = state.blinkers.lamp(50, lamps[0], lamps[1]);
        ret.set_left_blinker(blink[0]);
        ret.set_right_blinker(blink[1]);
    }
    if state.config.bsm {
        if let Some(channel) = state.bsm_channel {
            let left =
                state
                    .inputs
                    .signal(channel, "BLINDSPOTS_REAR_CORNERS", "FL_INDICATOR", now)?
                    + state.inputs.signal(
                        channel,
                        "BLINDSPOTS_REAR_CORNERS",
                        "INDICATOR_LEFT_TWO",
                        now,
                    )?
                    + state.inputs.signal(
                        channel,
                        "BLINDSPOTS_REAR_CORNERS",
                        "INDICATOR_LEFT_FOUR",
                        now,
                    )?;
            let right =
                state
                    .inputs
                    .signal(channel, "BLINDSPOTS_REAR_CORNERS", "FR_INDICATOR", now)?
                    + state.inputs.signal(
                        channel,
                        "BLINDSPOTS_REAR_CORNERS",
                        "INDICATOR_RIGHT_TWO",
                        now,
                    )?
                    + state.inputs.signal(
                        channel,
                        "BLINDSPOTS_REAR_CORNERS",
                        "INDICATOR_RIGHT_FOUR",
                        now,
                    )?;
            ret.set_left_blindspot(left > 0.);
            ret.set_right_blindspot(right > 0.);
        } else if state.inputs.pt.seen_addresses.contains(&442) {
            state.bsm_channel = Some(Channel::Pt);
            state
                .inputs
                .diagnostics
                .prints
                .push("######## BSM in ECAN".into());
        } else if state.inputs.cam.seen_addresses.contains(&442) {
            state.bsm_channel = Some(Channel::Cam);
            state
                .inputs
                .diagnostics
                .prints
                .push("######## BSM in CAM".into());
        }
    }
    let previous_main = super::fd_buttons::main(state, now)?;
    super::fd_cruise::update(state, ret.reborrow(), now)?;
    super::fd_corner::update(state, ret.reborrow())?;
    let mut camera = false;
    if let Some(data) = state.inputs.captured("hda_info_4a3")? {
        let mut limit = get(&data, "SPEED_LIMIT")?;
        if !state.metric {
            limit *= 1.609344;
        }
        ret.set_speed_limit(float32(if limit < 255. { limit } else { 0. })?);
        camera = get(&data, "MapSource")?.trunc() == 2.;
        if state.time_zone.is_none() {
            let zone = super::local_time::country_zone(
                get(&data, "CountryCode")?.to_i32().ok_or(Error::Numeric)?,
            );
            state.time_zone = Some(zone);
            state.zone = Some(super::local_time::Zone::load(zone)?);
        }
    }
    if state.navigation.wrapped {
        let data = super::state_navigation::data(state)?;
        camera = state.navigation.pv5_camera_warning(&data.input())?;
    }
    let step = if state.capabilities.gear {
        pt(state, ("GEAR", "GEAR_STEP"), now)?
    } else {
        0.
    };
    ret.set_gear_step(int16(step)?);
    if (1..=8).contains(&ret.reborrow_as_reader().get_gear_step())
        && ret.reborrow_as_reader().get_gear_shifter()? == GearShifter::Unknown
    {
        ret.set_gear_shifter(GearShifter::Drive);
    }
    if state.capabilities.alt_gear {
        ret.set_gear_step(int16(pt(state, ("GEAR_ALT", "GEAR_STEP"), now)?)?);
    }
    let lanes = state
        .inputs
        .captured("cam_0x2a4")?
        .or(state.inputs.captured("cam_0x362")?);
    if let Some(data) = lanes {
        ret.set_left_lane_line(int16(
            get(&data, "LEFT_LANE_COLOR")? * 10. + get(&data, "LEFT_LANE_TYPE")?,
        )?);
        ret.set_right_lane_line(int16(
            get(&data, "RIGHT_LANE_COLOR")? * 10. + get(&data, "RIGHT_LANE_TYPE")?,
        )?);
    }
    if flags & f::EV != 0 {
        if let Some(data) = state.inputs.captured("manual_speed_limit_assist")? {
            ret.reborrow()
                .get_cruise_state()?
                .set_non_adaptive(get(&data, "MSLA_ENABLED")? == 1.);
        }
    }
    if state.capabilities.local_time {
        if let Some(zone) = &state.zone {
            state
                .inputs
                .signal(Channel::Pt, "LOCAL_TIME", "YEAR", now)?;
            if let Some(value) = super::local_time::timestamp(
                zone,
                &state.inputs.values(Channel::Pt, "LOCAL_TIME")?,
            )? {
                ret.set_datetime(value);
            }
        }
    }
    ret.set_acc_faulted(pt(state, ("TCS", "ACCEnable"), now)? != 0.);
    let speed = pt(state, ("CRUISE_BUTTONS_ALT", "CLU_SPEED"), now)? / 3.6;
    state_fields::cluster_speed(state, ret.reborrow(), speed)?;
    super::state_navigation::update(state, ret.reborrow(), camera, true)?;
    let events = super::fd_buttons::events(state, previous_main, now)?;
    state_fields::button_list(ret, &events)
}
