use super::{
    can::{copy, message},
    Error,
};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control as hud_control;
use std::collections::BTreeMap;
type Values = BTreeMap<String, f64>;
pub struct UiInput<'a> {
    pub main_on: bool,
    pub enabled: bool,
    pub alert: bool,
    pub hud: hud_control::Reader<'a>,
}
pub fn lkas_ui(
    packer: &mut Packer,
    bus: u8,
    stock: &Values,
    input: UiInput<'_>,
) -> Result<Frame, Error> {
    let hud = input.hud;
    let lines = if input.enabled {
        (if hud.get_left_lane_depart() {
            4
        } else {
            i32::from(hud.get_left_lane_visible())
        }) + (if hud.get_right_lane_depart() {
            20
        } else {
            5 * i32::from(hud.get_right_lane_visible())
        })
    } else if input.main_on {
        0
    } else if hud.get_left_lane_depart() {
        3
    } else if hud.get_right_lane_depart() {
        15
    } else {
        30
    };
    let mut values = copy(
        stock,
        &[
            "FeatConfigIpmaActl",
            "FeatNoIpmaActl",
            "PersIndexIpma_D_Actl",
            "AhbcRampingV_D_Rq",
            "LaDenyStats_B_Dsply",
            "CamraDefog_B_Req",
            "CamraStats_D_Dsply",
            "DasAlrtLvl_D_Dsply",
            "DasStats_D_Dsply",
            "DasWarn_D_Dsply",
            "AhbHiBeam_D_Rq",
            "Passthru_63",
            "Passthru_48",
        ],
    )?;
    values.extend([
        ("LaActvStats_D_Dsply", f64::from(lines)),
        ("LaHandsOff_D_Dsply", f64::from(input.alert)),
    ]);
    message(packer, "IPMA_Data", bus, &values)
}
pub struct AccUiInput<'a> {
    pub ui: UiInput<'a>,
    pub longitudinal: bool,
    pub standstill: bool,
    pub show_distance: bool,
}
pub fn acc_ui(
    packer: &mut Packer,
    bus: u8,
    stock: &Values,
    input: AccUiInput<'_>,
) -> Result<Frame, Error> {
    let hud = input.ui.hud;
    let status = if input.ui.enabled {
        if hud.get_left_lane_depart() {
            3
        } else if hud.get_right_lane_depart() {
            4
        } else {
            2
        }
    } else if input.ui.main_on {
        if hud.get_left_lane_depart() {
            5
        } else if hud.get_right_lane_depart() {
            6
        } else {
            1
        }
    } else {
        0
    };
    let names = [
        "HaDsply_No_Cs",
        "HaDsply_No_Cnt",
        "AccStopStat_D_Dsply",
        "AccTrgDist2_D_Dsply",
        "AccStopRes_B_Dsply",
        "TjaWarn_D_Rq",
        "TjaMsgTxt_D_Dsply",
        "IaccLamp_D_Rq",
        "AccMsgTxt_D2_Rq",
        "FcwDeny_B_Dsply",
        "FcwMemStat_B_Actl",
        "AccTGap_B_Dsply",
        "CadsAlignIncplt_B_Actl",
        "AccFllwMde_B_Dsply",
        "CadsRadrBlck_B_Actl",
        "CmbbPostEvnt_B_Dsply",
        "AccStopMde_B_Dsply",
        "FcwMemSens_D_Actl",
        "FcwMsgTxt_D_Rq",
        "AccWarn_D_Dsply",
        "FcwVisblWarn_B_Rq",
        "FcwAudioWarn_B_Rq",
        "AccTGap_D_Dsply",
        "AccMemEnbl_B_RqDrv",
        "FdaMem_B_Stat",
    ];
    let mut values = copy(stock, &names)?;
    values.push(("Tja_D_Stat", f64::from(status)));
    for (name, value) in &mut values {
        if input.longitudinal {
            *value = match *name {
                "AccStopStat_D_Dsply" => {
                    if input.standstill {
                        2.
                    } else {
                        0.
                    }
                }
                "AccMsgTxt_D2_Rq" | "AccWarn_D_Dsply" => 0.,
                "AccTGap_B_Dsply" => f64::from(input.show_distance),
                "AccFllwMde_B_Dsply" => f64::from(hud.get_lead_visible()),
                "AccStopMde_B_Dsply" => f64::from(input.standstill),
                "AccTGap_D_Dsply" => f64::from(hud.get_lead_distance_bars()),
                _ => *value,
            };
        }
        if *name == "FcwVisblWarn_B_Rq" && input.ui.alert {
            *value = 1.;
        }
    }
    message(packer, "ACCDATA_3", bus, &values)
}
