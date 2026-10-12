use super::{meta, uint, Data};
use crate::models::{
    NaviGuidanceInfo, NaviItemMeta, NaviLaneInfo, NaviRouteInfo, NaviStatusInfo, NaviVehicleInfo,
};

pub(super) fn cached<T: Clone>(
    item: Data<'_>,
    now: f64,
    previous: Option<&T>,
    get_meta: impl Fn(&T) -> &NaviItemMeta,
    build: impl FnOnce(NaviItemMeta) -> T,
) -> Option<T> {
    let meta = meta(item, now)?;
    Some(match previous.filter(|value| get_meta(value) == &meta) {
        Some(value) => value.clone(),
        None => build(meta),
    })
}

pub(super) fn vehicle(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviVehicleInfo>,
) -> Option<NaviVehicleInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| NaviVehicleInfo {
            meta,
            latitude: item.number("latitude", 0.0),
            longitude: item.number("longitude", 0.0),
            heading_deg: item.number("headingDeg", 0.0),
            speed_kph: item.number("speedKph", 0.0).max(0.0),
            road_name: item.field("roadName").text().unwrap_or_default(),
            virtual_gps: item.flag("virtualGps", false),
        },
    )
}

pub(super) fn guidance(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviGuidanceInfo>,
) -> Option<NaviGuidanceInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| NaviGuidanceInfo {
            meta,
            distance_m: uint(item, "distanceM"),
            time_s: uint(item, "timeSec"),
            turn_type: item.field("turnType").int().unwrap_or(-1),
            road_name: item.field("roadName").text().unwrap_or_default(),
            main_text: item.field("mainText").text().unwrap_or_default(),
            near_direction: item.field("nearDirection").text().unwrap_or_default(),
            mid_direction: item.field("midDirection").text().unwrap_or_default(),
            far_direction: item.field("farDirection").text().unwrap_or_default(),
        },
    )
}

pub(super) fn lane(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviLaneInfo>,
) -> Option<NaviLaneInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| NaviLaneInfo {
            meta,
            count: uint(item, "count").min(16),
            distance_m: uint(item, "distanceM"),
            visible: item.flag("visible", false),
            lane_play: item.flag("lanePlay", false),
            current_lane: item.field("currentLane").int().unwrap_or(-1),
            turn_code: item.field("turnCode").int().unwrap_or(-1),
            turn_info: item
                .field("turnInfo")
                .items()
                .into_iter()
                .take(16)
                .map(Data::int)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
            available: item
                .field("available")
                .items()
                .into_iter()
                .take(16)
                .map(Data::int)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
        },
    )
}

pub(super) fn route(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviRouteInfo>,
) -> Option<NaviRouteInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| NaviRouteInfo {
            meta,
            remaining_distance_m: uint(item, "remainingDistanceM"),
            remaining_time_s: uint(item, "remainingTimeSec"),
            moved_distance_m: uint(item, "movedDistanceM"),
            moved_time_s: uint(item, "movedTimeSec"),
            total_distance_m: uint(item, "totalDistanceM"),
            polyline: item
                .field("polyline")
                .items()
                .into_iter()
                .take(256)
                .filter_map(|point| {
                    Some([
                        point.field("latitude").float()?,
                        point.field("longitude").float()?,
                    ])
                })
                .collect(),
        },
    )
}

pub(super) fn status(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviStatusInfo>,
) -> Option<NaviStatusInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| NaviStatusInfo {
            meta,
            mode: item.field("mode").text().unwrap_or_default(),
            guidance_active: item.flag("guidanceActive", false),
            off_route: item.flag("offRoute", false),
            route_present: item.flag("routePresent", false),
        },
    )
}
