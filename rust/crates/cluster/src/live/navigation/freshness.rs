use crate::models::{NaviItemMeta, NaviLiveState};

fn fresh<T: Clone>(
    item: Option<&T>,
    meta: impl Fn(&T) -> Option<&NaviItemMeta>,
    ttl: f64,
    now: f64,
    next: &mut f64,
) -> Option<T> {
    let item = item?;
    let deadline = meta(item)?.received_mono_s + ttl;
    if deadline <= now {
        return None;
    }
    *next = next.min(deadline);
    Some(item.clone())
}

#[must_use]
pub fn fresh_carrot_navi(state: Option<&NaviLiveState>, now: f64) -> (Option<NaviLiveState>, f64) {
    let Some(state) = state else {
        return (None, f64::INFINITY);
    };
    let mut next = f64::INFINITY;
    let result = NaviLiveState {
        generation: state.generation,
        session_id: state.session_id.clone(),
        vehicle: fresh(
            state.vehicle.as_ref(),
            |v| Some(&v.meta),
            6.0,
            now,
            &mut next,
        ),
        current: fresh(
            state.current.as_ref(),
            |v| Some(&v.meta),
            6.0,
            now,
            &mut next,
        ),
        next: fresh(state.next.as_ref(), |v| Some(&v.meta), 6.0, now, &mut next),
        lane_current: fresh(
            state.lane_current.as_ref(),
            |v| Some(&v.meta),
            6.0,
            now,
            &mut next,
        ),
        lane_ahead: state
            .lane_ahead
            .iter()
            .filter_map(|v| fresh(Some(v), |v| Some(&v.meta), 6.0, now, &mut next))
            .collect(),
        speed: fresh(state.speed.as_ref(), |v| Some(&v.meta), 6.0, now, &mut next),
        traffic_light: fresh(
            state.traffic_light.as_ref(),
            |v| v.meta.as_ref(),
            60.0,
            now,
            &mut next,
        ),
        crossroad: fresh(
            state.crossroad.as_ref(),
            |v| Some(&v.meta),
            6.0,
            now,
            &mut next,
        ),
        route: fresh(
            state.route.as_ref(),
            |v| Some(&v.meta),
            30.0,
            now,
            &mut next,
        ),
        status: fresh(
            state.status.as_ref(),
            |v| Some(&v.meta),
            10.0,
            now,
            &mut next,
        ),
    };
    let present = result.vehicle.is_some()
        || result.current.is_some()
        || result.next.is_some()
        || result.lane_current.is_some()
        || !result.lane_ahead.is_empty()
        || result.speed.is_some()
        || result.traffic_light.is_some()
        || result.crossroad.is_some()
        || result.route.is_some()
        || result.status.is_some();
    (present.then_some(result), next)
}
