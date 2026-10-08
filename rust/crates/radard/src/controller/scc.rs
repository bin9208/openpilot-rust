use super::constants::*;
use crate::{
    lead::duplicates_primary,
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::Point,
    primary::{constants::*, stationary_geometry::vision_base_cost},
    Lead,
};
use serde::Serialize;

pub fn shadow_supported(front: &Point, points: &[Point], path: &Path) -> bool {
    let front_path = path.project(front.d_rel, front.y_rel).d_path;
    let gate = minimum(
        STATIONARY_SHADOW_CORNER_MAX_DREL_GATE_M,
        maximum(
            STATIONARY_SHADOW_CORNER_MIN_DREL_GATE_M,
            front.d_rel * STATIONARY_SHADOW_CORNER_DREL_GATE_FRACTION,
        ),
    );
    points.iter().any(|corner| {
        if !corner.measured
            || !corner.corner()
            || corner.v_lead.abs() > STATIONARY_SHADOW_CORNER_MAX_ABS_VLEAD_MPS
            || (front.d_rel - corner.d_rel).abs() > gate
            || (front.v_lead - corner.v_lead).abs() > STATIONARY_SHADOW_CORNER_MAX_VLEAD_DELTA_MPS
        {
            return false;
        }
        let d_path = path.project(corner.d_rel, corner.y_rel).d_path;
        d_path.abs() <= STATIONARY_SHADOW_CORNER_MAX_DPATH_M
            && (front_path - d_path).abs() <= STATIONARY_SHADOW_CORNER_MAX_DPATH_DELTA_M
    })
}

pub fn physical_support<'a>(
    scc: &Point,
    points: impl Iterator<Item = &'a Point>,
    path: &Path,
    require_path: bool,
) -> Option<&'a Point> {
    let gate = minimum(
        SCC_PHYSICAL_MATCH_MAX_DREL_M,
        maximum(
            SCC_PHYSICAL_MATCH_MIN_DREL_M,
            scc.d_rel * SCC_PHYSICAL_MATCH_DREL_FRACTION,
        ),
    );
    let mut best: Option<((u8, f64, f64), &Point)> = None;
    for point in points {
        if !point.measured
            || point.source == "scc"
            || !(point.source == "frontRadar" || point.corner())
            || (point.d_rel - scc.d_rel).abs() > gate
            || (point.v_lead - scc.v_lead).abs() > SCC_PHYSICAL_MATCH_MAX_VLEAD_DELTA_MPS
        {
            continue;
        }
        let projection = path.project(point.d_rel, point.y_rel);
        if require_path && projection.d_path.abs() > SCC_PHYSICAL_MATCH_MAX_ABS_DPATH_M {
            continue;
        }
        let score = (
            u8::from(point.source != "frontRadar"),
            (point.d_rel - scc.d_rel).abs(),
            (point.v_lead - scc.v_lead).abs(),
        );
        if best.as_ref().is_none_or(|(previous, _)| score < *previous) {
            best = Some((score, point));
        }
    }
    best.map(|(_, point)| point)
}

pub fn independently_supported(
    scc: &Point,
    physical: Option<&Point>,
    points: &[Point],
    path: &Path,
    vision: Option<VisionLead>,
    primary: Option<&Lead>,
) -> bool {
    let front = physical
        .filter(|point| point.source == "frontRadar")
        .or_else(|| {
            physical_support(
                scc,
                points.iter().filter(|point| point.source == "frontRadar"),
                path,
                false,
            )
        });
    let corner = physical_support(
        scc,
        points.iter().filter(|point| {
            point.corner()
                && front.is_none_or(|front| {
                    (front.d_rel - point.d_rel).abs() <= STATIONARY_VISION_CROSS_SOURCE_MAX_DREL_M
                        && (front.y_rel - point.y_rel).abs() <= SCC_CORNER_MATCH_MAX_YREL_DELTA_M
                        && (front.v_lead - point.v_lead).abs()
                            <= STATIONARY_VISION_CROSS_SOURCE_MAX_VLEAD_MPS
                })
        }),
        path,
        front.is_none(),
    );
    if corner.is_some() {
        return true;
    }
    let point = physical.unwrap_or(scc);
    if vision_base_cost(vision, point).is_none() {
        return false;
    }
    !primary.is_some_and(|primary| {
        primary.status
            && primary.radar
            && primary.model_prob >= VISION_LEAD_MIN_PROB
            && ((point.d_rel - primary.d_rel).abs() > SCC_PRIMARY_DUPLICATE_MAX_DREL_DELTA_M
                || (point.v_lead - primary.v_lead).abs()
                    > SCC_PRIMARY_DUPLICATE_MAX_VLEAD_DELTA_MPS)
    })
}

pub fn can_compete(lead: &Lead, primary: Option<&Lead>) -> bool {
    let Some(primary) = primary.filter(|lead| lead.status) else {
        return true;
    };
    if duplicates_primary(lead, Some(primary)) {
        return false;
    }
    if (lead.d_rel - primary.d_rel).abs() <= SCC_PRIMARY_DUPLICATE_MAX_DREL_DELTA_M
        && (lead.v_lead - primary.v_lead).abs() <= SCC_PRIMARY_DUPLICATE_MAX_VLEAD_DELTA_MPS
    {
        return false;
    }
    lead.d_rel + SCC_PRIMARY_CLOSER_MARGIN_M < primary.d_rel
}

#[derive(Default, Serialize)]
pub struct Tracker {
    _since_s: Option<f64>,
    _last_time_s: Option<f64>,
    _last_point: Option<Point>,
    _last_support_s: Option<f64>,
    _confirmed: bool,
}
impl Tracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn continuous(&self, time: f64, point: &Point) -> bool {
        let Some((last, previous)) = self._last_time_s.zip(self._last_point.as_ref()) else {
            return true;
        };
        let dt = time - last;
        let backwards = dt < 0.;
        let stale = dt > RADAR_MOTION_MAX_TIME_SKEW_S;
        if backwards || stale {
            return false;
        }
        (point.d_rel - (previous.d_rel + previous.v_rel * dt)).abs()
            <= SCC_LEAD_TWO_MAX_POSITION_ERROR_M
            && (point.v_lead - previous.v_lead).abs() <= SCC_LEAD_TWO_MAX_SPEED_JUMP_MPS
    }
    pub fn update(&mut self, time: f64, point: Option<&Point>, supported: bool) -> Option<Point> {
        let Some(point) = point else {
            self.reset();
            return None;
        };
        if !self.continuous(time, point) {
            self.reset();
        }
        if supported {
            self._last_support_s = Some(time);
        } else if !self._confirmed
            || self
                ._last_support_s
                .is_none_or(|last| !(0. ..=SCC_LEAD_TWO_SUPPORT_HOLD_S).contains(&(time - last)))
        {
            self.reset();
            return None;
        }
        self._since_s.get_or_insert(time);
        self._last_time_s = Some(time);
        self._last_point = Some(point.clone());
        if self
            ._since_s
            .is_none_or(|since| time - since < SCC_LEAD_TWO_CONFIRMATION_S)
        {
            return None;
        }
        self._confirmed = true;
        Some(point.clone())
    }
}
