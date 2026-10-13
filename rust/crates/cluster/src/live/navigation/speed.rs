use super::{items::cached, uint, Data};
use crate::models::NaviSpeedInfo;

pub(super) fn speed(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviSpeedInfo>,
) -> Option<NaviSpeedInfo> {
    cached(
        item,
        now,
        previous,
        |v| &v.meta,
        |meta| {
            let sdi = item.flag("sdiPresent", false);
            let secondary = item.flag("secondarySdiPresent", false);
            let section = item.flag("sectionPresent", false);
            let road = item.field("roadLimitKph").int().unwrap_or(0);
            NaviSpeedInfo {
                meta,
                current_kph: item.number("currentKph", 0.0).max(0.0),
                road_limit_kph: (item.flag("roadLimitValid", false)
                    && (1..=200).contains(&road)
                    && road % 10 == 0)
                    .then_some(road),
                sdi_type: sdi.then(|| item.field("sdiType").int().unwrap_or(-1)),
                sdi_distance_m: sdi.then(|| uint(item, "sdiDistanceM")),
                sdi_speed_limit_kph: sdi.then(|| uint(item, "sdiSpeedLimitKph")),
                secondary_sdi_type: secondary
                    .then(|| item.field("secondarySdiType").int().unwrap_or(-1)),
                secondary_sdi_distance_m: secondary.then(|| uint(item, "secondarySdiDistanceM")),
                secondary_sdi_speed_limit_kph: secondary
                    .then(|| uint(item, "secondarySdiSpeedLimitKph")),
                section_active: item.flag("sectionActive", false),
                section_speed_limit_kph: section.then(|| uint(item, "sectionSpeedLimitKph")),
                section_average_kph: section
                    .then(|| item.number("sectionAverageKph", 0.0).max(0.0)),
                section_remaining_distance_m: section
                    .then(|| item.number("sectionRemainingDistanceM", 0.0).max(0.0)),
                section_remaining_time_s: section.then(|| uint(item, "sectionRemainingTimeSec")),
                section_progress: section
                    .then(|| item.number("sectionProgress", 0.0).clamp(0.0, 1.0)),
            }
        },
    )
}
