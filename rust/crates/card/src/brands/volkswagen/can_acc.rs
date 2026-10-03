use super::{can::send, config::Family, Error, MEB_GEN2, MQB_EVO};
use openpilot_can::{packer::Packer, Frame};
pub(super) struct Acc {
    pub family: Family,
    pub flags: u32,
    pub kind: f64,
    pub enabled: bool,
    pub accel: f64,
    pub control: i32,
    pub hold: i32,
    pub stopping: bool,
    pub starting: bool,
    pub esp_hold: bool,
    pub speed: f64,
    pub override_: bool,
    pub travel: bool,
}
pub(super) fn acceleration(packer: &mut Packer, input: Acc) -> Result<Vec<Frame>, Error> {
    let mut sends = Vec::new();
    match input.family {
        Family::Pq => sends.push(send(
            packer,
            "ACC_System",
            0,
            &[
                ("ACS_Sta_ADR", f64::from(input.control)),
                ("ACS_StSt_Info", f64::from(input.enabled)),
                ("ACS_Typ_ACC", input.kind),
                (
                    "ACS_Anhaltewunsch",
                    f64::from(input.kind == 1. && input.stopping),
                ),
                ("ACS_FreigSollB", f64::from(input.enabled)),
                (
                    "ACS_Sollbeschl",
                    if input.enabled { input.accel } else { 3.01 },
                ),
                ("ACS_zul_Regelabw", if input.enabled { 0.2 } else { 1.27 }),
                ("ACS_max_AendGrad", if input.enabled { 3. } else { 5.08 }),
            ],
        )?),
        Family::Mqb => {
            sends.push(send(
                packer,
                "ACC_06",
                0,
                &[
                    ("ACC_Typ", input.kind),
                    ("ACC_Status_ACC", f64::from(input.control)),
                    ("ACC_StartStopp_Info", f64::from(input.enabled)),
                    (
                        "ACC_Sollbeschleunigung_02",
                        if input.enabled { input.accel } else { 3.01 },
                    ),
                    ("ACC_zul_Regelabw_unten", 0.2),
                    ("ACC_zul_Regelabw_oben", 0.2),
                    (
                        "ACC_neg_Sollbeschl_Grad_02",
                        if input.enabled { 4. } else { 0. },
                    ),
                    (
                        "ACC_pos_Sollbeschl_Grad_02",
                        if input.enabled { 4. } else { 0. },
                    ),
                    ("ACC_Anfahren", f64::from(input.starting)),
                    ("ACC_Anhalten", f64::from(input.stopping)),
                ],
            )?);
            let hold = if input.starting {
                4.
            } else if input.esp_hold {
                3.
            } else if input.stopping {
                1.
            } else {
                0.
            };
            sends.push(send(
                packer,
                "ACC_07",
                0,
                &[
                    ("ACC_Anhalteweg", if input.stopping { 0.3 } else { 20.46 }),
                    ("ACC_Freilauf_Info", if input.enabled { 2. } else { 0. }),
                    ("ACC_Folgebeschl", 3.02),
                    (
                        "ACC_Sollbeschleunigung_02",
                        if input.enabled { input.accel } else { 3.01 },
                    ),
                    ("ACC_Anforderung_HMS", hold),
                    ("ACC_Anfahren", f64::from(input.starting)),
                    ("ACC_Anhalten", f64::from(input.stopping)),
                ],
            )?);
        }
        Family::Meb => {
            let full = input.stopping && input.esp_hold;
            let full_no_start = input.esp_hold && !input.starting;
            let stopping = input.stopping && !input.esp_hold;
            let accel = if input.enabled {
                if input.override_ {
                    0.
                } else if full {
                    3.01
                } else {
                    input.accel
                }
            } else {
                3.01
            };
            let limits = matches!(input.control, 3 | 4) && !full_no_start;
            let mut values = vec![
                ("ACC_Typ", input.kind),
                ("ACC_Status_ACC", f64::from(input.control)),
                ("ACC_StartStopp_Info", f64::from(input.enabled)),
                ("ACC_Sollbeschleunigung_02", accel),
                ("ACC_zul_Regelabw_unten", 0.),
                ("ACC_zul_Regelabw_oben", 0.),
                ("ACC_neg_Sollbeschl_Grad_02", if limits { 4. } else { 0. }),
                ("ACC_pos_Sollbeschl_Grad_02", if limits { 4. } else { 0. }),
                ("ACC_Anfahren", f64::from(input.starting)),
                ("ACC_Anhalten", f64::from(stopping)),
                (
                    "ACC_Anhalteweg",
                    if stopping {
                        if input.flags & MQB_EVO != 0 {
                            0.5
                        } else {
                            0.
                        }
                    } else {
                        20.46
                    },
                ),
                ("ACC_Anforderung_HMS", f64::from(input.hold)),
                ("ACC_AKTIV_regelt", f64::from(input.control == 3)),
                ("Speed", input.speed),
                ("SET_ME_0XFE", 254.),
                ("SET_ME_0X1", 1.),
                ("SET_ME_0X9", 9.),
            ];
            if input.flags & MEB_GEN2 != 0 {
                values.push(("SET_ME_0x2FE", 766.));
            }
            sends.push(send(packer, "ACC_18", 0, &values)?);
            if input.travel {
                sends.push(send(
                    packer,
                    "TA_01",
                    0,
                    &[
                        ("Travel_Assist_Status", if input.enabled { 4. } else { 2. }),
                        ("Travel_Assist_Request", 0.),
                        ("Travel_Assist_Available", 1.),
                    ],
                )?);
            }
        }
    }
    Ok(sends)
}
