use super::{
    bus::CanBus,
    cluster_fields,
    cluster_hud::{self, HudInput},
    flags as f,
    lead::Lead,
    wire::{get, remove_counter, set, CanWriter, Values},
    Error,
};
use openpilot_can::Frame;

#[derive(Default)]
pub struct ClusterMessages<'a> {
    pub lfahda: Option<&'a Values>,
    pub adrv161: Option<&'a Values>,
    pub adrv200: Option<&'a Values>,
    pub adrv1ea: Option<&'a Values>,
    pub ccnc162: Option<&'a Values>,
    pub buttons: Option<&'a Values>,
    pub buttons_alt2: Option<&'a Values>,
    pub scc: Option<&'a Values>,
}

pub struct ClusterInput<'a> {
    pub bus: CanBus,
    pub flags: u32,
    pub hud: HudInput,
    pub main_mode: bool,
    pub acc_mode: f64,
    pub speed: f64,
    pub stopping: bool,
    pub interlock: bool,
    pub button_name: &'a str,
    pub lane_changing: u8,
    pub corner_radar: i32,
    pub debug: i32,
    pub leads: &'a [Lead],
    pub path: (&'a [f64], &'a [f64]),
    pub hud_lateral: Option<f64>,
    pub lane_warnings: [bool; 2],
}

pub use super::cluster_buttons::alt2_request;

pub fn messages(
    writer: &mut CanWriter,
    i: &ClusterInput<'_>,
    source: &ClusterMessages<'_>,
) -> Result<Vec<Frame>, Error> {
    if i.flags & f::CAMERA_SCC == 0 {
        return Ok(Vec::new());
    }
    let frame = i.hud.frame;
    let hda = match source.lfahda {
        Some(data) => get(data, "HDA_CntrlModSta")?,
        None => 0.,
    };
    let lfa = match source.lfahda {
        Some(data) => get(data, "HDA_LFA_SymSta")?,
        None => 0.,
    };
    let mut result = Vec::with_capacity(6);
    if frame.is_multiple_of(2) {
        if source.buttons_alt2.is_some() {
            result.push(alt2_request(writer, i, source)?);
        } else if let Some(original) = source.buttons {
            let mut data = original.clone();
            data.insert("NORMAL_CRUISE_MAIN_BTN".into(), 0.);
            if lfa == 0. && !frame.is_multiple_of(200) && frame % 200 < 12 {
                data.insert("LFA_BTN".into(), 1.);
            }
            if i.hud.enabled && !i.interlock {
                if !i.main_mode {
                    if frame % 200 > 10 && frame % 200 <= 16 && i.speed > 3. {
                        data.insert("ADAPTIVE_CRUISE_MAIN_BTN".into(), 1.);
                    }
                } else if [0., 4.].contains(&i.acc_mode) {
                    if frame % 200 > 10 && frame % 200 <= 16 && i.speed > 3. {
                        data.insert("CRUISE_BUTTONS".into(), 2.);
                    }
                } else if source
                    .scc
                    .is_some_and(|data| data.get("InfoDisplay") == Some(&4.))
                {
                    if frame % 30 > 10 && frame % 30 <= 16 && !i.stopping {
                        data.insert("CRUISE_BUTTONS".into(), 2.);
                    }
                } else if source
                    .adrv1ea
                    .is_some_and(|data| data.get("HDA_MODE2") == Some(&0.))
                    && frame % 1000 > 10
                    && frame % 1000 <= 16
                    && i.speed > 3.
                {
                    data.insert("ADAPTIVE_CRUISE_MAIN_BTN".into(), 1.);
                }
            }
            result.push(writer.frame(i.button_name, i.bus.cam, &data, None)?);
        }
    }
    if frame.is_multiple_of(5) {
        if let Some(original) = source.adrv161 {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            cluster_hud::hud(&mut data, &i.hud)?;
            result.push(writer.frame("ADRV_0x161", i.bus.ecan, &data, counter)?);
        }
        if let Some(original) = source.adrv200 {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            data.insert("TauGapSet".into(), i.hud.gap);
            result.push(writer.frame("ADRV_0x200", i.bus.ecan, &data, counter)?);
        }
        if let Some(original) = source.adrv1ea {
            let mut data = original.clone();
            let counter = remove_counter(&mut data)?;
            set(
                &mut data,
                &[
                    (
                        "LEFT_BLINK_HOLD",
                        if i.lane_changing == 3 { 1. } else { 0. },
                    ),
                    (
                        "RIGHT_BLINK_HOLD",
                        if i.lane_changing == 4 { 1. } else { 0. },
                    ),
                ],
            );
            cluster_fields::lane_lines(
                &mut data,
                (i.hud.steering_angle, i.hud.active, i.hud.desire),
            );
            cluster_fields::normalize_corner(&mut data, false)?;
            result.push(writer.frame("ADRV_0x1ea", i.bus.ecan, &data, counter)?);
        }
        if let Some(original) = source.ccnc162 {
            let mut data = original.clone();
            cluster_fields::normalize_corner(&mut data, true)?;
            let front = get(&data, "FF_DETECT")?;
            if [1., 2.].contains(&front) {
                data.insert("FF_DETECT".into(), front + 2.);
            }
            cluster_fields::front_lead(&mut data, (i.leads, i.path, i.hud.enabled, i.hud_lateral))?;
            if (i.lane_warnings[0] && !i.hud.blinker[0])
                || (i.lane_warnings[1] && !i.hud.blinker[1])
            {
                data.insert("VIBRATE".into(), 1.);
            }
            cluster_fields::hide_service_warning(&mut data);
            if i.debug > 0 {
                set(&mut data, &[("FAULT_LSS", 0.), ("FAULT_DAS", 0.)]);
            }
            result.push(writer.frame("CCNC_0x162", i.bus.ecan, &data, None)?);
        }
    }
    if i.corner_radar > 0 && hda == 0. {
        let bytes = match frame % 500 {
            10 | 20 | 30 => Some([0., 0., 128., 138., 50., 48., 1., 0.]),
            40 | 50 | 60 => Some([255.; 8]),
            _ => None,
        };
        if let Some(bytes) = bytes {
            let data = bytes
                .into_iter()
                .enumerate()
                .map(|(index, value)| (format!("BYTE_{}", index + 1), value))
                .collect();
            result.push(writer.frame("NEW_MSG_4B9", i.bus.cam, &data, None)?);
        }
    }
    Ok(result)
}

