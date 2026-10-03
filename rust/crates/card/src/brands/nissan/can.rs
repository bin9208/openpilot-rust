use super::{Candidate, Error};
use crate::brands::hyundai::wire::crc8;
use num_traits::ToPrimitive;
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control;
use std::collections::BTreeMap;

type Values = BTreeMap<String, f64>;
fn frame(packer: &mut Packer, name: &str, bus: u8, values: &[(&str, f64)]) -> Result<Frame, Error> {
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, values, None)?,
        bus,
    })
}
fn copied<'a>(message: &Values, names: &'a [&'a str]) -> Result<Vec<(&'a str, f64)>, Error> {
    names
        .iter()
        .map(|name| {
            Ok((
                *name,
                message
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).to_owned()))?,
            ))
        })
        .collect()
}
pub fn steering(
    packer: &mut Packer,
    angle: f64,
    count: u64,
    active: bool,
    torque: f64,
) -> Result<Frame, Error> {
    let mut values = vec![
        ("COUNTER", (count % 16).to_f64().ok_or(Error::Numeric)?),
        ("DESIRED_ANGLE", angle),
        ("SET_0x80_2", 128.),
        ("SET_0x80", 128.),
        ("MAX_TORQUE", if active { torque } else { 0. }),
        ("LKA_ACTIVE", f64::from(active)),
    ];
    let data = packer.pack("LKAS", &values, None)?;
    values.push(("CHECKSUM", f64::from(crc8(&data[..7], 0x1d, 0xff, 0xff))));
    frame(packer, "LKAS", 0, &values)
}
pub(super) fn acc_cancel(
    packer: &mut Packer,
    candidate: Candidate,
    message: &Values,
) -> Result<Frame, Error> {
    let mut values = copied(
        message,
        &[
            "COUNTER",
            "PROPILOT_BUTTON",
            "CANCEL_BUTTON",
            "GAS_PEDAL_INVERTED",
            "SET_BUTTON",
            "RES_BUTTON",
            "FOLLOW_DISTANCE_BUTTON",
            "NO_BUTTON_PRESSED",
            "GAS_PEDAL",
            "USER_BRAKE_PRESSED",
            "NEW_SIGNAL_2",
            "GAS_PRESSED_INVERTED",
            "unsure1",
            "unsure2",
            "unsure3",
        ],
    )?;
    for (name, value) in &mut values {
        match *name {
            "CANCEL_BUTTON" => *value = 1.,
            "NO_BUTTON_PRESSED"
            | "PROPILOT_BUTTON"
            | "SET_BUTTON"
            | "RES_BUTTON"
            | "FOLLOW_DISTANCE_BUTTON" => *value = 0.,
            _ => {}
        }
    }
    frame(
        packer,
        "CRUISE_THROTTLE",
        if candidate.altima() { 1 } else { 2 },
        &values,
    )
}
pub fn leaf_cancel(packer: &mut Packer, message: &Values, cancel: bool) -> Result<Frame, Error> {
    let mut values = copied(
        message,
        &[
            "CANCEL_SEATBELT",
            "NEW_SIGNAL_1",
            "NEW_SIGNAL_2",
            "NEW_SIGNAL_3",
        ],
    )?;
    if cancel {
        values[0].1 = 1.;
    }
    frame(packer, "CANCEL_MSG", 2, &values)
}
pub fn hud(
    packer: &mut Packer,
    message: &Values,
    hud: h_u_d_control::Reader<'_>,
    enabled: bool,
) -> Result<Frame, Error> {
    let mut values = copied(
        message,
        &[
            "LARGE_WARNING_FLASHING",
            "SIDE_RADAR_ERROR_FLASHING1",
            "SIDE_RADAR_ERROR_FLASHING2",
            "LEAD_CAR",
            "LEAD_CAR_ERROR",
            "FRONT_RADAR_ERROR",
            "FRONT_RADAR_ERROR_FLASHING",
            "SIDE_RADAR_ERROR_FLASHING3",
            "LKAS_ERROR_FLASHING",
            "SAFETY_SHIELD_ACTIVE",
            "RIGHT_LANE_GREEN_FLASH",
            "LEFT_LANE_GREEN_FLASH",
            "FOLLOW_DISTANCE",
            "AUDIBLE_TONE",
            "SPEED_SET_ICON",
            "SMALL_STEERING_WHEEL_ICON",
            "unknown59",
            "unknown55",
            "unknown26",
            "unknown28",
            "unknown31",
            "SET_SPEED",
            "unknown43",
            "unknown08",
            "unknown05",
            "unknown02",
        ],
    )?;
    values.extend([
        (
            "RIGHT_LANE_YELLOW_FLASH",
            f64::from(hud.get_right_lane_depart()),
        ),
        (
            "LEFT_LANE_YELLOW_FLASH",
            f64::from(hud.get_left_lane_depart()),
        ),
        ("LARGE_STEERING_WHEEL_ICON", if enabled { 2. } else { 0. }),
        (
            "RIGHT_LANE_GREEN",
            f64::from(hud.get_right_lane_visible() && enabled),
        ),
        (
            "LEFT_LANE_GREEN",
            f64::from(hud.get_left_lane_visible() && enabled),
        ),
    ]);
    frame(packer, "PROPILOT_HUD", 0, &values)
}
pub fn hud_info(packer: &mut Packer, message: &Values, required: bool) -> Result<Frame, Error> {
    let mut values = copied(
        message,
        &[
            "NA_HIGH_ACCEL_TEMP",
            "SIDE_RADAR_NA_HIGH_CABIN_TEMP",
            "SIDE_RADAR_MALFUNCTION",
            "LKAS_MALFUNCTION",
            "FRONT_RADAR_MALFUNCTION",
            "SIDE_RADAR_NA_CLEAN_REAR_CAMERA",
            "NA_POOR_ROAD_CONDITIONS",
            "CURRENTLY_UNAVAILABLE",
            "SAFETY_SHIELD_OFF",
            "FRONT_COLLISION_NA_FRONT_RADAR_OBSTRUCTION",
            "PEDAL_MISSAPPLICATION_SYSTEM_ACTIVATED",
            "SIDE_IMPACT_NA_RADAR_OBSTRUCTION",
            "WARNING_DO_NOT_ENTER",
            "SIDE_IMPACT_SYSTEM_OFF",
            "SIDE_IMPACT_MALFUNCTION",
            "FRONT_COLLISION_MALFUNCTION",
            "SIDE_RADAR_MALFUNCTION2",
            "LKAS_MALFUNCTION2",
            "FRONT_RADAR_MALFUNCTION2",
            "PROPILOT_NA_MSGS",
            "BOTTOM_MSG",
            "HANDS_ON_WHEEL_WARNING",
            "WARNING_STEP_ON_BRAKE_NOW",
            "PROPILOT_NA_FRONT_CAMERA_OBSTRUCTED",
            "PROPILOT_NA_HIGH_CABIN_TEMP",
            "WARNING_PROPILOT_MALFUNCTION",
            "ACC_UNAVAILABLE_HIGH_CABIN_TEMP",
            "ACC_NA_FRONT_CAMERA_IMPARED",
            "unknown07",
            "unknown10",
            "unknown15",
            "unknown23",
            "unknown19",
            "unknown31",
            "unknown32",
            "unknown46",
            "unknown61",
            "unknown55",
            "unknown50",
        ],
    )?;
    if required {
        let value = values
            .iter_mut()
            .find(|(name, _)| *name == "HANDS_ON_WHEEL_WARNING")
            .ok_or_else(|| Error::Signal("HANDS_ON_WHEEL_WARNING".to_owned()))?;
        value.1 = 1.;
    }
    frame(packer, "PROPILOT_HUD_INFO_MSG", 0, &values)
}
