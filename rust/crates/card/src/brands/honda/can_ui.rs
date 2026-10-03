use super::{can::send, config::Config, state::Stock, Error, BOSCH_EXT_HUD};
use openpilot_can::{packer::Packer, Frame};
use openpilot_cereal::car_capnp::car_control::h_u_d_control;

pub(super) struct Ui<'a> {
    pub enabled: bool,
    pub pcm_speed: f64,
    pub pcm_accel: i32,
    pub cruise: f64,
    pub hud: h_u_d_control::Reader<'a>,
    pub steer: u8,
    pub metric: bool,
    pub acc_hud: &'a Stock,
    pub lkas_hud: &'a Stock,
}
pub(super) fn ui(packer: &mut Packer, config: &Config, input: Ui<'_>) -> Result<Vec<Frame>, Error> {
    let mut sends = Vec::new();
    if config.longitudinal {
        let mut values = vec![
            ("CRUISE_SPEED", input.cruise),
            ("ENABLE_MINI_CAR", f64::from(input.enabled)),
            (
                "HUD_DISTANCE",
                f64::from(input.hud.get_lead_distance_bars()),
            ),
            ("IMPERIAL_UNIT", f64::from(!input.metric)),
            (
                "HUD_LEAD",
                if input.enabled && input.hud.get_lead_visible() {
                    2.
                } else if input.enabled {
                    1.
                } else {
                    0.
                },
            ),
            ("SET_ME_X01_2", 1.),
        ];
        if config.bosch() {
            values.extend([
                ("ACC_ON", f64::from(input.enabled)),
                ("FCM_OFF", 1.),
                ("FCM_OFF_2", 1.),
            ]);
        } else {
            values.extend([
                ("ACC_ON", f64::from(input.enabled)),
                ("PCM_SPEED", input.pcm_speed * 3.6),
                ("PCM_GAS", f64::from(input.pcm_accel)),
                ("SET_ME_X01", 1.),
            ]);
            let stock = input.acc_hud.values("acc_hud")?;
            for name in ["FCM_OFF", "FCM_OFF_2", "FCM_PROBLEM", "ICONS"] {
                values.push((
                    name,
                    stock
                        .get(name)
                        .copied()
                        .ok_or_else(|| Error::Signal(name.into()))?,
                ));
            }
        }
        sends.push(send(packer, "ACC_HUD", config.bus.pt, &values)?);
    }
    let mut values = vec![
        ("SET_ME_X41", 65.),
        ("STEERING_REQUIRED", f64::from(input.steer)),
        ("SOLID_LANES", f64::from(input.hud.get_lanes_visible())),
        ("BEEP", 0.),
    ];
    if config.radarless() {
        values.extend([
            ("LANE_LINES", 3.),
            ("DASHED_LANES", f64::from(input.hud.get_lanes_visible())),
            (
                "LKAS_PROBLEM",
                input
                    .lkas_hud
                    .values("lkas_hud")?
                    .get("LKAS_PROBLEM")
                    .copied()
                    .ok_or_else(|| Error::Signal("LKAS_PROBLEM".into()))?,
            ),
        ]);
    }
    if config.flags & BOSCH_EXT_HUD == 0 {
        values.push(("SET_ME_X48", 72.));
    }
    if config.flags & BOSCH_EXT_HUD != 0 && !config.longitudinal {
        sends.push(send(packer, "LKAS_HUD_A", config.bus.lkas, &values)?);
        sends.push(send(packer, "LKAS_HUD_B", config.bus.lkas, &values)?);
    } else {
        sends.push(send(packer, "LKAS_HUD", config.bus.lkas, &values)?);
    }
    if config.bosch() && !config.radarless() && config.longitudinal {
        sends.push(send(
            packer,
            "RADAR_HUD",
            config.bus.pt,
            &[("CMBS_OFF", 1.), ("SET_TO_1", 1.)],
        )?);
        if config.candidate == "HONDA_CIVIC_BOSCH" {
            sends.push(send(packer, "LEGACY_BRAKE_COMMAND", config.bus.pt, &[])?);
        }
    }
    Ok(sends)
}
