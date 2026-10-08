#[path = "track_state.rs"]
mod state;
pub use state::Track;

use crate::{
    lead_filter::LeadFilter,
    numerics::Numerics,
    point::{Point, Source},
    scalar::{clip, divide, float_sum, maximum, square, FirstOrder},
    Error,
};
use std::collections::VecDeque;

impl Track {
    pub fn new(track_id: u64, point: &Point, dt: f64) -> Result<Self, Error> {
        Self::new_for_period(track_id, point, dt, false)
    }

    pub fn new_for_period(
        track_id: u64,
        point: &Point,
        dt: f64,
        numpy_period: bool,
    ) -> Result<Self, Error> {
        let v_lead = f64::from(point.v_lead);
        let v_lead_avg = FirstOrder::new_for_period(v_lead, 0.1, dt, numpy_period)?;
        let a_lead_avg = FirstOrder::new_for_period(0., 0.05, dt, numpy_period)?;
        let lead_filter = LeadFilter::new(v_lead, dt)?;
        let j_lead_avg = FirstOrder::new_for_period(0., 0.25, dt, numpy_period)?;
        let samples = (0.5 / dt).round_ties_even();
        if !samples.is_finite() {
            return Err(Error::InfiniteInteger);
        }
        if samples >= isize::MAX as f64 {
            return Err(Error::IntegerOverflow);
        }
        let jerk_history_samples = (samples as usize + 1).max(7);
        let mut state = Self {
            track_id,
            reused_corner_slot: (200..220).contains(&track_id) || (240..250).contains(&track_id),
            radar_source: point.radar_source,
            cnt: 0,
            d_rel: f64::from(point.d_rel),
            v_rel: f64::from(point.v_rel),
            y_rel: f64::from(point.y_rel),
            yv_rel: f64::from(point.yv_rel),
            v_lead,
            a_lead: 0.,
            j_lead: 0.,
            noisy: false,
            dt,
            v_lead_avg,
            a_lead_avg,
            j_lead_avg,
            lead_filter,
            y_rel_avg: FirstOrder::new_for_period(f64::from(point.y_rel), 0.1, dt, numpy_period)?,
            yv_rel_avg: FirstOrder::new_for_period(f64::from(point.yv_rel), 0.1, dt, numpy_period)?,
            a_lead_v_history: VecDeque::new(),
            j_lead_v_history: VecDeque::new(),
            jerk_history_samples,
            numpy_period,
        };
        state.reset_kinematics();
        Ok(state)
    }

    fn reset_kinematics(&mut self) {
        self.a_lead = 0.;
        self.j_lead = 0.;
        self.noisy = false;
        self.v_lead_avg.x = self.v_lead;
        self.lead_filter.reset(self.v_lead);
        self.a_lead_avg.x = self.a_lead;
        self.j_lead_avg.x = self.j_lead;
        self.a_lead_v_history.clear();
        self.a_lead_v_history.push_back(self.v_lead);
        self.j_lead_v_history.clear();
        self.j_lead_v_history.push_back(self.v_lead);
    }

    fn init_point(&mut self, point: &Point) {
        self.radar_source = point.radar_source;
        self.d_rel = f64::from(point.d_rel);
        self.v_rel = f64::from(point.v_rel);
        self.y_rel = f64::from(point.y_rel);
        self.yv_rel = f64::from(point.yv_rel);
        self.v_lead = f64::from(point.v_lead);
        self.reset_kinematics();
        self.y_rel_avg.x = self.y_rel;
        self.yv_rel_avg.x = self.yv_rel;
    }

    pub fn discontinuous(&self, point: &Point) -> bool {
        self.reused_corner_slot
            && ((f64::from(point.d_rel) - self.d_rel).abs() > 8.
                || (f64::from(point.y_rel) - self.y_rel).abs() > 1.5
                || (f64::from(point.v_rel) - self.v_rel).abs() > 4.)
    }

