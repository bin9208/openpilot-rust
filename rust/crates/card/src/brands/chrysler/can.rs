use super::Error;
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control::VisualAlert;

fn frame(packer: &mut Packer, name: &str, bus: u8, values: &[(&str, f64)]) -> Result<Frame, Error> {
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, values, None)?,
        bus,
    })
}
pub fn steering(
    packer: &mut Packer,
    torque: i32,
    control: bool,
    ram: bool,
) -> Result<Frame, Error> {
    frame(
        packer,
        "LKAS_COMMAND",
        0,
        &[
            ("STEERING_TORQUE", f64::from(torque)),
            (
                "LKAS_CONTROL_BIT",
                if control {
                    if ram {
                        2.
                    } else {
                        1.
                    }
                } else {
                    0.
                },
            ),
        ],
    )
}
pub fn buttons(packer: &mut Packer, counter: f64, bus: u8, cancel: bool) -> Result<Frame, Error> {
    frame(
        packer,
        "CRUISE_BUTTONS",
        bus,
        &[
            ("ACC_Cancel", f64::from(cancel)),
            ("ACC_Resume", f64::from(!cancel)),
            ("COUNTER", counter % 16.),
        ],
    )
}
pub struct Hud {
    pub active: bool,
    pub alert: VisualAlert,
    pub count: u64,
    pub model: f64,
    pub high_beam: f64,
    pub ram: bool,
}
pub fn hud(packer: &mut Packer, input: Hud) -> Result<Frame, Error> {
    let mut color = if input.active { 2. } else { 1. };
    let mut lines = if input.active { 3. } else { 0. };
    let mut alert = if input.active { 7. } else { 0. };
    if input.count < 4 {
        alert = 1.;
    }
    if matches!(input.alert, VisualAlert::Ldw | VisualAlert::SteerRequired) {
        color = 4.;
        lines = 0.;
        alert = 6.;
    }
    let mut values = vec![
        ("LKAS_ICON_COLOR", color),
        ("CAR_MODEL", input.model),
        ("LKAS_LANE_LINES", lines),
        ("LKAS_ALERTS", alert),
    ];
    if input.ram {
        values.push(("AUTO_HIGH_BEAM_ON", input.high_beam));
    }
    frame(packer, "DAS_6", 0, &values)
}
