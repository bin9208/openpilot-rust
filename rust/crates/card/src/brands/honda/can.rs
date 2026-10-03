use super::{config::Config, state::Values, Error};
use openpilot_can::{packer::Packer, Frame};

pub(super) fn send(
    packer: &mut Packer,
    name: &str,
    bus: u8,
    values: &[(&str, f64)],
) -> Result<Frame, Error> {
    let data = packer.pack(name, values, None)?;
    let address = if data.is_empty() {
        0
    } else {
        packer.dbc.message(name)?.address
    };
    Ok(Frame { address, data, bus })
}
pub(super) struct Brake<'a> {
    pub apply: i32,
    pub pump: bool,
    pub cancel: bool,
    pub fcw: u8,
    pub stock: &'a Values,
    pub bus: u8,
}
pub(super) fn brake(packer: &mut Packer, input: Brake<'_>) -> Result<Frame, Error> {
    let chime = if input.fcw != 0 {
        input
            .stock
            .get("CHIME")
            .copied()
            .ok_or_else(|| Error::Signal("CHIME".into()))?
    } else {
        0.
    };
    send(
        packer,
        "BRAKE_COMMAND",
        input.bus,
        &[
            ("COMPUTER_BRAKE", f64::from(input.apply)),
            ("BRAKE_PUMP_REQUEST", f64::from(input.pump)),
            ("CRUISE_OVERRIDE", 1.),
            ("CRUISE_FAULT_CMD", 0.),
            ("CRUISE_CANCEL_CMD", f64::from(input.cancel)),
            ("COMPUTER_BRAKE_REQUEST", f64::from(input.apply > 0)),
            ("SET_ME_1", 1.),
            ("BRAKE_LIGHTS", f64::from(input.apply > 0)),
            ("CHIME", chime),
            ("FCW", f64::from(input.fcw << 1)),
            ("AEB_REQ_1", 0.),
            ("AEB_REQ_2", 0.),
            ("AEB_STATUS", 0.),
        ],
    )
}
pub(super) struct Acceleration {
    pub enabled: bool,
    pub active: bool,
    pub accel: f64,
    pub gas: f64,
    pub stopping_counter: u64,
}
pub(super) fn acceleration(
    packer: &mut Packer,
    config: &Config,
    input: Acceleration,
) -> Result<Vec<Frame>, Error> {
    let mut sends = Vec::new();
    let mut values = vec![
        ("ACCEL_COMMAND", if input.active { input.accel } else { 0. }),
        (
            "STANDSTILL",
            f64::from(input.active && input.stopping_counter > 0),
        ),
    ];
    if config.radarless() {
        values.extend([
            ("CONTROL_ON", f64::from(input.enabled)),
            ("IDLESTOP_ALLOW", f64::from(input.stopping_counter > 200)),
        ]);
    } else {
        let braking = input.active && input.accel < -0.2;
        values.extend([
            ("CONTROL_ON", if input.enabled { 5. } else { 0. }),
            (
                "GAS_COMMAND",
                if input.active && input.accel > -0.2 {
                    input.gas
                } else {
                    -30000.
                },
            ),
            ("BRAKE_LIGHTS", f64::from(braking)),
            ("BRAKE_REQUEST", f64::from(braking)),
            (
                "STANDSTILL_RELEASE",
                f64::from(input.active && input.stopping_counter == 0),
            ),
        ]);
        sends.push(send(
            packer,
            "ACC_CONTROL_ON",
            config.bus.pt,
            &[
                ("SET_TO_3", 3.),
                ("CONTROL_ON", f64::from(input.enabled)),
                ("SET_TO_FF", 255.),
                ("SET_TO_75", 117.),
                ("SET_TO_30", 48.),
            ],
        )?);
    }
    sends.push(send(packer, "ACC_CONTROL", config.bus.pt, &values)?);
    Ok(sends)
}
