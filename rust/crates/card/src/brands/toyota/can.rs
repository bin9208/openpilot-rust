use super::{state::Values, Error};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control;

pub(super) fn send(
    packer: &mut Packer,
    name: &str,
    values: &[(&str, f64)],
) -> Result<Frame, Error> {
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, values, None)?,
        bus: 0,
    })
}
pub(super) struct Acceleration {
    pub accel: f64,
    pub cancel: bool,
    pub braking: bool,
    pub standstill: bool,
    pub lead: bool,
    pub acc_type: f64,
    pub fcw: bool,
    pub distance: f64,
}
pub(super) fn acceleration(packer: &mut Packer, input: Acceleration) -> Result<Frame, Error> {
    send(
        packer,
        "ACC_CONTROL",
        &[
            ("ACCEL_CMD", input.accel),
            ("ACC_TYPE", input.acc_type),
            ("DISTANCE", input.distance),
            ("MINI_CAR", f64::from(input.lead)),
            ("PERMIT_BRAKING", f64::from(input.braking)),
            ("RELEASE_STANDSTILL", f64::from(!input.standstill)),
            ("CANCEL_REQ", f64::from(input.cancel)),
            ("ALLOW_LONG_PRESS", 2.),
            ("ACC_CUT_IN", f64::from(input.fcw)),
        ],
    )
}
pub(super) struct Ui<'a> {
    pub hud: h_u_d_control::Reader<'a>,
    pub steer: bool,
    pub chime: bool,
    pub enabled: bool,
    pub stock: &'a Values,
}
pub(super) fn ui(packer: &mut Packer, input: Ui<'_>) -> Result<Frame, Error> {
    let mut values = vec![
        ("TWO_BEEPS", f64::from(input.chime)),
        ("LDA_ALERT", f64::from(input.steer)),
        (
            "RIGHT_LINE",
            if input.hud.get_right_lane_depart() {
                3.
            } else if input.hud.get_right_lane_visible() {
                1.
            } else {
                2.
            },
        ),
        (
            "LEFT_LINE",
            if input.hud.get_left_lane_depart() {
                3.
            } else if input.hud.get_left_lane_visible() {
                1.
            } else {
                2.
            },
        ),
        ("BARRIERS", f64::from(input.enabled)),
        ("SET_ME_X02", 2.),
        ("SET_ME_X01", 1.),
        ("LKAS_STATUS", 1.),
        ("REPEATED_BEEPS", 0.),
        ("LANE_SWAY_FLD", 7.),
        ("LANE_SWAY_BUZZER", 0.),
        ("LANE_SWAY_WARNING", 0.),
        ("LDA_FRONT_CAMERA_BLOCKED", 0.),
        ("TAKE_CONTROL", 0.),
        ("LANE_SWAY_SENSITIVITY", 2.),
        ("LANE_SWAY_TOGGLE", 1.),
        ("LDA_ON_MESSAGE", 0.),
        ("LDA_MESSAGES", 0.),
        ("LDA_SA_TOGGLE", 1.),
        ("LDA_SENSITIVITY", 2.),
        ("LDA_UNAVAILABLE", 0.),
        ("LDA_MALFUNCTION", 0.),
        ("LDA_UNAVAILABLE_QUIET", 0.),
        ("ADJUSTING_CAMERA", 0.),
        ("LDW_EXIST", 1.),
    ];
    if !input.stock.is_empty() {
        for (name, value) in &mut values {
            if [
                "LANE_SWAY_FLD",
                "LANE_SWAY_BUZZER",
                "LANE_SWAY_WARNING",
                "LANE_SWAY_SENSITIVITY",
                "LANE_SWAY_TOGGLE",
            ]
            .contains(name)
            {
                *value = input
                    .stock
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).to_owned()))?;
            }
        }
    }
    send(packer, "LKAS_HUD", &values)
}
