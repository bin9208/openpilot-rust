//! Navigation freshness and presentation policy from `cluster_navi.py` (MIT).
mod freshness;
mod items;
mod speed;
mod traffic;
use super::projection::Data;
use crate::models::{NaviCrossroadInfo, NaviItemMeta, NaviLiveState};
use capnp::dynamic_value::Reader;
pub use freshness::fresh_carrot_navi;

#[must_use]
pub fn parse_carrot_navi(
    data: Reader<'_>,
    now: f64,
    previous: Option<&NaviLiveState>,
) -> Option<NaviLiveState> {
    let data = Data(data);
    if data.field("schemaVersion").int() != Some(1) || !data.flag("connected", false) {
        return None;
    }
    let session_id = data.field("sessionId").text().unwrap_or_default();
    let previous = previous.filter(|state| state.session_id == session_id);
    let crossroad = data.field("crossroad");
    let state = NaviLiveState {
        generation: data.field("generation").uint().unwrap_or(0),
        session_id,
        vehicle: items::vehicle(
            data.field("vehicle"),
            now,
            previous.and_then(|v| v.vehicle.as_ref()),
        ),
        current: items::guidance(
            data.field("guidanceCurrent"),
            now,
            previous.and_then(|v| v.current.as_ref()),
        ),
        next: items::guidance(
            data.field("guidanceNext"),
            now,
            previous.and_then(|v| v.next.as_ref()),
        ),
        lane_current: items::lane(
            data.field("laneCurrent"),
            now,
            previous.and_then(|v| v.lane_current.as_ref()),
        ),
        lane_ahead: data
            .field("laneAhead")
            .items()
            .into_iter()
            .enumerate()
            .filter_map(|(index, item)| {
                items::lane(item, now, previous.and_then(|v| v.lane_ahead.get(index)))
            })
            .collect(),
        speed: speed::speed(
            data.field("speed"),
            now,
            previous.and_then(|v| v.speed.as_ref()),
        ),
        traffic_light: traffic::traffic(
            data.field("trafficSignal"),
            now,
            previous.and_then(|v| v.traffic_light.as_ref()),
        ),
        crossroad: items::cached(
            crossroad,
            now,
            previous.and_then(|v| v.crossroad.as_ref()),
            |v| &v.meta,
            |meta| NaviCrossroadInfo {
                meta,
                visible: crossroad.flag("visible", false),
                distance_m: uint(crossroad, "distanceM"),
                image_code: uint(crossroad, "imageCode"),
            },
        ),
        route: items::route(
            data.field("route"),
            now,
            previous.and_then(|v| v.route.as_ref()),
        ),
        status: items::status(
            data.field("navigationStatus"),
            now,
            previous.and_then(|v| v.status.as_ref()),
        ),
    };
    fresh_carrot_navi(Some(&state), now).0
}

fn uint(item: Data<'_>, name: &str) -> i64 {
    item.field(name).int().unwrap_or(0).max(0)
}

fn meta(item: Data<'_>, now: f64) -> Option<NaviItemMeta> {
    let meta = item.field("meta");
    if !meta.flag("present", false) {
        return None;
    }
    let received = meta.field("receivedMonoTimeNanos").float().unwrap_or(0.0) / 1e9;
    Some(NaviItemMeta {
        sequence: meta.field("sequence").uint().unwrap_or(0),
        source_timestamp_ms: meta.field("sourceTimestampMillis").uint().unwrap_or(0),
        received_mono_s: if received <= 0.0 || received > now + 60.0 {
            now
        } else {
            received
        },
    })
}

#[must_use]
pub fn resolve_navi_speed_limit(
    base: Option<i64>,
    source: Option<&str>,
    navi: Option<&NaviLiveState>,
) -> (Option<i64>, Option<String>) {
    if source == Some("v") {
        return (base, source.map(str::to_owned));
    }
    let Some(speed) = navi.and_then(|state| state.speed.as_ref()) else {
        return (base, source.map(str::to_owned));
    };
    match speed.road_limit_kph.filter(|limit| *limit > 0) {
        Some(limit) => (Some(limit), Some("n".into())),
        None => (None, None),
    }
}
