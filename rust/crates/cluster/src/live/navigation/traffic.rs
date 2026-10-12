use super::{meta, uint, Data};
use crate::models::NaviTrafficLightInfo;

pub(super) fn traffic(
    item: Data<'_>,
    now: f64,
    previous: Option<&NaviTrafficLightInfo>,
) -> Option<NaviTrafficLightInfo> {
    let meta = meta(item, now)?;
    if !item.flag("visible", false) {
        return None;
    }
    if let Some(value) = previous.filter(|value| value.meta.as_ref() == Some(&meta)) {
        return Some(value.clone());
    }
    let remaining = |valid, name| item.flag(valid, false).then(|| uint(item, name));
    let on = |valid, name| item.flag(valid, false).then(|| item.flag(name, false));
    Some(NaviTrafficLightInfo {
        distance_m: Some(uint(item, "distanceM")),
        red_s: remaining("redValid", "redRemainSec"),
        straight_s: remaining("greenValid", "greenRemainSec"),
        left_s: remaining("leftValid", "leftRemainSec"),
        right_s: remaining("rightValid", "rightRemainSec"),
        uturn_s: remaining("uturnValid", "uturnRemainSec"),
        red_on: on("redValid", "redOn"),
        straight_on: on("greenValid", "greenOn"),
        left_on: on("leftValid", "leftOn"),
        right_on: on("rightValid", "rightOn"),
        uturn_on: on("uturnValid", "uturnOn"),
        meta: Some(meta),
    })
}
