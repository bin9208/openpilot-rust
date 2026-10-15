use super::{constants::*, Matcher};
use crate::point::Point;
impl Matcher {
    pub(super) fn update_corner_motion_history(&mut self, points: &[Point], time: Option<f64>) {
        let Some(t) = time.filter(|t| t.is_finite()) else {
            self._corner_motion_history.clear();
            self._corner_motion_unreliable_until.clear();
            return;
        };
        self._corner_motion_unreliable_until
            .retain(|_, until| t < *until && *until <= t + 1.);
        self._corner_motion_history.retain(|_, h| {
            h.back().is_some_and(|(last, _)| {
                (0.0..=VISION_CORROBORATED_MAX_OBSERVATION_GAP_S).contains(&(t - last))
            })
        });
        for p in points {
            if !(p.corner() && p.measured && p.d_rel > 0.5 && p.d_rel < 180. && p.v_rel.is_finite())
            {
                continue;
            }
            let key = p.identity();
            let history = self._corner_motion_history.entry(key.clone()).or_default();
            if let Some((last, old)) = history.back() {
                let dt = t - last;
                if dt <= 0.
                    || (p.d_rel - (old.d_rel + old.v_rel * dt)).abs()
                        > STATIONARY_LONGITUDINAL_CONTINUITY_M
                {
                    history.clear();
                    self._corner_motion_unreliable_until.insert(key, t + 1.);
                }
            }
            history.push_back((t, p.clone()));
            while history.len() > 2 && t - history[1].0 >= 1. {
                history.pop_front();
            }
        }
    }
    pub(super) fn corner_motion_consistent(&mut self, p: &Point, t: f64) -> bool {
        let key = p.identity();
        let Some(h) = self
            ._corner_motion_history
            .get(&key)
            .filter(|h| h.back().is_some_and(|(last, _)| *last == t))
        else {
            return false;
        };
        let Some((start, first)) = h.front() else {
            return false;
        };
        let duration = t - start;
        if duration < 0.5 {
            return false;
        }
        let mut displacement = 0.;
        let (mut previous_time, mut previous) = (*start, first);
        for (sample_time, sample) in h {
            displacement += 0.5 * (previous.v_rel + sample.v_rel) * (sample_time - previous_time);
            previous_time = *sample_time;
            previous = sample;
        }
        if (p.d_rel - first.d_rel - displacement).abs() / duration
            > RADAR_ONLY_MOVING_CORNER_MAX_LONGITUDINAL_ERROR_RATE_MPS
        {
            self._corner_motion_unreliable_until.insert(key, t + 1.);
            return false;
        }
        t >= self
            ._corner_motion_unreliable_until
            .get(&key)
            .copied()
            .unwrap_or(f64::NEG_INFINITY)
    }
}
