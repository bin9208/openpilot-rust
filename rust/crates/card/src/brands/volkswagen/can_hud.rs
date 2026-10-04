use super::{can::send, config::Family, Error};
use openpilot_can::{packer::Packer, Frame};
pub(super) struct Legacy {
    pub family: Family,
    pub status: i32,
    pub speed: f64,
    pub lead: f64,
    pub bars: f64,
}
pub(super) fn legacy(packer: &mut Packer, input: Legacy) -> Result<Frame, Error> {
    match input.family {
        Family::Pq => send(
            packer,
            "ACC_GRA_Anzeige",
            0,
            &[
                ("ACA_StaACC", f64::from(input.status)),
                ("ACA_Zeitluecke", input.bars + 2.),
                ("ACA_V_Wunsch", input.speed),
                ("ACA_gemZeitl", input.lead),
                ("ACA_PrioDisp", 3.),
            ],
        ),
        Family::Mqb | Family::Meb => send(
            packer,
            "ACC_02",
            0,
            &[
                ("ACC_Status_Anzeige", f64::from(input.status)),
                (
                    "ACC_Wunschgeschw_02",
                    if input.speed < 250. {
                        input.speed
                    } else {
                        327.36
                    },
                ),
                ("ACC_Gesetzte_Zeitluecke", input.bars + 2.),
                ("ACC_Display_Prio", 3.),
                ("ACC_Abstandsindex", input.lead),
            ],
        ),
    }
}
pub(super) struct Meb {
    pub status: i32,
    pub speed: f64,
    pub lead: bool,
    pub bars: f64,
    pub distance: f64,
    pub gap: f64,
    pub event: i32,
    pub speed_limit: f64,
    pub event_speed: f64,
    pub icon: i32,
}
fn speed_limit(value: f64) -> Result<f64, Error> {
    let value = (value * 3.6).round_ties_even();
    if !value.is_finite() {
        return Err(Error::Numeric);
    }
    let mut selected = 0.;
    for (code, limit) in [
        (1., 5.),
        (2., 7.),
        (3., 10.),
        (4., 15.),
        (5., 20.),
        (6., 25.),
        (7., 30.),
        (8., 35.),
        (9., 40.),
        (10., 45.),
        (11., 50.),
        (12., 55.),
        (13., 60.),
        (14., 65.),
        (15., 70.),
        (16., 75.),
        (17., 80.),
        (18., 85.),
        (19., 90.),
        (20., 95.),
        (21., 100.),
        (22., 110.),
        (23., 120.),
        (24., 130.),
        (25., 140.),
        (26., 150.),
        (27., 160.),
        (28., 200.),
        (30., 250.),
    ] {
        if value >= limit {
            selected = code;
        } else {
            break;
        }
    }
    Ok(selected)
}
pub(super) fn meb(packer: &mut Packer, input: Meb) -> Result<Frame, Error> {
    let active = matches!(input.status, 3 | 4);
    let limit = if active {
        speed_limit(input.speed_limit)?
    } else {
        0.
    };
    send(
        packer,
        "MEB_ACC_01",
        0,
        &[
            ("ACC_Status_ACC", f64::from(input.status)),
            ("ACC_Tempolimit", limit),
            (
                "ACC_Wunschgeschw_02",
                if input.speed < 250. {
                    input.speed
                } else {
                    327.36
                },
            ),
            ("ACC_Gesetzte_Zeitluecke", input.bars),
            ("ACC_Display_Prio", 1.),
            ("ACC_Optischer_Fahrerhinweis", 0.),
            ("ACC_Akustischer_Fahrerhinweis", 0.),
            ("ACC_Texte_Zusatzanz_02", 0.),
            ("ACC_Abstandsindex_02", 569.),
            ("ACC_EGO_Fahrzeug", f64::from(input.status == 3)),
            ("Lead_Type_Detected", f64::from(input.lead)),
            ("Lead_Type", if input.lead { 3. } else { 0. }),
            (
                "Lead_Distance",
                if input.lead { input.distance } else { 0. },
            ),
            ("ACC_Enabled", f64::from(active)),
            ("ACC_Standby_Override", f64::from(input.status != 3)),
            ("Street_Color", f64::from(active)),
            (
                "Lead_Brightness",
                if input.icon > 0 {
                    f64::from(input.icon)
                } else if input.status == 3 {
                    3.
                } else {
                    0.
                },
            ),
            ("ACC_Events", f64::from(input.event)),
            (
                "ACC_Event_Wunschgeschw",
                if input.event_speed > 0. {
                    input.event_speed
                } else {
                    input.speed_limit * 3.6
                },
            ),
            (
                "Zeitluecke_1",
                if input.bars == 1. { input.gap } else { 0. },
            ),
            (
                "Zeitluecke_2",
                if input.bars == 2. { input.gap } else { 0. },
            ),
            (
                "Zeitluecke_3",
                if input.bars == 3. { input.gap } else { 0. },
            ),
            (
                "Zeitluecke_4",
                if input.bars == 4. { input.gap } else { 0. },
            ),
            (
                "Zeitluecke_5",
                if input.bars == 5. { input.gap } else { 0. },
            ),
            ("Zeitluecke_Farbe", f64::from(matches!(input.status, 2..=4))),
            ("ACC_Anzeige_Zeitluecke", f64::from(input.status != 0)),
            ("SET_ME_0X1", 1.),
            ("SET_ME_0X6A", 106.),
            ("SET_ME_0XFFFF", 65535.),
            ("SET_ME_0X7FFF", 32767.),
        ],
    )
}
