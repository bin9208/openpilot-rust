use super::{
    can::send,
    config::Family,
    state::{required, Values},
    Error,
};
use openpilot_can::{packer::Packer, Frame};
pub(super) struct Buttons<'a> {
    pub stock: &'a Values,
    pub cancel: bool,
    pub resume: bool,
    pub bus: u8,
    pub family: Family,
}
pub(super) fn buttons(packer: &mut Packer, input: Buttons<'_>) -> Result<Frame, Error> {
    let mut values = match input.family {
        Family::Pq => required(
            input.stock,
            &[
                "GRA_Hauptschalt",
                "GRA_Typ_Hauptschalt",
                "GRA_Kodierinfo",
                "GRA_Sender",
            ],
        )?,
        Family::Mqb | Family::Meb => required(
            input.stock,
            &[
                "GRA_Hauptschalter",
                "GRA_Typ_Hauptschalter",
                "GRA_Codierung",
                "GRA_Tip_Stufe_2",
                "GRA_ButtonTypeInfo",
            ],
        )?,
    };
    let counter = *input
        .stock
        .get("COUNTER")
        .ok_or_else(|| Error::Signal("COUNTER".into()))?;
    values.extend([
        ("COUNTER", (counter + 1.) % 16.),
        ("GRA_Abbrechen", f64::from(input.cancel)),
        (
            if matches!(input.family, Family::Pq) {
                "GRA_Recall"
            } else {
                "GRA_Tip_Wiederaufnahme"
            },
            f64::from(input.resume),
        ),
    ]);
    if matches!(input.family, Family::Meb) {
        values.push(("GRA_Tip_Setzen", 0.));
    }
    send(
        packer,
        if matches!(input.family, Family::Pq) {
            "GRA_Neu"
        } else {
            "GRA_ACC_01"
        },
        input.bus,
        &values,
    )
}
