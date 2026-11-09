use super::{
    fd_stopping::apply_stopping,
    stopping::{CanfdStopping, ENTRY_SPEED},
    wire::{boolean, get, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;
use openpilot_control_policy::math::clip;

#[derive(Clone, Copy, serde::Deserialize)]
pub struct AccInput {
    pub bus: u8,
    pub enabled: bool,
    pub accel_last: f64,
    pub value_last: f64,
    pub accel: f64,
    pub stopping: bool,
    pub gas_override: bool,
    pub set_speed: f64,
    pub gap: f64,
    pub jerk_u: f64,
    pub jerk_l: f64,
    pub carrot_cruise: u8,
    pub carrot_accel: f64,
    pub lead: [f64; 4],
    #[serde(default)]
    pub signal_stop_object: bool,
    #[serde(default)]
    pub has_lead: bool,
}

pub struct SccState<'a> {
    pub original: Option<&'a Values>,
    pub wheels: [f64; 4],
    pub v_ego: f64,
    pub v_ego_raw: f64,
    pub a_ego: f64,
    pub brake: bool,
    pub gas: bool,
    pub brake_hold: bool,
    pub parking_brake: bool,
    pub can_valid: bool,
    pub can_timeout: bool,
    pub drive: bool,
    pub available: bool,
    pub standstill: bool,
    pub paddle: u8,
    pub soft_hold: bool,
    pub scc_hold: bool,
}

pub fn jerk_limit(accel: f64, previous: f64, limits: [f64; 2]) -> f64 {
    clip(
        accel,
        previous - limits[1].max(0.) * 0.02,
        previous + limits[0].max(0.) * 0.02,
    )
}

/// A low-speed signal-stop CAN object, never a radar/planner observation.
/// Returns whether the final, already-interlocked packet contains the object.
fn apply_signal_stop_object(data: &mut Values, state: &SccState<'_>, input: &AccInput) -> Result<bool, Error> {
    let [fl, fr, rl, rr] = state.wheels;
    let speeds = [state.v_ego, state.v_ego_raw, fl, fr, rl, rr];
    if !input.signal_stop_object
        || get(data, "ACCMode")? != 1.
        || get(data, "StopReq")? != 1.
        || !state.can_valid
        || state.can_timeout
        || !state.drive
        || state.gas
        || state.brake
        || state.brake_hold
        || state.parking_brake
        || !speeds.iter().all(|value| value.is_finite())
        || speeds.iter().any(|value| value.abs() > ENTRY_SPEED)
        || input.has_lead
    {
        return Ok(false);
    }
    // A stationary object has zero absolute speed, not zero relative speed while
    // ego creeps. This is intentionally local to the outgoing SCC object fields.
    set(
        data,
        &[
            ("ACC_ObjDist", 3.),
            ("ACC_ObjLatPos", 0.),
            ("ACC_ObjRelSpd", -state.v_ego),
            ("HUD_LEAD_INFO", 2.),
        ],
    );
    Ok(true)
}

pub fn camera_acc(
    writer: &mut CanWriter,
    state: &SccState<'_>,
    mut controller: Option<&mut CanfdStopping>,
    input: &AccInput,
) -> Result<(Option<Frame>, f64), Error> {
    if let Some(controller) = controller.as_deref_mut() {
        controller.stop_object_active = false;
    }
    let Some(original) = state.original else {
        if let Some(controller) = controller {
            controller.reset();
        }
        return Ok((None, input.value_last));
    };
    let has_controller = controller.is_some();
    let interlock = state.brake_hold || state.parking_brake;
    let soft_hold = state.soft_hold && state.available;
    let control_enabled =
        (input.enabled || soft_hold) && state.available && state.paddle == 0 && !interlock;
    let mut enabled = control_enabled;
    let mut mode = if !enabled {
        0.
    } else if input.gas_override {
        2.
    } else {
        1.
    };
    let mut accel = input.accel;
    let mut previous = input.value_last;
    match input.carrot_cruise {
        1 => {
            mode = if enabled { 4. } else { 0. };
            enabled = false;
            accel = 0.5;
            previous = 0.5;
        }
        2 => {
            accel = input.carrot_accel;
            previous = input.carrot_accel;
        }
        _ => {}
    }
    let jerk_u = if input.stopping || soft_hold {
        2.
    } else {
        input.jerk_u
    };
    let (raw, value) = if !enabled || input.gas_override {
        (0., 0.)
    } else {
        (accel, jerk_limit(accel, previous, [jerk_u, input.jerk_l]))
    };
    let mut data = original.clone();
    let _received_counter = data.remove("COUNTER");
    set(
        &mut data,
        &[
            ("ACCMode", mode),
            ("MainMode_ACC", 1.),
            (
                "StopReq",
                boolean(control_enabled && (input.stopping || soft_hold)),
            ),
            ("aReqValue", value),
            ("aReqRaw", raw),
            ("VSetDis", input.set_speed),
            ("JerkLowerLimit", if enabled { input.jerk_l } else { 1. }),
            ("JerkUpperLimit", jerk_u),
            ("DISTANCE_SETTING", input.gap),
            ("DriveMode", 0.),
            ("DriverAlert", 0.),
            ("ACC_ObjDist", input.lead[0]),
            ("ACC_ObjLatPos", input.lead[1]),
            ("ACC_ObjRelSpd", input.lead[2]),
            ("HUD_LEAD_INFO", input.lead[3]),
            (
                "TARGET_DISTANCE",
                if state.v_ego.is_finite() {
                    state.v_ego + 4.
                } else {
                    4.
                },
            ),
            ("TakeOverReq", 0.),
            ("SysFailState", 0.),
            ("AccelLimitBandUpper", 0.),
            ("AccelLimitBandLower", 0.),
            ("ZEROS_7", if has_controller { 0. } else { 1. }),
        ],
    );
    if has_controller {
        data.insert("InfoDisplay".into(), 0.);
    } else if get(&data, "InfoDisplay")? != 5. {
        data.insert(
            "InfoDisplay".into(),
            if !interlock && input.stopping && state.a_ego > -0.3 {
                4.
            } else {
                0.
            },
        );
    }
    let stopping_input = AccInput { accel, ..*input };
    apply_stopping(
        &mut data,
        state,
        (controller.as_deref_mut(), &stopping_input, jerk_u, false),
    )?;
    if let Some(controller) = controller {
        // State-only fallback for no-lead stops (OEM SCC_ObjSta). The requested
        // signal-stop object additionally supplies range and speed.
        if get(&data, "ACCMode")? == 1.
            && get(&data, "StopReq")? == 1.
            && get(&data, "HUD_LEAD_INFO")? == 0.
        {
            data.insert("HUD_LEAD_INFO".into(), 2.);
        }
        controller.stop_object_active = apply_signal_stop_object(&mut data, state, input)?;
    }
    let value = get(&data, "aReqValue")?;
    Ok((
        Some(writer.frame("SCC_CONTROL", input.bus, &data, None)?),
        value,
    ))
}

pub fn acc(
    writer: &mut CanWriter,
    state: &SccState<'_>,
    controller: Option<&mut CanfdStopping>,
    input: &AccInput,
) -> Result<(Frame, f64), Error> {
    let interlock = state.brake_hold || state.parking_brake;
    let soft_hold = state.soft_hold && state.available;
    let enabled = (input.enabled || soft_hold) && state.available && !interlock;
    let (raw, value) = if !enabled || input.gas_override {
        (0., 0.)
    } else {
        (
            input.accel,
            clip(input.accel, input.accel_last - 0.1, input.accel_last + 0.1),
        )
    };
    let mut data = values(&[
        (
            "ACCMode",
            if !enabled {
                0.
            } else if input.gas_override {
                2.
            } else {
                1.
            },
        ),
        ("MainMode_ACC", 1.),
        ("StopReq", boolean(enabled && (input.stopping || soft_hold))),
        ("aReqValue", value),
        ("aReqRaw", raw),
        ("VSetDis", input.set_speed),
        ("JerkLowerLimit", if enabled { input.jerk_l } else { 1. }),
        ("JerkUpperLimit", input.jerk_u),
        ("ACC_ObjDist", 1.),
        ("NSCCOper", 0.),
        ("NSCCOnOff", 2.),
        ("DriveMode", 0.),
        ("ACC_ObjLatPos", 100.),
        ("DISTANCE_SETTING", input.gap),
        (
            "InfoDisplay",
            if controller.is_none() && !interlock && input.stopping && state.standstill {
                4.
            } else {
                0.
            },
        ),
        ("ZEROS_7", 0.),
    ]);
    apply_stopping(&mut data, state, (controller, input, input.jerk_u, true))?;
    let value = get(&data, "aReqValue")?;
    Ok((writer.frame("SCC_CONTROL", input.bus, &data, None)?, value))
}
