use super::{
    bus::{contains, CanBus},
    flags as f,
    parameters::{setting_int, ParamsInput},
    Error,
};
use capnp::message::{Builder, HeapAllocator};
use openpilot_cereal::car_capnp::car_params;

pub fn lines(
    input: &ParamsInput<'_>,
    params: &Builder<HeapAllocator>,
) -> Result<Vec<String>, Error> {
    let cp = params.get_root_as_reader::<car_params::Reader<'_>>()?;
    let camera = setting_int(input.settings, "HyundaiCameraSCC")?;
    let hda = setting_int(input.settings, "CanfdHDA2")? > 0;
    let bus = CanBus::fingerprint(input.fingerprints, hda, camera);
    let has = |channel, address| contains(input.fingerprints, channel, address);
    let mut lines = Vec::new();
    if camera > 0 {
        lines.push("$$$CAMERA_SCC toggled...".into());
    }
    if cp.get_flags() & f::CANFD != 0 {
        if input.candidate != "HYUNDAI_TUCSON_4TH_GEN" && has(bus.cam, 0xcb) {
            lines.push("##### Anglecontrol detected (LFA_ALT)".into());
        }
        let acan = input
            .fingerprints
            .iter()
            .find(|(channel, _)| *channel == bus.acan)
            .map(|(_, entries)| entries.as_slice())
            .unwrap_or(&[]);
        lines.push(format!(
            "ACAN= {{{}}}",
            acan.iter()
                .map(|(address, len)| format!("{address}: {len}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if cp.get_ext_flags() & f::ext::RADAR_GROUP1 != 0 {
            lines.push("##### Radar Group 1 detected (0x210)".into());
        } else if cp.get_ext_flags() & f::ext::RADAR_GROUP3 != 0 {
            lines.push("##### Radar Group 3 detected (0x400-0x41D)".into());
        }
        if cp.get_ext_flags() & f::ext::CORNER_235 != 0 {
            lines.push("##### Corner radar objects 0x235 group detected".into());
        }
        if cp.get_ext_flags() & f::ext::CORNER_180 != 0 {
            lines.push("##### Corner radar objects 0x180 group detected".into());
        }
        if hda {
            lines.push("$$$CANFD HDA2".into());
            if camera > 0 {
                if has(bus.acan, 0x110) {
                    lines.push("$$$CANFD ALT_STEERING1".into());
                }
            } else {
                if has(bus.cam, 0x110) {
                    lines.push("$$$CANFD ALT_STEERING1".into());
                }
                if !has(bus.cam, 0x2a4) {
                    lines.push("$$$CANFD ALT_STEERING2".into());
                }
            }
        } else {
            lines.push("$$$CANFD non HDA2".into());
        }
        if !has(bus.ecan, 0x1cf) {
            lines.push("$$$CANFD ALT_BUTTONS".into());
        }
        lines.push(
            if has(bus.ecan, 0x40) {
                "$$$CANFD ALT_GEARS"
            } else if has(bus.ecan, 69) {
                "$$$CANFD GEARS_69"
            } else if has(bus.ecan, 112) {
                "$$$CANFD ALT_GEARS_2"
            } else if has(bus.ecan, 0x130) {
                "$$$CANFD GEAR_SHIFTER present"
            } else {
                "$$$CANFD GEARS_NONE"
            }
            .into(),
        );
    } else {
        lines.push(format!("$$$ enableBsm = {}", boolean(cp.get_enable_bsm())));
        if has(2, 0x485) {
            lines.push("$$$SEND_LFA".into());
        }
        if has(0, 0x38d) || has(2, 0x38d) {
            lines.push("$$$USE_FCA".into());
        }
        if cp.get_flags() & f::LEGACY != 0 {
            lines.push("$$$Legacy Safety Model".into());
        }
        if cp.get_flags() & f::CAMERA_SCC != 0 {
            lines.push("$$$CAMERA_SCC".into());
        }
    }
    let radar = setting_int(input.settings, "EnableRadarTracks")?;
    if cp.get_flags() & f::CAMERA_SCC != 0 || radar > 0 || radar == -2 {
        lines.push(format!(
            "$$$OenpilotLongitudinalControl = True, CAMERA_SCC({}) or RadarTracks{radar}",
            cp.get_flags() & f::CAMERA_SCC
        ));
    } else {
        lines.push(format!(
            "$$$OenpilotLongitudinalControl = {}",
            boolean(input.alpha_long)
        ));
    }
    if cp.get_flags() & f::CANFD != 0 {
        lines.push(format!("$$$$$ CanFD ECAN = {}", bus.ecan));
        lines.push(format!(
            "$$$$ NaviCluster = {}",
            boolean(has(bus.ecan, 0x1fa))
        ));
    } else {
        if has(0, 1348) {
            lines.push("$$$$ NaviCluster = True".into());
        }
        if has(0, 1157) || has(2, 1157) {
            lines.push("$$$$ HasLFAHDA".into());
        }
        if has(0, 1007) {
            lines.push("#### cruiseButtonAlt".into());
        }
    }
    lines.push(format!("$$$$ enableBsm = {}", boolean(cp.get_enable_bsm())));
    Ok(lines)
}

fn boolean(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}
