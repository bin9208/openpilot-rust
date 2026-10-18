//! Source callback order and Python and/or return-value behavior.
use super::{ops as o, params::Params};
use crate::seal::Result;
use openpilot_log_format::JsonValue;
pub const PREDICATES: [&str; 24] = [
    "driverview",
    "notcar",
    "iscar",
    "logging",
    "ublox",
    "joystick",
    "not_joystick",
    "long_maneuver",
    "lat_maneuver",
    "not_long_maneuver",
    "qcomgps",
    "always_run",
    "only_onroad",
    "only_offroad",
    "enable_updated",
    "enable_dm",
    "enable_xiaoge_data",
    "enable_webrtc",
    "c3x_lite",
    "enable_youtube_low_encoder",
    "enable_youtube_medium_encoder",
    "enable_youtube_encoder",
    "enable_youtube_wide_encoder",
    "enable_cluster_hud",
];
fn gps(p: &mut Params) -> Result<bool> {
    Ok(p.exists("/dev/ttyHS0")? && !p.exists("/persist/comma/use-quectel-gps")?)
}
fn caught(result: Result<bool>) -> Result<bool> {
    match result {
        Ok(value) => Ok(value),
        Err(e)
            if e.downcast_ref::<super::ops::PythonError>()
                .is_some_and(|v| v.kind == "fatal_integer") =>
        {
            Err(e)
        }
        Err(_) => Ok(false),
    }
}
fn cluster(p: &mut Params) -> Result<bool> {
    caught(p.integer("ClusterHud").map(|v| v == 1))
}
pub fn callback(
    name: &str,
    started: &JsonValue,
    not_car: &JsonValue,
    p: &mut Params,
) -> Result<JsonValue> {
    if let Some(names) = name.strip_prefix("all:") {
        for name in names.split(',') {
            if !o::truth(&callback(name, started, not_car, p)?) {
                return Ok(o::boolean(false));
            }
        }
        return Ok(o::boolean(true));
    }
    if let Some(names) = name.strip_prefix("any:") {
        for name in names.split(',') {
            if o::truth(&callback(name, started, not_car, p)?) {
                return Ok(o::boolean(true));
            }
        }
        return Ok(o::boolean(false));
    }
    let running = o::truth(started);
    Ok(match name {
        "driverview" => {
            if running {
                started.clone()
            } else {
                o::boolean(p.boolean("IsDriverViewEnabled")?)
            }
        }
        "notcar" => {
            if running {
                not_car.clone()
            } else {
                started.clone()
            }
        }
        "iscar" => {
            if running {
                o::boolean(!o::truth(not_car))
            } else {
                started.clone()
            }
        }
        "logging" => {
            let run = !o::truth(not_car) || !p.boolean("DisableLogging")?;
            if running {
                o::boolean(run)
            } else {
                started.clone()
            }
        }
        "ublox" => {
            let use_ublox = gps(p)?;
            if use_ublox != p.boolean("UbloxAvailable")? {
                p.put_bool("UbloxAvailable", use_ublox)?;
            }
            if running {
                o::boolean(use_ublox)
            } else {
                started.clone()
            }
        }
        "qcomgps" => {
            if running {
                o::boolean(!gps(p)?)
            } else {
                started.clone()
            }
        }
        "joystick" | "not_joystick" | "long_maneuver" | "lat_maneuver" | "not_long_maneuver"
        | "c3x_lite" => {
            if !running {
                started.clone()
            } else {
                let (key, invert) = match name {
                    "joystick" => ("JoystickDebugMode", false),
                    "not_joystick" => ("JoystickDebugMode", true),
                    "long_maneuver" => ("LongitudinalManeuverMode", false),
                    "not_long_maneuver" => ("LongitudinalManeuverMode", true),
                    "lat_maneuver" => ("LateralManeuverMode", false),
                    _ => ("HardwareC3xLite", false),
                };
                o::boolean(p.boolean(key)? ^ invert)
            }
        }
        "always_run" => o::boolean(true),
        "only_onroad" => started.clone(),
        "only_offroad" => o::boolean(!running),
        "enable_updated" => o::boolean(!running && p.boolean("SoftwareMenu")?),
        "enable_dm" => o::boolean(
            (running || p.boolean("IsDriverViewEnabled")?) && p.integer("DisableDM")? == 0,
        ),
        "enable_xiaoge_data" => o::boolean(p.boolean("ShareData")?),
        "enable_webrtc" => o::boolean(p.integer("DisableDM")? == 2 && !cluster(p)?),
        "enable_cluster_hud" => o::boolean(cluster(p)?),
        "enable_youtube_low_encoder"
        | "enable_youtube_medium_encoder"
        | "enable_youtube_encoder"
        | "enable_youtube_wide_encoder" => o::boolean(caught((|| {
            if name == "enable_youtube_wide_encoder" && !p.default_bool("UseWideCamera")? {
                return Ok(false);
            }
            if p.integer("CarrotYouTubeLive")? <= 0 {
                return Ok(false);
            }
            let quality = p.integer("CarrotYouTubeQuality")?;
            Ok(match name {
                "enable_youtube_low_encoder" => !matches!(quality, 1..=3),
                "enable_youtube_medium_encoder" => quality == 1,
                "enable_youtube_encoder" => quality == 2,
                _ => quality == 3,
            })
        })())?),
        _ => {
            return Err(o::error(
                "AttributeError",
                format!("unknown manager predicate {name}"),
            ))
        }
    })
}
