use super::{
    bus::CanBus,
    flags as f,
    wire::{get, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

pub fn spas(writer: &mut CanWriter, bus: CanBus, blinkers: [bool; 2]) -> Result<Vec<Frame>, Error> {
    Ok(vec![
        writer.frame("SPAS1", bus.ecan, &Values::new(), None)?,
        writer.frame(
            "SPAS2",
            bus.ecan,
            &values(&[(
                "BLINKER_CONTROL",
                if blinkers[0] {
                    3.
                } else if blinkers[1] {
                    4.
                } else {
                    0.
                },
            )]),
            None,
        )?,
    ])
}

pub fn fca_warning(writer: &mut CanWriter, input: (CanBus, u32, u32)) -> Result<Vec<Frame>, Error> {
    let (bus, flags, frame) = input;
    if flags & f::CAMERA_SCC != 0 || frame % 2 != 0 {
        return Ok(Vec::new());
    }
    Ok(vec![writer.frame(
        "ADRV_0x160",
        bus.ecan,
        &values(&[
            ("AEB_SETTING", 1.),
            ("SET_ME_2", 2.),
            ("SET_ME_FF", 255.),
            ("SET_ME_FC", 252.),
            ("SET_ME_9", 9.),
        ]),
        None,
    )?])
}

pub fn adrv(writer: &mut CanWriter, input: (CanBus, u32, u32)) -> Result<Vec<Frame>, Error> {
    let (bus, flags, frame) = input;
    if flags & f::CAMERA_SCC != 0 {
        return Ok(Vec::new());
    }
    let mut result = fca_warning(writer, input)?;
    if frame.is_multiple_of(5) {
        result.push(writer.frame(
            "ADRV_0x1ea",
            bus.ecan,
            &values(&[("HDA_MODE2", 1.), ("SET_ME_FF", 255.)]),
            None,
        )?);
        result.push(writer.frame(
            "ADRV_0x200",
            bus.ecan,
            &values(&[("SET_ME_E1", 225.), ("TauGapSet", 1.), ("NEW_SIGNAL_2", 3.)]),
            None,
        )?);
    }
    if frame.is_multiple_of(20) {
        result.push(writer.frame("ADRV_0x345", bus.ecan, &values(&[("SET_ME_15", 21.)]), None)?);
    }
    if frame.is_multiple_of(100) {
        result.push(writer.frame(
            "ADRV_0x1da",
            bus.ecan,
            &values(&[("SET_ME_22", 34.), ("SET_ME_41", 65.)]),
            None,
        )?);
    }
    Ok(result)
}

pub fn tcs(
    writer: &mut CanWriter,
    bus: CanBus,
    source: Option<&Values>,
) -> Result<Vec<Frame>, Error> {
    let Some(source) = source else {
        return Ok(Vec::new());
    };
    let mut data = source.clone();
    set(
        &mut data,
        &[
            ("DriverBraking", 0.),
            ("NEW_SIGNAL_20", 0.),
            ("NEW_SIGNAL_11", 0.),
            ("DriverBrakingLowSens", 0.),
            (
                "NEW_SIGNAL_1",
                if get(source, "ACC_REQ")? == 1. {
                    0.
                } else {
                    1.
                },
            ),
        ],
    );
    Ok(vec![writer.frame("TCS", bus.cam, &data, None)?])
}

pub fn suppress_lfa(
    writer: &mut CanWriter,
    bus: CanBus,
    captured: (Option<&Values>, Option<&Values>),
) -> Result<Vec<Frame>, Error> {
    let (name, original) = match captured {
        (Some(data), _) => ("CAM_0x362", data),
        (None, Some(data)) => ("CAM_0x2a4", data),
        (None, None) => return Ok(Vec::new()),
    };
    let mut data = original.clone();
    set(
        &mut data,
        &[
            ("SET_ME_0", 0.),
            ("SET_ME_0_2", 0.),
            ("LEFT_LANE_LINE", 0.),
            ("RIGHT_LANE_LINE", 0.),
        ],
    );
    Ok(vec![writer.frame(name, bus.acan, &data, None)?])
}
