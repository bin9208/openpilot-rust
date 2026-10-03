use super::Error;
use openpilot_can::{packer::Packer, Frame};
use std::collections::BTreeMap;

fn frame(packer: &mut Packer, name: &str, values: &[(&str, f64)]) -> Result<Frame, Error> {
    let address = packer.dbc.message(name)?.address;
    Ok(Frame {
        address,
        data: packer.pack(name, values, None)?,
        bus: 0,
    })
}

pub fn steering(
    packer: &mut Packer,
    flags: u32,
    count: u64,
    torque: i32,
    camera: &BTreeMap<String, f64>,
) -> Result<Frame, Error> {
    if flags & 1 == 0 {
        return frame(packer, "CAM_LKAS", &[]);
    }
    let lo = (torque + 2048) & 255;
    let hi = (torque + 2048) >> 8;
    let counter = count % 16;
    let bit = camera["BIT_1"] as i32;
    let error1 = camera["ERR_BIT_1"] as i32;
    let error2 = camera["ERR_BIT_2"] as i32;
    let mut checksum = 249 - counter as i32 - hi - lo - error1 - (error2 << 4) - (bit << 5) - 2;
    if checksum < 0 {
        checksum += if checksum < -256 { 512 } else { 256 };
    }
    frame(
        packer,
        "CAM_LKAS",
        &[
            ("LKAS_REQUEST", f64::from(torque)),
            ("CTR", counter as f64),
            ("ERR_BIT_1", f64::from(error1)),
            ("LINE_NOT_VISIBLE", 0.),
            ("LDW", 0.),
            ("BIT_1", f64::from(bit)),
            ("ERR_BIT_2", f64::from(error2)),
            ("STEERING_ANGLE", 0.),
            ("ANGLE_ENABLED", 0.),
            ("CHKSUM", f64::from(checksum % 256)),
        ],
    )
}

pub fn alert(
    packer: &mut Packer,
    camera: &BTreeMap<String, f64>,
    required: bool,
) -> Result<Frame, Error> {
    let mut values = [
        "LINE_VISIBLE",
        "LINE_NOT_VISIBLE",
        "LANE_LINES",
        "BIT1",
        "BIT2",
        "BIT3",
        "NO_ERR_BIT",
        "S1",
        "S1_HBEAM",
    ]
    .map(|key| (key, camera[key]))
    .to_vec();
    values.extend([
        ("HANDS_WARN_3_BITS", if required { 7. } else { 0. }),
        ("HANDS_ON_STEER_WARN", f64::from(required)),
        ("HANDS_ON_STEER_WARN_2", f64::from(required)),
        ("LDW_WARN_LL", 0.),
        ("LDW_WARN_RL", 0.),
    ]);
    frame(packer, "CAM_LANEINFO", &values)
}

pub fn button(packer: &mut Packer, flags: u32, counter: u64, command: i32) -> Result<Frame, Error> {
    if flags & 1 == 0 {
        return Err(Error::ButtonFlags(flags));
    }
    let cancel = f64::from(command == 4);
    let resume = f64::from(command == 3);
    let plus = f64::from(command == 1);
    let minus = f64::from(command == 2);
    frame(
        packer,
        "CRZ_BTNS",
        &[
            ("CAN_OFF", cancel),
            ("CAN_OFF_INV", 1. - cancel),
            ("SET_P", plus),
            ("SET_P_INV", 1. - plus),
            ("RES", resume),
            ("RES_INV", 1. - resume),
            ("SET_M", minus),
            ("SET_M_INV", 1. - minus),
            ("DISTANCE_LESS", 0.),
            ("DISTANCE_LESS_INV", 1.),
            ("DISTANCE_MORE", 0.),
            ("DISTANCE_MORE_INV", 1.),
            ("MODE_X", 0.),
            ("MODE_X_INV", 1.),
            ("MODE_Y", 0.),
            ("MODE_Y_INV", 1.),
            ("BIT1", 1.),
            ("BIT2", 1.),
            ("BIT3", 1.),
            ("CTR", ((counter + 1) % 16) as f64),
        ],
    )
}
