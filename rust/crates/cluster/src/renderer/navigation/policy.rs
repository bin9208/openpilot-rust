use crate::models::{NaviDashboardState, NaviLiveState, NaviTrafficLightInfo};

#[derive(Clone, Copy)]
pub enum Signal {
    Red,
    Left,
    Uturn,
    Straight,
    Right,
}

#[must_use]
pub fn live_panel_visible(navi: Option<&NaviLiveState>) -> bool {
    navi.is_some_and(|navi| {
        navi.current.is_some()
            || navi.next.is_some()
            || navi.lane_current.is_some()
            || navi.route.is_some()
            || navi.crossroad.as_ref().is_some_and(|v| v.visible)
            || navi
                .speed
                .as_ref()
                .is_some_and(|v| v.sdi_type.is_some() || v.section_active)
            || navi.status.as_ref().is_some_and(|v| v.guidance_active)
    })
}

#[must_use]
pub fn map_frame_present(dashboard: Option<&NaviDashboardState>) -> bool {
    dashboard.is_some_and(|v| {
        v.connected
            && !v.map_stream_stalled
            && v.media
                .iter()
                .any(|frame| frame.key == "render:map_main" && frame.present)
    })
}

#[must_use]
pub fn signal_active(
    traffic: Option<&NaviTrafficLightInfo>,
    signal: Signal,
    seconds: Option<i64>,
) -> bool {
    let Some(traffic) = traffic else {
        return false;
    };
    let flag = match signal {
        Signal::Red => traffic.red_on,
        Signal::Left => traffic.left_on,
        Signal::Uturn => traffic.uturn_on,
        Signal::Straight => traffic.straight_on,
        Signal::Right => traffic.right_on,
    };
    flag.unwrap_or(seconds.is_some())
}

#[must_use]
pub fn primary_signal_seconds(
    traffic: Option<&NaviTrafficLightInfo>,
    remaining: [Option<i64>; 5],
) -> (Option<i64>, bool) {
    let Some(traffic) = traffic else {
        return (None, false);
    };
    let ordered = [
        (Signal::Red, remaining[0].or(traffic.red_s), true),
        (Signal::Left, remaining[1].or(traffic.left_s), false),
        (Signal::Uturn, remaining[2].or(traffic.uturn_s), false),
        (Signal::Straight, remaining[3].or(traffic.straight_s), false),
        (Signal::Right, remaining[4].or(traffic.right_s), false),
    ];
    ordered
        .iter()
        .find(|(signal, seconds, _)| signal_active(Some(traffic), *signal, *seconds))
        .or_else(|| ordered.iter().find(|(_, seconds, _)| seconds.is_some()))
        .map_or((None, false), |(_, seconds, red)| (*seconds, *red))
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
