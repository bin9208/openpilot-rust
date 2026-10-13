use super::Frame;
use crate::models::{ClusterUiState, DetectedVehicle, RadarPoint};

fn optional(value: Option<f64>, format: impl FnOnce(f64) -> String) -> String {
    value.map_or_else(|| "--".into(), format)
}
fn text<'a>(value: Option<&'a str>, fallback: &'a str) -> &'a str {
    value.filter(|value| !value.is_empty()).unwrap_or(fallback)
}
fn nonzero(value: Option<f64>) -> f64 {
    value.filter(|value| *value != 0.0).unwrap_or(0.0)
}
#[must_use]
pub fn data_lines(
    frame: &Frame,
    state: &ClusterUiState,
    shown_time: f64,
    duration: f64,
    segment: &str,
) -> Vec<String> {
    let raw = &frame.state;
    let signal = format!(
        "{}{}",
        if raw.left_signal { "L" } else { "-" },
        if raw.right_signal { "R" } else { "-" }
    );
    let center = optional(frame.lanes.center, |value| format!("{value:+.2}m"));
    let limit = raw.speed_limit_kph.map_or_else(
        || "--".into(),
        |value| format!("{value}:{}", text(raw.speed_limit_source.as_deref(), "-")),
    );
    let cruise = raw
        .cruise_kph
        .map_or_else(|| "--".into(), |value| value.to_string());
    let curve = optional(frame.metadata.road_curvature, |value| {
        format!("{value:+.5}")
    });
    let change = raw.lane_change.as_ref().map_or_else(
        || "idle".into(),
        |side| format!("{side}:{:.2}", raw.lane_change_progress),
    );
    let speed = optional(raw.planned_speed_kph, |value| format!("{value:.0}"));
    let accel = optional(raw.planned_accel_mps2, |value| format!("{value:+.1}"));
    let turn = optional(raw.model_turn_speed_kph, |value| format!("{value:.0}"));
    let engaged = optional(raw.engaged_prob, |value| format!("{:.0}%", value * 100.0));
    let vision = optional(raw.vision_speed_mps, |value| {
        format!("{:.1}kph", value * 3.6)
    });
    let drop = optional(raw.frame_drop_perc, |value| format!("{value:.1}%"));
    let execution = optional(raw.model_execution_time_ms, |value| format!("{value:.0}ms"));
    let available = format!(
        "{}{}",
        if raw.lane_change_available_left == Some(true) {
            "L"
        } else {
            "-"
        },
        if raw.lane_change_available_right == Some(true) {
            "R"
        } else {
            "-"
        }
    );
    let gap = raw
        .cruise_gap
        .map_or_else(|| "--".into(), |value| value.to_string());
    vec![
        format!(
            "device cutin {}   sound {}",
            if raw.recorded_cutin_active {
                "YES"
            } else {
                "NO"
            },
            if raw.recorded_cutin_sound {
                "PROMPT"
            } else {
                "--"
            }
        ),
        format!("t {shown_time:6.1}/{duration:6.1}s   seg {segment}"),
        format!(
            "vEgo {:5.1} km/h   aEgo {:+.2} m/s2",
            state.speed_kph, state.accel_mps2
        ),
        format!(
            "steer {:+.1} deg   limit {limit}   cruise {cruise}",
            nonzero(raw.steering_angle_deg)
        ),
        format!(
            "gear {}   gap {gap}   signals {signal}",
            text(raw.gear_text.as_deref(), "--")
        ),
        format!("curve {curve}   plan {speed}kph {accel}m/s2"),
        format!(
            "lane {:.2}m center {center}   src {}",
            raw.lane_width_m, frame.metadata.lane_position_source
        ),
        format!("lc {change} avail {available} p{:.2}", raw.lane_change_prob),
        format!(
            "model {} eng {engaged} risk {:.2} hb {:.2}",
            text(raw.model_confidence.as_deref(), "--"),
            raw.disengage_risk,
            raw.hard_brake_risk
        ),
        format!(
            "turn {turn}kph ttc {} stop {} drop {drop} exec {execution}",
            nearest_ttc(&raw.detected_vehicles),
            u8::from(raw.should_stop)
        ),
        format!(
            "vision {vision} yaw {:+.3}   detected {}",
            nonzero(raw.vision_yaw_rate_rps),
            detected_summary(&raw.detected_vehicles)
        ),
        format!("radar points {}", radar_summary(&raw.radar_points)),
    ]
}
fn detected_summary(vehicles: &[DetectedVehicle]) -> String {
    if vehicles.is_empty() {
        return "none".into();
    }
    let mut parts: Vec<_> = vehicles
        .iter()
        .take(4)
        .map(|vehicle| {
            let relative = vehicle
                .relative_speed_mps
                .map_or_else(String::new, |value| format!(" {value:+.1}mps"));
            let probability = if vehicle.probability >= 0.995 {
                String::new()
            } else {
                format!(" p{:.0}%", vehicle.probability * 100.0)
            };
            format!(
                "{} {:+.0}/{:+.1}{relative}{probability}{}",
                vehicle.label,
                vehicle.longitudinal_m,
                vehicle.lateral_m,
                if vehicle.cut_in { " cut" } else { "" }
            )
        })
        .collect();
    if vehicles.len() > 4 {
        parts.push(format!("+{}", vehicles.len() - 4));
    }
    parts.join(" ")
}
fn nearest_ttc(vehicles: &[DetectedVehicle]) -> String {
    optional(
        vehicles
            .iter()
            .filter_map(|vehicle| vehicle.ttc_s)
            .reduce(f64::min),
        |value| format!("{value:.1}s"),
    )
}
fn radar_summary(points: &[RadarPoint]) -> String {
    let Some(nearest) = points.iter().min_by(|left, right| {
        left.longitudinal_m
            .max(0.0)
            .total_cmp(&right.longitudinal_m.max(0.0))
    }) else {
        return "none".into();
    };
    let relative = nearest
        .relative_speed_mps
        .map_or_else(String::new, |value| format!(" v{value:+.1}"));
    let probability = nearest
        .probability
        .map_or_else(String::new, |value| format!(" p{:.0}%", value * 100.0));
    format!(
        "{} nearest {} {:.0}/{:+.1}{relative}{probability}",
        points.len(),
        nearest.label,
        nearest.longitudinal_m,
        nearest.lateral_m
    )
}
