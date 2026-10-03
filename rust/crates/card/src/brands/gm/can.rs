use super::{integer, Error};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control;
use std::collections::BTreeMap;

pub fn message(
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
pub fn buttons(packer: &mut Packer, bus: u8, counter: i32, button: i32) -> Result<Frame, Error> {
    let checksum = 255 + counter * 0x4ef - ((button - 1) << 4);
    message(
        packer,
        "ASCMSteeringButton",
        bus,
        &[
            ("ACCButtons", f64::from(button)),
            ("RollingCounter", f64::from(counter)),
            ("ACCAlwaysOne", 1.),
            ("DistanceButton", 0.),
            ("SteeringButtonChecksum", f64::from(checksum)),
        ],
    )
}
pub fn steering(
    packer: &mut Packer,
    torque: i32,
    counter: i32,
    active: bool,
) -> Result<Frame, Error> {
    let checksum = 0x1000 - (i32::from(active) << 11) - (torque & 0x7ff) - counter;
    message(
        packer,
        "ASCMLKASteeringCmd",
        0,
        &[
            ("LKASteeringCmdActive", f64::from(active)),
            ("LKASteeringCmd", f64::from(torque)),
            ("RollingCounter", f64::from(counter)),
            ("LKASteeringCmdChecksum", f64::from(checksum)),
        ],
    )
}
pub fn pscm(packer: &mut Packer, stock: &BTreeMap<String, f64>) -> Result<Frame, Error> {
    let names = [
        "HandsOffSWDetectionMode",
        "HandsOffSWlDetectionStatus",
        "LKATorqueDeliveredStatus",
        "LKADriverAppldTrq",
        "LKATorqueDelivered",
        "LKATotalTorqueDelivered",
        "RollingCounter",
        "PSCMStatusChecksum",
    ];
    let mut values = names
        .iter()
        .map(|name| {
            Ok((
                *name,
                stock
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).into()))?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let modification = integer(1. - values[1].1)? << 5;
    values[1].1 = 1.;
    values[7].1 += f64::from(modification);
    message(packer, "PSCMStatus", 2, &values)
}
pub fn gas(
    packer: &mut Packer,
    throttle: i32,
    counter: i32,
    enabled: bool,
    stopped: bool,
) -> Result<Frame, Error> {
    let mut values = vec![
        ("GasRegenCmdActive", f64::from(enabled)),
        ("RollingCounter", f64::from(counter)),
        ("GasRegenCmd", f64::from(throttle)),
        ("GasRegenFullStopActive", f64::from(stopped)),
        ("GasRegenAccType", 1.),
    ];
    let data = packer.pack("ASCMGasRegenCmd", &values, None)?;
    let data: [u8; 8] = data.try_into().map_err(|_| Error::Numeric)?;
    let checksum = (i32::from(!enabled) << 24)
        | ((255 - i32::from(data[1])) << 16)
        | ((255 - i32::from(data[2])) << 8)
        | ((256 - i32::from(data[3]) - counter) & 255);
    values.push(("GasRegenChecksum", f64::from(checksum)));
    message(packer, "ASCMGasRegenCmd", 0, &values)
}
pub fn brake(
    packer: &mut Packer,
    bus: u8,
    input: i32,
    counter: i32,
    enabled_bolt: bool,
    full_stop: bool,
    auto_resume: bool,
) -> Result<Frame, Error> {
    let mode = if input > 0 {
        if full_stop && !auto_resume {
            0xd
        } else {
            0xa
        }
    } else if enabled_bolt && !auto_resume {
        9
    } else {
        1
    };
    let apply = if auto_resume {
        input
    } else {
        input.clamp(0, 0xfff)
    };
    let raw = (0x1000 - apply) & 0xfff;
    let checksum = (0x10000 - (mode << 12) - raw - counter) & 0xffff;
    message(
        packer,
        "EBCMFrictionBrakeCmd",
        bus,
        &[
            ("RollingCounter", f64::from(counter)),
            ("FrictionBrakeMode", f64::from(mode)),
            ("FrictionBrakeChecksum", f64::from(checksum)),
            (
                "FrictionBrakeCmd",
                f64::from(if auto_resume { -apply } else { raw }),
            ),
        ],
    )
}
pub fn dashboard(
    packer: &mut Packer,
    enabled: bool,
    speed: f64,
    hud: h_u_d_control::Reader<'_>,
) -> Result<Frame, Error> {
    message(
        packer,
        "ASCMActiveCruiseControlStatus",
        0,
        &[
            ("ACCAlwaysOne", 1.),
            ("ACCResumeButton", 0.),
            ("ACCSpeedSetpoint", speed.min(255.)),
            (
                "ACCGapLevel",
                f64::from(hud.get_lead_distance_bars()) * f64::from(enabled),
            ),
            ("ACCCmdActive", f64::from(enabled)),
            ("ACCAlwaysOne2", 1.),
            ("ACCLeadCar", f64::from(hud.get_lead_visible())),
            (
                "FCWAlert",
                if hud.get_visual_alert()? == h_u_d_control::VisualAlert::Fcw {
                    3.
                } else {
                    0.
                },
            ),
        ],
    )
}
pub fn pedal(packer: &mut Packer, gas: f64, counter: i32) -> Result<Frame, Error> {
    let enable = gas > 0.001;
    let mut values = vec![
        ("ENABLE", f64::from(enable)),
        ("COUNTER_PEDAL", f64::from(counter & 15)),
    ];
    if enable {
        values.extend([("GAS_COMMAND", gas * 255.), ("GAS_COMMAND2", gas * 255.)]);
    }
    let data = packer.pack("GAS_COMMAND", &values, None)?;
    let prefix = data
        .get(..data.len().checked_sub(1).ok_or(Error::Numeric)?)
        .ok_or(Error::Numeric)?;
    let mut reversed = prefix.to_vec();
    reversed.reverse();
    let checksum = crate::brands::hyundai::wire::crc8(&reversed, 0xd5, 0xff, 0);
    values.push(("CHECKSUM_PEDAL", f64::from(checksum)));
    message(packer, "GAS_COMMAND", 0, &values)
}
