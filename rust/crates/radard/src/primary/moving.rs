use super::{constants::*, Matcher, VisionMatch};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Path,
    point::Point,
};

fn laplacian(value: f64, mean: f64, scale: f64) -> f64 {
    let scale = maximum(scale.abs(), 0.1);
    (-(value - mean).abs() / scale).exp() / (2. * scale)
}

impl Matcher {
    pub fn match_moving(
        &mut self,
        vision: Option<VisionLead>,
        points: &[Point],
        path: &Path,
    ) -> Option<VisionMatch> {
        let Some(vision) = vision else {
            self.reset_moving();
            return None;
        };
        let high = vision.probability >= VISION_LEAD_MIN_PROB;
        let holding = !high
            && vision.probability > VISION_LEAD_HOLD_MIN_PROB
            && self.last_identity.is_some()
            && self.low_probability_hold_frames < VISION_LEAD_HOLD_MAX_FRAMES;
        if !high && !holding {
            self.reset_moving();
            return None;
        }
        let velocity_tolerance = maximum(
            5.,
            vision.velocity.abs()
                * (0.3 + 0.2 * minimum(maximum((vision.probability - 0.8) / 0.18, 0.), 1.)),
        );
        let distance_tolerance = maximum(
            maximum(5., vision.d_rel * 0.25),
            minimum(
                VISION_MATCH_XSTD_MAX_M,
                vision.x_std.abs() * VISION_MATCH_XSTD_SIGMA,
            ),
        );
        let mut candidates = Vec::new();
        for point in points {
            if point.source != "frontRadar" && point.source != "scc" {
                continue;
            }
            let held = self.last_identity.as_ref() == Some(&point.identity());
            if holding && !held {
                continue;
            }
            if !(0.5 < point.d_rel && point.d_rel < 180.) {
                continue;
            }
            let score = laplacian(point.d_rel, vision.d_rel, vision.x_std)
                * laplacian(point.y_rel, vision.y_rel, vision.y_std)
                * laplacian(point.v_lead, vision.velocity, vision.v_std);
            let velocity_error = (point.v_lead - vision.velocity).abs();
            if (!held && score < VISION_MATCH_FRESH_MIN_SCORE)
                || (point.d_rel - vision.d_rel).abs() > VISION_RADAR_MAX_DISTANCE_ERROR_M
                || (point.d_rel - vision.d_rel).abs()
                    >= distance_tolerance
                        + if held {
                            VISION_MATCH_DISTANCE_HYSTERESIS_M
                        } else {
                            0.
                        }
                || (point.y_rel - vision.y_rel).abs() >= 2.
                || !(velocity_error < velocity_tolerance
                    || (point.v_lead > 3.
                        && velocity_error < maximum(velocity_tolerance * 3., 20.)))
            {
                continue;
            }
            let projection = path.project(point.d_rel, point.y_rel);
            if projection.d_path.abs()
                > if held {
                    VISION_MATCH_HELD_MAX_DPATH_M
                } else {
                    VISION_MATCH_FRESH_MAX_DPATH_M
                }
            {
                continue;
            }
            candidates.push(VisionMatch {
                point: point.clone(),
                probability: vision.probability,
                score,
                d_path: projection.d_path,
            });
        }
        if candidates.is_empty() {
            self.reset_moving();
            return None;
        }
        if candidates
            .iter()
            .any(|candidate| candidate.point.source == "frontRadar")
        {
            candidates.retain(|candidate| candidate.point.source == "frontRadar");
        }
        let mut selected = candidates.remove(0);
        for candidate in candidates {
            let candidate_key = (
                candidate.score,
                -(candidate.point.d_rel - vision.d_rel).abs(),
            );
            let selected_key = (selected.score, -(selected.point.d_rel - vision.d_rel).abs());
            if candidate_key > selected_key {
                selected = candidate;
            }
        }
        self.last_identity = Some(selected.point.identity());
        self.low_probability_hold_frames = if holding {
            self.low_probability_hold_frames + 1
        } else {
            0
        };
        Some(selected)
    }
}
