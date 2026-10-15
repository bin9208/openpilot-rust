use super::{Matcher, VisionMatch};
use crate::{model::VisionLead, path::Path, point::Point};
use indexmap::IndexMap;
impl Matcher {
    pub(super) fn match_closing_range(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
        time: Option<f64>,
        yaw: f64,
    ) -> Option<VisionMatch> {
        self.closing_range_match = None;
        let mut history = IndexMap::new();
        let mut matches = Vec::new();
        if let Some((v, t)) = vision.zip(time).filter(|(v, t)| {
            t.is_finite()
                && yaw.is_finite()
                && yaw.abs() < 0.05
                && [
                    v.d_rel,
                    v.y_rel,
                    v.velocity,
                    v.acceleration,
                    v.probability,
                    v.x_std,
                ]
                .iter()
                .all(|v| v.is_finite())
                && v.probability >= 0.6
                && v.acceleration >= -0.5
        }) {
            let visual_path = path.project(v.d_rel, v.y_rel).d_path;
            for p in points {
                if ![p.d_rel, p.y_rel, p.v_rel, p.v_lead, p.yv_rel]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    continue;
                }
                let delta = p.d_rel - v.d_rel;
                let limit = 35_f64
                    .min(0.4 * v.d_rel)
                    .min(15_f64.max(2.5 * v.x_std.abs()));
                let offset = path.project(p.d_rel, p.y_rel).d_path;
                if p.source != "frontRadar"
                    || !p.measured
                    || p.radar_track_state < 2
                    || !(40.0..=150.).contains(&p.d_rel)
                    || p.v_rel >= -1.
                    || !(p.v_lead > 4. && p.v_lead < v.velocity - 2.)
                    || v.velocity - p.v_lead > 20.
                    || !(0.0..=limit).contains(&delta)
                    || offset.abs() > 1.5
                    || visual_path.abs() > 1.
                    || (p.y_rel - v.y_rel).abs() > 1.25
                    || p.d_rel + p.v_lead.powi(2) / 5. + 5. > v.d_rel + v.velocity.powi(2) / 5.
                {
                    continue;
                }
                let key = p.identity();
                let since = self
                    ._closing_range_history
                    .get(&key)
                    .filter(|(_, last, old)| {
                        let dt = t - last;
                        dt > 0.
                            && dt <= 0.15
                            && (p.d_rel - old.d_rel - old.v_rel * dt).abs() <= 1.
                            && (p.y_rel - old.y_rel - old.yv_rel * dt).abs() <= 0.5
                            && (p.v_lead - old.v_lead).abs() <= 2.
                    })
                    .map_or(t, |(start, _, _)| *start);
                history.insert(key, (since, t, p.clone()));
                if t - since >= 0.25 {
                    matches.push(VisionMatch {
                        point: p.clone(),
                        probability: v.probability,
                        score: 1. - delta / limit.max(1.),
                        d_path: offset,
                    });
                }
            }
        }
        self._closing_range_history = history;
        self.closing_range_match = matches
            .into_iter()
            .min_by(|a, b| a.point.d_rel.total_cmp(&b.point.d_rel));
        self.closing_range_match.clone()
    }
}
