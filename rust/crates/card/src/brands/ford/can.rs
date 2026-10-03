use super::Error;
use openpilot_can::{packer::Packer, Frame};
use std::collections::BTreeMap;

type Values = BTreeMap<String, f64>;
pub(super) fn copy<'a>(
    source: &Values,
    names: &'a [&'a str],
) -> Result<Vec<(&'a str, f64)>, Error> {
    names
        .iter()
        .map(|name| {
            Ok((
                *name,
                source
                    .get(*name)
                    .copied()
                    .ok_or_else(|| Error::Signal((*name).into()))?,
            ))
        })
        .collect()
}
pub fn message(
    packer: &mut Packer,
    name: &str,
    bus: u8,
    values: &[(&str, f64)],
) -> Result<Frame, Error> {
    Ok(Frame {
        address: packer.dbc.message(name)?.address,
        data: packer.pack(name, values, None)?,
        bus,
    })
}
pub fn lateral(
    packer: &mut Packer,
    bus: u8,
    active: bool,
    curvature: f64,
    counter: Option<u8>,
) -> Result<Frame, Error> {
    let mut values = vec![
        ("LatCtlRampType_D_Rq", 0.),
        ("LatCtlPrecision_D_Rq", 1.),
        ("LatCtlPathOffst_L_Actl", 0.),
        ("LatCtlPath_An_Actl", 0.),
        ("LatCtlCurv_No_Actl", curvature),
        ("HandsOffCnfm_B_Rq", 0.),
    ];
    if let Some(counter) = counter {
        values.extend([
            ("LatCtl_D2_Rq", f64::from(active)),
            ("LatCtlCrv_NoRate2_Actl", 0.),
            ("LatCtlPath_No_Cnt", f64::from(counter)),
            ("LatCtlPath_No_Cs", 0.),
        ]);
        let data = packer.pack("LateralMotionControl2", &values, None)?;
        let data: [u8; 8] = data.try_into().map_err(|_| Error::Numeric)?;
        let curvature = (u16::from(data[2]) << 3) | u16::from(data[3] >> 5);
        let rate = (u16::from(data[6]) << 3) | u16::from(data[7] >> 5);
        let angle = (u16::from(data[3] & 0x1f) << 6) | u16::from(data[4] >> 2);
        let offset = (u16::from(data[4] & 3) << 8) | u16::from(data[5]);
        let checksum = u16::from(active)
            + u16::from(counter)
            + [curvature, rate, angle, offset]
                .into_iter()
                .map(|v| v + (v >> 8))
                .sum::<u16>();
        values.pop();
        values.push(("LatCtlPath_No_Cs", f64::from(255 - (checksum & 255))));
        message(packer, "LateralMotionControl2", bus, &values)
    } else {
        values.extend([
            ("LatCtlRng_L_Max", 0.),
            ("LatCtl_D_Rq", f64::from(active)),
            ("LatCtlCurv_NoRate_Actl", 0.),
        ]);
        message(packer, "LateralMotionControl", bus, &values)
    }
}
pub struct AccCommand {
    pub active: bool,
    pub gas: f64,
    pub accel: f64,
    pub stopping: bool,
    pub brake: bool,
}
pub fn acceleration(packer: &mut Packer, bus: u8, input: AccCommand) -> Result<Frame, Error> {
    message(
        packer,
        "ACCDATA",
        bus,
        &[
            ("AccBrkTot_A_Rq", input.accel),
            ("Cmbb_B_Enbl", f64::from(input.active)),
            ("AccPrpl_A_Rq", input.gas),
            ("AccPrpl_A_Pred", -5.),
            ("AccResumEnbl_B_Rq", f64::from(input.active)),
            ("AccVeh_V_Trg", 145.),
            ("AccBrkPrchg_B_Rq", f64::from(input.brake)),
            ("AccBrkDecel_B_Rq", f64::from(input.brake)),
            ("AccStopStat_B_Rq", f64::from(input.stopping)),
        ],
    )
}
pub fn button(
    packer: &mut Packer,
    bus: u8,
    stock: &Values,
    cancel: bool,
    resume: bool,
    toggle: bool,
) -> Result<Frame, Error> {
    let mut values = copy(
        stock,
        &[
            "HeadLghtHiFlash_D_Stat",
            "TurnLghtSwtch_D_Stat",
            "WiprFront_D_Stat",
            "LghtAmb_D_Sns",
            "AccButtnGapDecPress",
            "AccButtnGapIncPress",
            "AslButtnOnOffCnclPress",
            "AslButtnOnOffPress",
            "LaSwtchPos_D_Stat",
            "CcAslButtnCnclResPress",
            "CcAslButtnDeny_B_Actl",
            "CcAslButtnIndxDecPress",
            "CcAslButtnIndxIncPress",
            "CcAslButtnOffCnclPress",
            "CcAslButtnOnOffCncl",
            "CcAslButtnOnPress",
            "CcAslButtnResDecPress",
            "CcAslButtnResIncPress",
            "CcAslButtnSetDecPress",
            "CcAslButtnSetIncPress",
            "CcAslButtnSetPress",
            "CcButtnOffPress",
            "CcButtnOnOffCnclPress",
            "CcButtnOnOffPress",
            "CcButtnOnPress",
            "HeadLghtHiFlash_D_Actl",
            "HeadLghtHiOn_B_StatAhb",
            "AhbStat_B_Dsply",
            "AccButtnGapTogglePress",
            "WiprFrontSwtch_D_Stat",
            "HeadLghtHiCtrl_D_RqAhb",
        ],
    )?;
    values.extend([
        ("CcAslButtnCnclPress", f64::from(cancel)),
        ("CcAsllButtnResPress", f64::from(resume)),
        ("TjaButtnOnOffPress", f64::from(toggle)),
    ]);
    message(packer, "Steering_Data_FD1", bus, &values)
}