    pub fn write_acceleration(&self, point: &mut Point) {
        point.a_lead = if self.cnt >= 6 {
            self.a_lead as f32
        } else {
            0.
        };
        point.j_lead = if self.cnt >= 6 {
            self.j_lead as f32
        } else {
            0.
        };
    }

    pub fn update(
        &mut self,
        point: &Point,
        _a_ego: f64,
        numerics: &mut Numerics,
    ) -> Result<(), Error> {
        if !point.measured {
            if self.cnt > 0 {
                self.init_point(point);
            }
            self.cnt = 0;
        } else if self.cnt < 1 || self.discontinuous(point) {
            self.init_point(point);
            self.cnt += 1;
        } else {
            self.v_lead = f64::from(point.v_lead);
            if self.reused_corner_slot {
                self.y_rel = f64::from(point.y_rel);
                self.yv_rel = f64::from(point.yv_rel);
                self.y_rel_avg.x = self.y_rel;
                self.yv_rel_avg.x = self.yv_rel;
            } else {
                self.y_rel = self.y_rel_avg.update(f64::from(point.y_rel));
                self.yv_rel = self.yv_rel_avg.update(f64::from(point.yv_rel));
            }
            let filtered = self.v_lead_avg.update(self.v_lead);
            let pseudo_stop = filtered.abs() < 0.3 && (self.v_lead - filtered).abs() < 0.05;
            if self.radar_source == Source::Scc {
                push_bounded(&mut self.a_lead_v_history, self.v_lead, 3);
                let a_raw = if self.a_lead_v_history.len() == 3 {
                    (self.a_lead_v_history[2] - self.a_lead_v_history[0]) / (2. * self.dt)
                } else {
                    0.
                };
                self.noisy = (a_raw - self.a_lead).abs() > 3.;
                if self.noisy {
                    self.cnt = 0;
                }
                let sample = if pseudo_stop {
                    0.
                } else {
                    clip(a_raw, -10., 5.)
                };
                self.a_lead = self.a_lead_avg.update(sample);
            } else {
                self.a_lead = self.lead_filter.update(self.v_lead, pseudo_stop)?;
                self.noisy = self.lead_filter.limited;
            }
            let mut velocity = self.v_lead;
            if let Some(previous) = self.j_lead_v_history.back().copied() {
                velocity = previous + clip((velocity - previous) / self.dt, -10., 5.) * self.dt;
            }
            push_bounded(
                &mut self.j_lead_v_history,
                velocity,
                self.jerk_history_samples,
            );
            let estimate = if self.j_lead_v_history.len() < 7 {
                0.
            } else {
                let weights = numerics.jerk_weights(self.j_lead_v_history.len())?;
                let values = weights
                    .iter()
                    .zip(&self.j_lead_v_history)
                    .map(|(w, v)| w * v);
                let total = if self.numpy_period {
                    values.fold(0., |sum, value| sum + value)
                } else {
                    float_sum(values)
                };
                divide(total, square(self.dt)?)?
            };
            let measured = clip(estimate, -6., 6.);
            let mut target = if pseudo_stop || (self.noisy && self.radar_source == Source::Scc) {
                0.
            } else {
                maximum(0., measured.abs() - 0.2).copysign(measured)
            };
            if self.cnt <= 2 {
                target = 0.;
            }
            self.j_lead_avg
                .update_alpha(if target == 0. { 0.1 } else { 0.25 })?;
            self.j_lead = clip(self.j_lead_avg.update(target), -5., 5.);
            self.j_lead_avg.x = self.j_lead;
            self.d_rel = f64::from(point.d_rel);
            self.v_rel = f64::from(point.v_rel);
            self.cnt += 1;
        }
        Ok(())
    }
}

fn push_bounded(values: &mut VecDeque<f64>, value: f64, length: usize) {
    if values.len() == length {
        values.pop_front();
    }
    values.push_back(value);
}
