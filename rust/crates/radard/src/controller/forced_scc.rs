use super::{Controller, constants::*, scc};
use crate::{
    model::{Model, VisionLead},
    path::Path,
    point::Point,
    predictor::CutOutPredictor,
};
fn evidence(
    scc: &Point,
    points: &[Point],
    path: &Path,
    vision: Option<VisionLead>,
    model: &Model,
) -> (bool, bool) {
    if matches!(
        model.lane_change_state.as_str(),
        "preLaneChange" | "laneChangeStarting" | "laneChangeFinishing"
    ) && scc.v_lead < 5.
    {
        return (false, true);
    }
    if vision.is_some_and(|v| {
        v.probability >= 0.5
            && (v.d_rel - scc.d_rel).abs()
                <= SCC_PHYSICAL_MATCH_MIN_DREL_M.max(scc.d_rel * SCC_PHYSICAL_MATCH_DREL_FRACTION)
            && v.y_rel.abs() <= 2.
    }) {
        return (false, true);
    }
    let supports: Vec<_> = [
        scc::physical_support(
            scc,
            points.iter().filter(|p| p.source == "frontRadar"),
            path,
            false,
        ),
        scc::physical_support(scc, points.iter().filter(|p| p.corner()), path, false),
    ]
    .into_iter()
    .flatten()
    .collect();
    if supports.is_empty() {
        return (false, false);
    }
    let offsets: Vec<_> = supports
        .iter()
        .map(|p| path.project(p.d_rel, p.y_rel).d_path.abs())
        .collect();
    if offsets.iter().copied().fold(f64::INFINITY, f64::min) < 2.
        || supports.iter().any(|p| p.y_rel.abs() < 2.)
    {
        return (false, true);
    }
    let agree = supports.len() < 2
        || (supports[0].y_rel - supports[1].y_rel).abs() <= SCC_CORNER_MATCH_MAX_YREL_DELTA_M;
    let beside = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max) >= 2.5
        && (agree || offsets.iter().all(|v| *v >= 2.5));
    (beside, false)
}
impl<P: CutOutPredictor> Controller<P> {
    pub(super) fn reject_forced_scc(
        &mut self,
        scc: &Point,
        points: &[Point],
        path: &Path,
        vision: Option<VisionLead>,
        model: &Model,
        time: f64,
    ) -> bool {
        let continuous = self._forced_scc_previous.as_ref().is_some_and(|(t, p)| {
            let dt = time - t;
            (0.0..=1.).contains(&dt)
                && scc.identity() == p.identity()
                && (scc.d_rel - (p.d_rel + p.v_rel * dt)).abs() <= 2.
                && (scc.v_rel - p.v_rel).abs() <= 3.
        });
        if !continuous {
            self.forced_scc_rejected_s = None;
        }
        self._forced_scc_previous = Some((time, scc.clone()));
        let (beside, in_lane) = evidence(scc, points, path, vision, model);
        if in_lane {
            self.forced_scc_rejected_s = None;
            return false;
        }
        if beside {
            self.forced_scc_rejected_s = Some(time);
            return true;
        }
        self.forced_scc_rejected_s
            .is_some_and(|t| (0.0..=1.).contains(&(time - t)))
    }
}
