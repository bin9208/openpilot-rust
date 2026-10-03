use super::{
    config::Family,
    state::{required, Values},
    Error,
};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control;
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
pub(super) fn patch(values: &mut Vec<(&'static str, f64)>, pairs: &[(&'static str, f64)]) {
    for &(name, value) in pairs {
        if let Some(entry) = values.iter_mut().find(|(key, _)| *key == name) {
            entry.1 = value;
        } else {
            values.push((name, value));
        }
    }
}
pub(super) struct Steering {
    pub value: f64,
    pub enabled: bool,
    pub power: i32,
    pub family: Family,
}
pub(super) fn steering(packer: &mut Packer, input: Steering) -> Result<Frame, Error> {
    match input.family {
        Family::Pq => send(
            packer,
            "HCA_1",
            0,
            &[
                ("LM_Offset", input.value.abs()),
                ("LM_OffSign", f64::from(input.value < 0.)),
                (
                    "HCA_Status",
                    if input.enabled && input.value != 0. {
                        5.
                    } else {
                        3.
                    },
                ),
                ("Vib_Freq", 16.),
            ],
        ),
        Family::Mqb => send(
            packer,
            "HCA_01",
            0,
            &[
                ("HCA_01_Status_HCA", if input.enabled { 5. } else { 3. }),
                ("HCA_01_LM_Offset", input.value.abs()),
                ("HCA_01_LM_OffSign", f64::from(input.value < 0.)),
                ("HCA_01_Vib_Freq", 18.),
                ("HCA_01_Sendestatus", f64::from(input.enabled)),
                ("EA_ACC_Wunschgeschwindigkeit", 327.36),
            ],
        ),
        Family::Meb => send(
            packer,
            "HCA_03",
            0,
            &[
                ("Curvature", input.value.abs()),
                ("Curvature_VZ", f64::from(input.value > 0. && input.enabled)),
                (
                    "Power",
                    if input.enabled {
                        f64::from(input.power)
                    } else {
                        0.
                    },
                ),
                ("RequestStatus", if input.enabled { 4. } else { 2. }),
                ("HighSendRate", f64::from(input.enabled)),
            ],
        ),
    }
}
pub(super) fn eps(packer: &mut Packer, stock: &Values, torque: f64) -> Result<Frame, Error> {
    let mut values = required(
        stock,
        &[
            "COUNTER",
            "EPS_Lenkungstyp",
            "EPS_Berechneter_LW",
            "EPS_VZ_BLW",
            "EPS_HCA_Status",
        ],
    )?;
    values.extend([
        ("EPS_Lenkmoment", torque.abs()),
        ("EPS_VZ_Lenkmoment", f64::from(torque < 0.)),
    ]);
    send(packer, "LH_EPS_03", 2, &values)
}
pub(super) struct Lka<'a> {
    pub stock: &'a Values,
    pub active: bool,
    pub pressed: bool,
    pub alert: i32,
    pub hud: h_u_d_control::Reader<'a>,
    pub family: Family,
}
pub(super) fn lka(packer: &mut Packer, input: Lka<'_>) -> Result<Frame, Error> {
    let mut values = if input.stock.is_empty() {
        Vec::new()
    } else {
        required(
            input.stock,
            &[
                "LDW_SW_Warnung_links",
                "LDW_SW_Warnung_rechts",
                "LDW_Seite_DLCTLC",
                "LDW_DLC",
                "LDW_TLC",
            ],
        )?
    };
    let display = if matches!(input.family, Family::Meb) {
        i32::from(input.active)
    } else {
        0
    };
    if matches!(input.family, Family::Meb) {
        values.push(("LDW_Gong", 0.));
    }
    values.extend([
        (
            if matches!(input.family, Family::Pq) {
                "LDW_Lampe_gelb"
            } else {
                "LDW_Status_LED_gelb"
            },
            f64::from(input.active && input.pressed),
        ),
        (
            if matches!(input.family, Family::Pq) {
                "LDW_Lampe_gruen"
            } else {
                "LDW_Status_LED_gruen"
            },
            f64::from(input.active && !input.pressed),
        ),
        (
            "LDW_Lernmodus_links",
            f64::from(if input.hud.get_left_lane_depart() {
                3 + display
            } else {
                1 + i32::from(input.hud.get_left_lane_visible()) + display
            }),
        ),
        (
            "LDW_Lernmodus_rechts",
            f64::from(if input.hud.get_right_lane_depart() {
                3 + display
            } else {
                1 + i32::from(input.hud.get_right_lane_visible()) + display
            }),
        ),
        (
            if matches!(input.family, Family::Pq) {
                "LDW_Textbits"
            } else {
                "LDW_Texte"
            },
            f64::from(input.alert),
        ),
    ]);
    send(
        packer,
        if matches!(input.family, Family::Pq) {
            "LDW_Status"
        } else {
            "LDW_02"
        },
        0,
        &values,
    )
}
