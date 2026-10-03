use super::{
    bus::CanBus,
    flags as f,
    wire::{copy_signals, get, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

pub fn buttons(
    writer: &mut CanWriter,
    bus: CanBus,
    counter: f64,
    button: f64,
) -> Result<Frame, Error> {
    writer.frame(
        "CRUISE_BUTTONS",
        bus.ecan,
        &values(&[
            ("COUNTER", counter),
            ("SET_ME_1", 1.),
            ("CRUISE_BUTTONS", button),
        ]),
        None,
    )
}

pub fn alt_buttons(
    writer: &mut CanWriter,
    input: (CanBus, u32, f64, f64),
    source: &mut Values,
) -> Result<Frame, Error> {
    let (bus, flags, button, count) = input;
    let counter = (get(source, "COUNTER")? + 1. + count) % 256.;
    set(source, &[("CRUISE_BUTTONS", button), ("COUNTER", counter)]);
    writer.frame(
        "CRUISE_BUTTONS_ALT",
        if flags & f::HDA2 != 0 {
            bus.ecan
        } else {
            bus.cam
        },
        source,
        None,
    )
}

pub fn cancel(
    writer: &mut CanWriter,
    input: (CanBus, u32),
    source: &Values,
) -> Result<Frame, Error> {
    let (bus, flags) = input;
    let signals = if flags & f::CAMERA_SCC != 0 {
        &[
            "COUNTER",
            "CHECKSUM",
            "NEW_SIGNAL_1",
            "MainMode_ACC",
            "ACCMode",
            "ZEROS_9",
            "CRUISE_STANDSTILL",
            "ZEROS_5",
            "DISTANCE_SETTING",
            "VSetDis",
        ][..]
    } else {
        &[
            "COUNTER",
            "CHECKSUM",
            "ACCMode",
            "VSetDis",
            "CRUISE_STANDSTILL",
        ][..]
    };
    let mut data = copy_signals(source, signals)?;
    set(
        &mut data,
        &[("ACCMode", 4.), ("aReqRaw", 0.), ("aReqValue", 0.)],
    );
    writer.frame("SCC_CONTROL", bus.ecan, &data, None)
}

pub struct ForwardInput<'a> {
    pub bus: CanBus,
    pub frame: u32,
    pub message: &'a str,
    pub button: f64,
    pub main_trigger: i32,
    pub lfa_trigger: i32,
}

pub fn forward(
    writer: &mut CanWriter,
    input: ForwardInput<'_>,
    source: Option<&Values>,
) -> Result<Vec<Frame>, Error> {
    let mut result = Vec::with_capacity(1);
    if input.frame.is_multiple_of(2) {
        if let Some(source) = source {
            let mut data = source.clone();
            data.insert("NORMAL_CRUISE_MAIN_BTN".into(), 0.);
            if get(&data, "CRUISE_BUTTONS")? == 0. {
                data.insert("CRUISE_BUTTONS".into(), input.button);
            }
            if input.main_trigger <= 0 && input.lfa_trigger > 0 {
                data.insert("LFA_BTN".into(), 1.);
            }
            result.push(writer.frame(input.message, input.bus.cam, &data, None)?);
        }
    }
    Ok(result)
}
