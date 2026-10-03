//! Native translation of Hyundai `_get_params` capability and safety detection.
use super::{
    bus::{contains, size, CanBus, Fingerprints},
    flags::{self as f, ext, safety},
};
use openpilot_cereal::car_capnp::car_params::SafetyModel;

pub struct DetectionInput<'a> {
    pub candidate: &'a str,
    pub fingerprints: &'a Fingerprints,
    pub flags: u32,
    pub has_radar_dbc: bool,
    pub camera_scc: i32,
    pub hda2: i32,
    pub radar_tracks: i32,
    pub alpha_long: bool,
}

#[derive(Debug)]
pub struct Detection {
    pub flags: u32,
    pub ext_flags: u32,
    pub bsm: bool,
    pub radar_unavailable: bool,
    pub longitudinal: bool,
    pub safety_model: SafetyModel,
    pub safety_param: u16,
    pub bus: CanBus,
}

pub fn detect(input: DetectionInput<'_>) -> Detection {
    let DetectionInput {
        candidate,
        fingerprints: fp,
        mut flags,
        has_radar_dbc,
        camera_scc,
        hda2,
        radar_tracks,
        alpha_long,
    } = input;
    if camera_scc > 0 {
        flags |= f::CAMERA_SCC;
    }
    let bus = CanBus::fingerprint(fp, hda2 > 0, camera_scc);
    let mut ext_flags = if candidate == "KIA_SORENTO" {
        ext::RADAR_GROUP4
    } else {
        0
    };
    let mut safety_param = 0;
    let (bsm, safety_model) = if flags & f::CANFD != 0 {
        if contains(fp, bus.ecan, 0x105) {
            flags |= f::HYBRID;
        }
        if size(fp, bus.ecan, 0xfa) == Some(32) && size(fp, bus.ecan, 0x230) == Some(32) {
            ext_flags |= ext::EV_MODE_230;
        }
        if candidate != "HYUNDAI_TUCSON_4TH_GEN" && contains(fp, bus.cam, 0xcb) {
            flags |= f::ANGLE_CONTROL;
        }
        if contains(fp, bus.acan, 0x210) {
            ext_flags |= ext::RADAR_GROUP1;
        } else if contains(fp, bus.acan, 0x400) && contains(fp, bus.acan, 0x41d) {
            ext_flags |= ext::RADAR_GROUP3;
        }
        if (0x235..0x249).all(|address| size(fp, bus.acan, address) == Some(32)) {
            ext_flags |= ext::CORNER_235;
        }
        if (0x180..0x185).all(|address| size(fp, bus.acan, address) == Some(32)) {
            ext_flags |= ext::CORNER_180;
        }
        if hda2 > 0 {
            flags |= f::HDA2;
            let alt = if camera_scc > 0 {
                contains(fp, bus.acan, 0x110)
            } else {
                contains(fp, bus.cam, 0x110) || !contains(fp, bus.cam, 0x2a4)
            };
            if alt {
                flags |= f::ALT_STEERING;
            }
        }
        if !contains(fp, bus.ecan, 0x1cf) {
            flags |= f::ALT_BUTTONS;
        }
        if contains(fp, bus.ecan, 0x40) {
            flags |= f::ALT_GEARS;
        } else if contains(fp, bus.ecan, 69) {
            ext_flags |= ext::GEARS_69;
        } else if contains(fp, bus.ecan, 112) {
            flags |= f::ALT_GEARS_2;
        } else if !contains(fp, bus.ecan, 0x130) {
            ext_flags |= ext::GEARS_NONE;
        }
        if flags & f::HDA2 != 0 {
            safety_param |= safety::LKA_STEERING;
            if flags & f::ALT_STEERING != 0 {
                safety_param |= safety::ALT_STEERING;
            }
        }
        if flags & f::ALT_BUTTONS != 0 {
            safety_param |= safety::ALT_BUTTONS;
        }
        if flags & f::CAMERA_SCC != 0 {
            safety_param |= safety::CAMERA_SCC;
        }
        if contains(fp, bus.ecan, 0x1fa) {
            ext_flags |= ext::NAVI_CLUSTER;
        }
        (contains(fp, bus.ecan, 0x1ba), SafetyModel::HyundaiCanfd)
    } else {
        if contains(fp, 2, 0x485) {
            flags |= f::SEND_LFA;
        }
        if contains(fp, 0, 0x38d) || contains(fp, 2, 0x38d) {
            flags |= f::USE_FCA;
        }
        if flags & f::CAMERA_SCC != 0 {
            safety_param |= safety::CAMERA_SCC;
        }
        if contains(fp, 0, 1348) {
            ext_flags |= ext::NAVI_CLUSTER;
        }
        if contains(fp, 0, 1157) || contains(fp, 2, 1157) {
            ext_flags |= ext::HAS_LFAHDA;
        }
        (
            contains(fp, 0, 0x58b),
            if flags & f::LEGACY != 0 {
                SafetyModel::HyundaiLegacy
            } else {
                SafetyModel::Hyundai
            },
        )
    };
    if flags & f::ALT_LIMITS != 0 {
        safety_param |= safety::ALT_LIMITS;
    }
    let mut radar_unavailable = !contains(fp, 1, 0x500) || !has_radar_dbc;
    let mut longitudinal = alpha_long;
    if flags & f::CAMERA_SCC != 0 || radar_tracks > 0 || radar_tracks == -2 {
        radar_unavailable = false;
        longitudinal = camera_scc < 3;
    }
    if longitudinal {
        safety_param |= safety::LONG;
    }
    if flags & f::HYBRID != 0 {
        safety_param |= safety::HYBRID_GAS;
    } else if flags & f::EV != 0 {
        safety_param |= safety::EV_GAS;
    } else if flags & f::FCEV != 0 {
        safety_param |= safety::FCEV_GAS;
    }
    Detection {
        flags,
        ext_flags,
        bsm,
        radar_unavailable,
        longitudinal,
        safety_model,
        safety_param,
        bus,
    }
}
