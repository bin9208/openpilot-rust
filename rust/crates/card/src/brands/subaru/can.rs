use super::{state::Values, Error};
use openpilot_can::{packer::Packer, Frame};

pub(super) fn copied<'a>(
    source: &Values,
    names: &'a [&'a str],
) -> Result<Vec<(&'a str, f64)>, Error> {
    names
        .iter()
        .map(|name| {
            Ok((
                *name,
                source
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).to_owned()))?,
            ))
        })
        .collect()
}
pub(super) fn set<'a>(values: &mut Vec<(&'a str, f64)>, name: &'a str, value: f64) {
    if let Some((_, target)) = values.iter_mut().find(|(key, _)| *key == name) {
        *target = value;
    } else {
        values.push((name, value));
    }
}
pub(super) fn get(values: &[(&str, f64)], name: &str) -> Result<f64, Error> {
    values
        .iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
        .ok_or_else(|| Error::Signal(name.to_owned()))
}
pub(super) fn send(
    packer: &mut Packer,
    name: &str,
    bus: u8,
    values: &[(&str, f64)],
) -> Result<Frame, Error> {
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, values, None)?,
        bus,
    })
}
pub(super) fn preglobal(
    packer: &mut Packer,
    name: &str,
    mut values: Vec<(&str, f64)>,
) -> Result<Frame, Error> {
    let data = packer.pack(name, &values, None)?;
    let checksum = data
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 7)
        .fold(0u8, |sum, (_, byte)| sum.wrapping_add(*byte));
    set(&mut values, "Checksum", f64::from(checksum));
    send(packer, name, 0, &values)
}
pub(super) const DISTANCE: &[&str] = &[
    "CHECKSUM",
    "Signal1",
    "Cruise_Fault",
    "Cruise_Throttle",
    "Signal2",
    "Car_Follow",
    "Low_Speed_Follow",
    "Cruise_Soft_Disable",
    "Signal7",
    "Cruise_Brake_Active",
    "Distance_Swap",
    "Cruise_EPB",
    "Signal4",
    "Close_Distance",
    "Signal5",
    "Cruise_Cancel",
    "Cruise_Set",
    "Cruise_Resume",
    "Signal6",
];
pub(super) const PREGLOBAL_DISTANCE: &[&str] = &[
    "Cruise_Throttle",
    "Signal1",
    "Car_Follow",
    "Signal2",
    "Cruise_Brake_Active",
    "Distance_Swap",
    "Standstill",
    "Signal3",
    "Close_Distance",
    "Signal4",
    "Standstill_2",
    "Cruise_Fault",
    "Signal5",
    "COUNTER",
    "Signal6",
    "Cruise_Button",
    "Signal7",
];
pub(super) const DASH: &[&str] = &[
    "CHECKSUM",
    "PCB_Off",
    "LDW_Off",
    "Signal1",
    "Cruise_State_Msg",
    "LKAS_State_Msg",
    "Signal2",
    "Cruise_Soft_Disable",
    "Cruise_Status_Msg",
    "Signal3",
    "Cruise_Distance",
    "Signal4",
    "Conventional_Cruise",
    "Signal5",
    "Cruise_Disengaged",
    "Cruise_Activated",
    "Signal6",
    "Cruise_Set_Speed",
    "Cruise_Fault",
    "Cruise_On",
    "Display_Own_Car",
    "Brake_Lights",
    "Car_Follow",
    "Signal7",
    "Far_Distance",
    "Cruise_State",
];
pub(super) const LKAS: &[&str] = &[
    "CHECKSUM",
    "LKAS_Alert_Msg",
    "Signal1",
    "LKAS_ACTIVE",
    "LKAS_Dash_State",
    "Signal2",
    "Backward_Speed_Limit_Menu",
    "LKAS_Left_Line_Enable",
    "LKAS_Left_Line_Light_Blink",
    "LKAS_Right_Line_Enable",
    "LKAS_Right_Line_Light_Blink",
    "LKAS_Left_Line_Visible",
    "LKAS_Right_Line_Visible",
    "LKAS_Alert",
    "Signal3",
];
pub(super) const BRAKE: &[&str] = &[
    "CHECKSUM",
    "Signal1",
    "Brake_Pressure",
    "AEB_Status",
    "Cruise_Brake_Lights",
    "Cruise_Brake_Fault",
    "Cruise_Brake_Active",
    "Cruise_Activated",
    "Signal3",
];
pub(super) const STATUS: &[&str] = &[
    "CHECKSUM",
    "Signal1",
    "Cruise_Fault",
    "Cruise_RPM",
    "Cruise_Activated",
    "Brake_Lights",
    "Cruise_Hold",
    "Signal3",
];
pub(super) const INFOTAINMENT: &[&str] = &[
    "CHECKSUM",
    "LKAS_State_Infotainment",
    "LKAS_Blue_Lines",
    "Signal1",
    "Signal2",
];
