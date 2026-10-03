use super::{
    can::{patch, send},
    state::{required, Values},
    Error,
};
use openpilot_can::{packer::Packer, Frame};
pub(super) struct Touch<'a> {
    pub stock: &'a Values,
    pub active: bool,
    pub bus: u8,
}
pub(super) fn touch(packer: &mut Packer, input: Touch<'_>) -> Result<Frame, Error> {
    let mut values = required(
        input.stock,
        &[
            "COUNTER",
            "KLR_Touchintensitaet_1",
            "KLR_Touchintensitaet_2",
            "KLR_Touchintensitaet_3",
            "KLR_Touchauswertung",
        ],
    )?;
    if input.active {
        let counter = *input
            .stock
            .get("COUNTER")
            .ok_or_else(|| Error::Signal("COUNTER".into()))?;
        patch(
            &mut values,
            &[
                ("COUNTER", (counter + 1.) % 16.),
                ("KLR_Touchintensitaet_1", 80.),
                ("KLR_Touchintensitaet_2", 200.),
                ("KLR_Touchintensitaet_3", 10.),
                ("KLR_Touchauswertung", 10.),
            ],
        );
    }
    send(packer, "KLR_01", input.bus, &values)
}
pub(super) struct Blinker<'a> {
    pub hud: &'a Values,
    pub control: &'a Values,
    pub left: bool,
    pub right: bool,
    pub hide: bool,
}
pub(super) fn blinker(packer: &mut Packer, input: Blinker<'_>) -> Result<Frame, Error> {
    let mut values = required(
        input.hud,
        &[
            "EA_Texte",
            "ACF_Lampe_Hands_Off",
            "EA_Infotainment_Anf",
            "EA_Tueren_Anf",
            "EA_Innenraumlicht_Anf",
            "zFAS_Warnblinken",
            "STP_Primaeranz",
            "EA_Bremslichtblinken",
            "EA_Blinken",
            "EA_Unknown",
        ],
    )?;
    let blink = *input
        .hud
        .get("EA_Blinken")
        .ok_or_else(|| Error::Signal("EA_Blinken".into()))?;
    if blink == 0. {
        patch(
            &mut values,
            &[(
                "EA_Blinken",
                if input.left {
                    1.
                } else if input.right {
                    2.
                } else {
                    blink
                },
            )],
        );
    }
    if input.hide
        && matches!(
            input
                .control
                .get("EA_Funktionsstatus")
                .copied()
                .unwrap_or(0.),
            0. | 1. | 7. | 8.
        )
    {
        patch(&mut values, &[("EA_Texte", 0.), ("EA_Unknown", 1.)]);
    }
    send(packer, "EA_02", 0, &values)
}
