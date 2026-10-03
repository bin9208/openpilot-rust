use super::{
    flags as f,
    wire::{boolean, crc8, get, set, values, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

#[derive(serde::Deserialize)]
pub struct LegacyAccInput {
    pub enabled: bool,
    pub accel: f64,
    pub index: u32,
    pub gap: f64,
    pub lead_visible: bool,
    pub lead_distance: f64,
    pub lead_speed: f64,
    pub set_speed: f64,
    pub stopping: bool,
    pub long_override: bool,
    pub available: bool,
    pub brake_hold: bool,
    pub brake_pressed: bool,
    pub paddle: u8,
    pub soft_hold: u8,
    pub soft_hold_mode: u8,
    pub carrot_cruise: u8,
    pub carrot_accel: f64,
    pub band_upper: f64,
    pub band_lower: f64,
    pub jerk_u: f64,
    pub jerk_l: f64,
    pub use_fca: bool,
    pub flags: u32,
    pub casper_fca: bool,
}

#[derive(Default)]
pub struct LegacySccMessages<'a> {
    pub scc11: Option<&'a Values>,
    pub scc12: Option<&'a Values>,
    pub scc14: Option<&'a Values>,
    pub fca11: Option<&'a Values>,
}

pub fn suppress_fca_fault(data: &mut Values) -> Result<(), Error> {
    let fault = get(data, "FCA_Failinfo")? != 0. || get(data, "FCA_Status")? == 3.;
    data.insert("FCA_Failinfo".into(), 0.);
    if fault {
        set(
            data,
            &[
                ("FCA_Status", 2.),
                ("CF_VSM_Prefill", 0.),
                ("CF_VSM_HBACmd", 0.),
                ("CF_VSM_Warn", 0.),
                ("CF_VSM_BeltCmd", 0.),
                ("CR_VSM_DecCmd", 0.),
                ("FCA_CmdAct", 0.),
                ("FCA_StopReq", 0.),
                ("CF_VSM_DecCmdAct", 0.),
            ],
        );
    }
    Ok(())
}

pub fn commands(
    writer: &mut CanWriter,
    input: &LegacyAccInput,
    source: Option<LegacySccMessages<'_>>,
) -> Result<Vec<Frame>, Error> {
    let camera = source.is_some();
    let i = input;
    let available = i.available && (!camera || i.paddle == 0);
    let mut long_enabled = i.enabled || (i.soft_hold > 0 && i.soft_hold_mode == 2);
    let stop = i.stopping || (i.soft_hold > 0 && i.soft_hold_mode == 2);
    let mut accel = i.accel;
    if camera && long_enabled {
        match i.carrot_cruise {
            1 => {
                long_enabled = false;
                accel = -0.5;
            }
            2 => accel = i.carrot_accel,
            _ => {}
        }
    }
    let (mode12, mode14) = if !long_enabled || i.brake_hold {
        (0., 4.)
    } else if i.brake_pressed {
        (1., 1.)
    } else if i.long_override {
        (2., 2.)
    } else {
        (1., 1.)
    };
    let d = i.lead_distance;
    let obj_gap = if d == 0. {
        0.
    } else if d < 25. {
        2.
    } else if d < 40. {
        3.
    } else if d < 70. {
        4.
    } else {
        5.
    };
    let obj_gap2 = if obj_gap == 0. {
        0.
    } else if i.lead_speed < -0.2 {
        2.
    } else {
        1.
    };
    let source = source.unwrap_or_default();
    let mut result = Vec::with_capacity(4);
    if !camera || source.scc11.is_some() {
        let mut data = source.scc11.cloned().unwrap_or_default();
        set(
            &mut data,
            &[
                ("MainMode_ACC", boolean(available)),
                ("TauGapSet", i.gap),
                ("VSetDis", if i.enabled { i.set_speed } else { 0. }),
                ("AliveCounterACC", f64::from(i.index % 16)),
                (
                    "SCCInfoDisplay",
                    if i.soft_hold > 1 && i.enabled { 4. } else { 0. },
                ),
                ("ObjValid", boolean(i.lead_visible)),
                ("ACC_ObjStatus", boolean(i.lead_visible)),
                ("ACC_ObjLatPos", 0.),
                ("ACC_ObjRelSpd", i.lead_speed),
                ("ACC_ObjDist", i.lead_distance.trunc()),
                ("DriverAlertDisplay", 0.),
            ],
        );
        result.push(writer.frame("SCC11", 0, &data, None)?);
    }
    if !camera || source.scc12.is_some() {
        let mut data = source.scc12.cloned().unwrap_or_default();
        set(
            &mut data,
            &[
                ("ACCMode", mode12),
                ("StopReq", boolean(stop)),
                ("aReqRaw", if !camera && stop { 0. } else { accel }),
                ("aReqValue", accel),
                ("CR_VSM_ChkSum", 0.),
                ("CR_VSM_Alive", f64::from(i.index % 15)),
            ],
        );
        if camera {
            data.insert("ACCFailInfo".into(), 0.);
        } else if !i.use_fca {
            set(&mut data, &[("CF_VSM_ConfMode", 1.), ("AEB_Status", 1.)]);
        }
        let first = writer.frame("SCC12", 0, &data, None)?;
        let sum = first
            .data
            .iter()
            .map(|v| u16::from(v / 16 + v % 16))
            .sum::<u16>();
        data.insert("CR_VSM_ChkSum".into(), f64::from(16 - sum % 16));
        result.push(writer.frame("SCC12", 0, &data, None)?);
    }
    if !camera || source.scc14.is_some() {
        let mut data = source.scc14.cloned().unwrap_or_default();
        set(
            &mut data,
            &[
                ("ComfortBandUpper", i.band_upper),
                ("ComfortBandLower", i.band_lower),
                ("JerkUpperLimit", i.jerk_u),
                (
                    "JerkLowerLimit",
                    if camera && !long_enabled {
                        0.
                    } else {
                        i.jerk_l
                    },
                ),
                ("ACCMode", mode14),
                ("ObjGap", obj_gap),
                ("ObjDistStat", obj_gap2),
            ],
        );
        result.push(writer.frame("SCC14", 0, &data, None)?);
    }
    let fca = if camera && i.casper_fca {
        source.fca11.cloned()
    } else if !camera && i.use_fca && i.flags & f::CAMERA_SCC == 0 {
        Some(values(&[
            ("CR_FCA_Alive", f64::from(i.index % 15)),
            ("PAINT1_Status", 1.),
            ("FCA_DrvSetStatus", 1.),
            ("FCA_Status", 1.),
        ]))
    } else {
        None
    };
    if let Some(mut data) = fca {
        if camera {
            suppress_fca_fault(&mut data)?;
        }
        let first = writer.frame("FCA11", 0, &data, None)?;
        let checked = first.data.get(..7).ok_or(Error::Numeric)?;
        data.insert(
            "CR_FCA_ChkSum".into(),
            f64::from(crc8(checked, 0x1d, 0xfd ^ 0xdf, 0xdf)),
        );
        result.push(writer.frame("FCA11", 0, &data, None)?);
    }
    Ok(result)
}

pub fn options(writer: &mut CanWriter, flags: u32) -> Result<Vec<Frame>, Error> {
    let mut frames = vec![writer.frame(
        "SCC13",
        0,
        &values(&[
            ("SCCDrvModeRValue", 2.),
            ("SCC_Equip", 1.),
            ("Lead_Veh_Dep_Alert_USM", 2.),
        ]),
        None,
    )?];
    if flags & f::CAMERA_SCC == 0 {
        frames.push(writer.frame(
            "FCA12",
            0,
            &values(&[("FCA_DrvSetState", 2.), ("FCA_USM", 1.)]),
            None,
        )?);
    }
    Ok(frames)
}

pub fn radar_option(writer: &mut CanWriter) -> Result<Frame, Error> {
    writer.frame(
        "FRT_RADAR11",
        0,
        &values(&[("CF_FCA_Equip_Front_Radar", 1.)]),
        None,
    )
}
