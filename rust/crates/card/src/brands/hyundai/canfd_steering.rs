use super::{
    bus::CanBus,
    flags as f,
    wire::{boolean, crc8, get, remove_counter, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

#[derive(serde::Deserialize)]
pub struct SteeringInput {
    pub bus: CanBus,
    pub flags: u32,
    pub frame: u32,
    pub enabled: bool,
    pub lat_active: bool,
    pub cc_lat_active: bool,
    pub longitudinal: bool,
    pub torque: f64,
    pub angle: f64,
    pub max_torque: f64,
    pub angle_control: bool,
}

#[derive(Default, serde::Deserialize)]
pub struct SteeringMessages {
    pub mdps: Option<Values>,
    pub touch: Option<Values>,
    pub lfa: Option<Values>,
    pub lfa_alt: Option<Values>,
    pub adrv_161: Option<Values>,
}

pub fn steering(writer: &mut CanWriter, input: &SteeringInput) -> Result<Vec<Frame>, Error> {
    let i = input;
    let data = if i.angle_control {
        values(&[
            ("LKA_MODE", 0.),
            ("LKA_ICON", if i.enabled { 2. } else { 1. }),
            ("TORQUE_REQUEST", 0.),
            ("VALUE63", 0.),
            ("STEER_REQ", 0.),
            ("HAS_LANE_SAFETY", 0.),
            ("LKA_ACTIVE", if i.lat_active { 3. } else { 0. }),
            ("VALUE64", 0.),
            ("LKAS_ANGLE_CMD", -i.angle),
            ("LKAS_ANGLE_ACTIVE", if i.lat_active { 2. } else { 1. }),
            (
                "LKAS_ANGLE_MAX_TORQUE",
                if i.lat_active { i.max_torque } else { 0. },
            ),
            ("NEW_SIGNAL_1", 10.),
            ("DampingGain", 9.),
            ("VALUE231", 146.),
            ("VALUE239", 1.),
            ("VALUE247", 255.),
            ("VALUE255", 255.),
        ])
    } else {
        values(&[
            ("LKA_MODE", 2.),
            ("LKA_ICON", if i.enabled { 2. } else { 1. }),
            ("TORQUE_REQUEST", i.torque),
            ("DampingGain", 100.),
            ("STEER_REQ", boolean(i.lat_active)),
            ("HAS_LANE_SAFETY", 0.),
            ("VALUE63", 0.),
            ("VALUE64", 100.),
        ])
    };
    let mut frames = Vec::with_capacity(2);
    if i.flags & f::HDA2 != 0 {
        if i.longitudinal {
            frames.push(writer.frame("LFA", i.bus.ecan, &data, None)?);
        }
        if i.flags & f::CAMERA_SCC == 0 {
            frames.push(writer.frame(
                if i.flags & f::ALT_STEERING != 0 {
                    "LKAS_ALT"
                } else {
                    "LKAS"
                },
                i.bus.acan,
                &data,
                None,
            )?);
        }
    } else {
        frames.push(writer.frame("LFA", i.bus.ecan, &data, None)?);
    }
    Ok(frames)
}

/// `TorqueSteerTest`: the 0x12A torque request on an angle car. Fresh: the
/// frame torque cars send through `camera_steering`. `stock`: the camera's
/// own 0x12A with only the torque fields changed.
pub fn torque_test_lfa(
    writer: &mut CanWriter,
    bus: u8,
    request: &super::torque_steer::Request,
    stock: Option<&Values>,
) -> Result<Frame, Error> {
    let torque = [
        ("LKA_MODE", 2.),
        ("LKA_ICON", 2.),
        ("TORQUE_REQUEST", request.torque),
        ("STEER_REQ", boolean(request.steer_req)),
        ("VALUE64", 0.),
        ("HAS_LANE_SAFETY", 0.),
        ("LKA_ACTIVE", 0.),
        ("DampingGain", 0.),
    ];
    match stock.filter(|_| request.copy_stock) {
        Some(original) => {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            set(&mut data, &torque);
            writer.frame("LFA", bus, &data, counter)
        }
        None => writer.frame("LFA", bus, &values(&torque), None),
    }
}

pub fn camera_steering(
    writer: &mut CanWriter,
    input: &SteeringInput,
    source: &SteeringMessages,
) -> Result<Vec<Frame>, Error> {
    let i = input;
    let emergency = match &source.adrv_161 {
        Some(data) => [11., 12., 13., 14., 15., 21., 22., 23., 24., 25., 26.]
            .contains(&get(data, "ALERTS_1")?),
        None => false,
    };
    let mut frames = Vec::with_capacity(4);
    if let Some(original) = &source.mdps {
        let mut data = original.clone();
        let counter = remove_counter(&mut data)?;
        if i.angle_control {
            if let Some(lfa) = &source.lfa_alt {
                data.insert("LFA2_ACTIVE".into(), get(lfa, "LKAS_ANGLE_ACTIVE")?);
            }
        } else if let Some(lfa) = &source.lfa {
            data.insert("LKA_ACTIVE".into(), boolean(get(lfa, "STEER_REQ")? == 1.));
        }
        if i.frame % 1000 < 40 {
            data.insert(
                "STEERING_COL_TORQUE".into(),
                get(&data, "STEERING_COL_TORQUE")? + 220.,
            );
        }
        frames.push(writer.frame("MDPS", i.bus.cam, &data, counter)?);
    }
    if i.frame.is_multiple_of(10) {
        if let Some(original) = &source.touch {
            let mut data = original.clone();
            if i.frame % 1000 < 40 {
                set(
                    &mut data,
                    &[
                        ("TOUCH_DETECT", 3.),
                        ("TOUCH1", 50.),
                        ("TOUCH2", 50.),
                        ("CHECKSUM_", 0.),
                    ],
                );
                let frame = writer.frame("STEER_TOUCH_2AF", 0, &data, None)?;
                let checked = frame.data.get(1..8).ok_or(Error::Numeric)?;
                data.insert(
                    "CHECKSUM_".into(),
                    f64::from(crc8(checked, 0x2f, 0xff, 0xff)),
                );
            }
            frames.push(writer.frame("STEER_TOUCH_2AF", i.bus.cam, &data, None)?);
        }
    }
    if i.angle_control {
        if let Some(original) = &source.lfa_alt {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            if !emergency {
                set(
                    &mut data,
                    &[
                        ("LKAS_ANGLE_ACTIVE", if i.cc_lat_active { 2. } else { 1. }),
                        ("LKAS_ANGLE_CMD", -i.angle),
                        (
                            "LKAS_ANGLE_MAX_TORQUE",
                            if i.cc_lat_active { i.max_torque } else { 0. },
                        ),
                    ],
                );
            }
            frames.push(writer.frame("LFA_ALT", i.bus.ecan, &data, counter)?);
        }
        if let Some(original) = &source.lfa {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            if !emergency {
                set(
                    &mut data,
                    &[
                        ("LKA_MODE", 0.),
                        ("LKA_ICON", if i.cc_lat_active { 2. } else { 1. }),
                        ("TORQUE_REQUEST", -1024.),
                        ("VALUE63", 0.),
                        ("STEER_REQ", 0.),
                        ("HAS_LANE_SAFETY", 0.),
                        ("LKA_ACTIVE", if i.cc_lat_active { 3. } else { 0. }),
                        ("VALUE64", 0.),
                        ("LKAS_ANGLE_CMD", -25.6),
                        ("LKAS_ANGLE_ACTIVE", 0.),
                        ("LKAS_ANGLE_MAX_TORQUE", 0.),
                        ("NEW_SIGNAL_1", 10.),
                    ],
                );
            }
            frames.push(writer.frame("LFA", i.bus.ecan, &data, counter)?);
        }
    } else if source.lfa.is_some() {
        let data = values(&[
            ("LKA_MODE", 2.),
            ("LKA_ICON", if i.lat_active { 2. } else { 1. }),
            ("TORQUE_REQUEST", i.torque),
            ("STEER_REQ", boolean(i.lat_active)),
            ("VALUE64", 0.),
            ("HAS_LANE_SAFETY", 0.),
            ("LKA_ACTIVE", 0.),
            ("DampingGain", if i.lat_active { 0. } else { 100. }),
        ]);
        frames.push(writer.frame("LFA", i.bus.ecan, &data, None)?);
    }
    Ok(frames)
}

#[cfg(test)]
mod torque_test_tests {
    use super::*;
    use std::path::Path;

    fn writer() -> CanWriter {
        CanWriter::new(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../opendbc_repo/opendbc/dbc/hyundai_canfd_generated.dbc"),
        )
        .unwrap()
    }

    /// The bits panda and the EPS read: TORQUE_REQUEST 41|11 (+1024),
    /// STEER_REQ 52, LKA_MODE 24|3.
    fn decode(frame: &Frame) -> (i64, bool, u8) {
        let v = u128::from_le_bytes(frame.data[..16].try_into().unwrap());
        (
            ((v >> 41) & 0x7ff) as i64 - 1024,
            (v >> 52) & 1 == 1,
            ((v >> 24) & 7) as u8,
        )
    }

    #[test]
    fn fresh_and_stock_copies_carry_the_torque_request() {
        let mut w = writer();
        let request = super::super::torque_steer::Request {
            torque: -150.,
            steer_req: true,
            copy_stock: false,
        };
        let fresh = torque_test_lfa(&mut w, 1, &request, None).unwrap();
        assert_eq!((fresh.address, fresh.bus, fresh.data.len()), (0x12a, 1, 16));
        assert_eq!(decode(&fresh), (-150, true, 2));
        // A stock camera 0x12A on an angle car: LKA_MODE 0, torque 0.
        let mut stock: Values = w
            .packer
            .dbc
            .message("LFA")
            .unwrap()
            .signals
            .iter()
            .map(|s| (std::sync::Arc::<str>::from(s.name.as_str()), 0.))
            .collect();
        stock.insert("LKA_WARNING".into(), 1.);
        stock.insert("COUNTER".into(), 7.);
        let copy = torque_test_lfa(
            &mut w,
            1,
            &super::super::torque_steer::Request {
                copy_stock: true,
                ..request
            },
            Some(&stock),
        )
        .unwrap();
        assert_eq!(decode(&copy), (-150, true, 2));
        let bits = u128::from_le_bytes(copy.data[..16].try_into().unwrap());
        assert_eq!((bits >> 32) & 1, 1, "LKA_WARNING kept");
    }
}