pub fn lfa_cluster(
    writer: &mut CanWriter,
    bus: CanBus,
    source: Option<&Values>,
    active: [bool; 2],
) -> Result<Vec<Frame>, Error> {
    let Some(original) = source else {
        return Ok(Vec::new());
    };
    let mut data = original.clone();
    let counter = remove_counter(&mut data)?;
    set(
        &mut data,
        &[
            ("HDA_CntrlModSta", if active[0] { 2. } else { 0. }),
            ("HDA_LFA_SymSta", if active[1] { 2. } else { 0. }),
        ],
    );
    Ok(vec![writer.frame(
        "LFAHDA_CLUSTER",
        bus.ecan,
        &data,
        counter,
    )?])
}

pub fn lfa_icon(
    writer: &mut CanWriter,
    bus: CanBus,
    source: Option<&Values>,
    active: [bool; 2],
) -> Result<Vec<Frame>, Error> {
    let Some(original) = source else {
        return Ok(Vec::new());
    };
    let mut data = original.clone();
    let counter = remove_counter(&mut data)?;
    set(
        &mut data,
        &[
            (
                "LFA_ICON",
                if active[0] {
                    2.
                } else if active[1] {
                    1.
                } else {
                    0.
                },
            ),
            (
                "LKA_ICON",
                if active[0] {
                    4.
                } else if active[1] {
                    3.
                } else {
                    0.
                },
            ),
        ],
    );
    cluster_fields::hide_alerts(&mut data, false, 0, false)?;
    Ok(vec![writer.frame(
        "ADRV_0x161",
        bus.ecan,
        &data,
        counter,
    )?])
}
