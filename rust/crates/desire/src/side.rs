use crate::types::{maximum, minimum, Lead};
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Default, Serialize)]
pub struct ExistCounter {
    pub counter: i64,
    pub true_count: u64,
    pub false_count: u64,
}

impl ExistCounter {
    pub fn update(&mut self, exists: bool) {
        if exists {
            self.true_count = self.true_count.saturating_add(1);
            self.false_count = 0;
            if self.true_count >= 4 {
                self.counter = self.counter.saturating_add(1).max(1);
            }
        } else {
            self.false_count = self.false_count.saturating_add(1);
            self.true_count = 0;
            if self.false_count >= 4 {
                self.counter = self.counter.saturating_sub(1).min(-1);
            }
        }
    }
}

#[derive(Serialize)]
pub struct SideState {
    pub name: String,
    pub lane_width: f64,
    pub lane_width_diff: f64,
    pub dist_to_edge: f64,
    pub dist_to_edge_far: f64,
    pub cur_prob: f64,
    pub current_lane_missing: bool,
    pub lane_exist_count: ExistCounter,
    pub lane_width_count: ExistCounter,
    pub edge_count: ExistCounter,
    pub lane_available: bool,
    pub edge_available: bool,
    pub lane_width_queue: VecDeque<f64>,
    pub lane_line_info_raw: i32,
    pub lane_line_info_mod: i32,
    pub last_lane_line_mod: i32,
    pub lane_line_info_edge_detect: bool,
    pub lane_available_last: bool,
    pub edge_available_last: bool,
    pub lane_available_trigger: bool,
    pub lane_appeared: bool,
    pub object_detected_count: i64,
    pub side_object_detected: bool,
    pub bsd_hold_counter: u32,
    pub bsd_detected_now: bool,
    pub bsd_receding_track_id: i64,
    pub bsd_receding_frames: u64,
    pub bsd_receding_start_d_rel: f64,
    pub bsd_receding_last_d_rel: f64,
    pub lane_change_available_geom: bool,
    pub lane_change_available: bool,
    pub lane_change_available_last: bool,
    pub lane_change_available_released: bool,
    pub lane_width_sum: f64,
}

impl SideState {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            lane_width: 0.0,
            lane_width_diff: 0.0,
            dist_to_edge: 0.0,
            dist_to_edge_far: 0.0,
            cur_prob: 1.0,
            current_lane_missing: false,
            lane_exist_count: ExistCounter::default(),
            lane_width_count: ExistCounter::default(),
            edge_count: ExistCounter::default(),
            lane_available: false,
            edge_available: false,
            lane_width_queue: VecDeque::with_capacity(20),
            lane_line_info_raw: 0,
            lane_line_info_mod: 0,
            last_lane_line_mod: 0,
            lane_line_info_edge_detect: false,
            lane_available_last: false,
            edge_available_last: false,
            lane_available_trigger: false,
            lane_appeared: false,
            object_detected_count: 0,
            side_object_detected: false,
            bsd_hold_counter: 0,
            bsd_detected_now: false,
            bsd_receding_track_id: -1,
            bsd_receding_frames: 0,
            bsd_receding_start_d_rel: 0.0,
            bsd_receding_last_d_rel: 0.0,
            lane_change_available_geom: false,
            lane_change_available: false,
            lane_change_available_last: false,
            lane_change_available_released: false,
            lane_width_sum: 0.0,
        }
    }

    pub(crate) fn update_geometry(
        &mut self,
        outer: &[f64],
        outer_probability: f64,
        current: &[f64],
        edge: &[f64],
        current_probability: f64,
    ) {
        let current_y = interpolate(1.0, current);
        let width = (current_y - interpolate(1.0, outer)).abs();
        let distance = (current_y - interpolate(1.0, edge)).abs();
        let far = (current_y - interpolate(2.0, edge)).abs();
        let width = minimum(width, distance);
        self.lane_exist_count.update(outer_probability > 0.5);
        if self.lane_width_queue.len() == 20 {
            self.lane_width_sum -= self.lane_width_queue.pop_front().unwrap();
        }
        self.lane_width_queue.push_back(width);
        self.lane_width_sum += width;
        self.lane_width = self.lane_width_sum / self.lane_width_queue.len() as f64;
        self.lane_width_diff = if self.lane_width_queue.len() >= 2 {
            self.lane_width_queue.back().unwrap() - self.lane_width_queue.front().unwrap()
        } else {
            0.0
        };
        self.dist_to_edge = distance;
        self.dist_to_edge_far = far;
        self.lane_width_count.update(self.lane_width > 2.5);
        self.edge_count.update(distance > 2.5);
        self.lane_available = self.lane_width_count.counter > 4;
        self.edge_available = self.edge_count.counter > 4 && far > 2.5;
        self.cur_prob = current_probability;
        self.current_lane_missing = current_probability < 0.3;
    }

    pub fn update_line(&mut self, value: i32) {
        self.lane_line_info_raw = value;
        let kind = value.rem_euclid(10);
        self.lane_line_info_edge_detect =
            matches!(kind, 0 | 5) && !matches!(self.last_lane_line_mod, 0 | 5);
        self.last_lane_line_mod = kind;
        self.lane_line_info_mod = kind;
    }

    pub fn unsafe_lead(v_ego: f64, lead: &Lead) -> bool {
        if !(lead.status && lead.d_rel > 0.1 && lead.d_rel < 160.0) {
            return false;
        }
        let v_rel = lead.v_relative(v_ego);
        let v_ego = maximum(0.0, v_ego);
        if lead.d_rel <= 5.0 {
            return true;
        }
        let min_gap = (v_ego * 0.3).clamp(6.0, 12.0);
        let closing = maximum(0.0, -v_rel);
        if closing > 0.5 {
            let limit = if v_ego <= 15.0 {
                (1.0 / 15.0) * v_ego + 2.0
            } else if v_ego < 30.0 {
                (0.5 / 15.0) * (v_ego - 15.0) + 3.0
            } else {
                3.5
            };
            if lead.d_rel < maximum(min_gap + 6.0, v_ego * 1.2) && lead.d_rel / closing < limit {
                return true;
            }
            if lead.d_rel + v_rel * 1.5 < min_gap {
                return true;
            }
        }
        lead.d_rel < min_gap && v_rel < 1.0
    }

    pub fn update_obstacles(
        &mut self,
        v_ego: f64,
        primary: &Lead,
        blindspot: bool,
        ignore_bsd: bool,
        objects: &[Lead],
    ) {
        let primary_unsafe = Self::unsafe_lead(v_ego, primary);
        let object_unsafe = primary_unsafe
            || objects
                .iter()
                .filter(|object| object.corner())
                .any(|object| Self::unsafe_lead(v_ego, object));
        self.object_detected_count = if object_unsafe {
            self.object_detected_count.saturating_add(1).max(1)
        } else {
            self.object_detected_count.saturating_sub(1).min(-1)
        };
        // Python int(-0.3 / 0.05) is -5 because the quotient rounds above -6.
        self.side_object_detected = self.object_detected_count > -5;
        self.bsd_detected_now = blindspot;
        if blindspot && !ignore_bsd {
            self.bsd_hold_counter = 40;
            self.reset_receding();
        } else if !ignore_bsd {
            self.bsd_hold_counter = self.bsd_hold_counter.saturating_sub(1);
            if self.bsd_hold_counter > 0 && self.receding_ready(v_ego, objects) {
                self.bsd_hold_counter = 0;
                if !primary_unsafe {
                    self.object_detected_count = -5;
                    self.side_object_detected = false;
                }
            }
        } else {
            self.bsd_hold_counter = 0;
            self.reset_receding();
        }
    }

    fn reset_receding(&mut self) {
        self.bsd_receding_track_id = -1;
        self.bsd_receding_frames = 0;
        self.bsd_receding_start_d_rel = 0.0;
        self.bsd_receding_last_d_rel = 0.0;
    }

    fn receding_ready(&mut self, v_ego: f64, objects: &[Lead]) -> bool {
        let selected = objects
            .iter()
            .filter(|object| {
                object.corner()
                    && object.radar_track_id >= 0
                    && object.v_relative(v_ego) >= 3.0
                    && object.d_rel > 0.1
                    && object.d_rel < 160.0
                    && (object.radar_track_id == self.bsd_receding_track_id || object.d_rel <= 6.0)
            })
            .min_by(|a, b| {
                (a.radar_track_id != self.bsd_receding_track_id)
                    .cmp(&(b.radar_track_id != self.bsd_receding_track_id))
                    .then_with(|| a.d_rel.partial_cmp(&b.d_rel).unwrap())
                    .then_with(|| a.radar_track_id.cmp(&b.radar_track_id))
            });
        let Some(selected) = selected else {
            self.reset_receding();
            return false;
        };
        let unsafe_other = objects
            .iter()
            .filter(|object| object.corner())
            .any(|object| {
                object.radar_track_id != selected.radar_track_id && Self::unsafe_lead(v_ego, object)
            });
        let continuous = selected.radar_track_id == self.bsd_receding_track_id
            && selected.d_rel >= self.bsd_receding_last_d_rel - 0.25;
        if unsafe_other || !continuous {
            self.bsd_receding_track_id = selected.radar_track_id;
            self.bsd_receding_frames = 1;
            self.bsd_receding_start_d_rel = selected.d_rel;
        } else {
            self.bsd_receding_frames = self.bsd_receding_frames.saturating_add(1);
        }
        self.bsd_receding_last_d_rel = selected.d_rel;
        !unsafe_other
            && !Self::unsafe_lead(v_ego, selected)
            && self.bsd_receding_frames >= 6
            && selected.d_rel >= 5.0
            && selected.d_rel - self.bsd_receding_start_d_rel >= 1.0
    }

    pub fn compute_available(&mut self, line_allowed: bool, ignore_bsd: bool) {
        self.lane_change_available_geom =
            (self.lane_available || self.edge_available) && line_allowed;
        let blindspot = self.bsd_hold_counter > 0 && !ignore_bsd;
        self.lane_change_available =
            self.lane_change_available_geom && !self.side_object_detected && !blindspot;
        self.lane_change_available_released =
            self.lane_change_available && !self.lane_change_available_last;
    }

    pub fn update_triggers(&mut self) {
        self.lane_available_trigger =
            self.lane_width_diff > 0.8 && self.lane_width < self.dist_to_edge;
        self.lane_appeared =
            (self.lane_appeared || self.lane_exist_count.counter >= 4) && self.dist_to_edge < 4.0;
    }

    pub fn commit_last(&mut self) {
        self.lane_available_last = self.lane_available;
        self.edge_available_last = self.edge_available;
        self.lane_change_available_last = self.lane_change_available;
    }
}

fn interpolate(time: f64, values: &[f64]) -> f64 {
    let index = (1..33)
        .find(|&index| 10.0 * (f64::from(index) / 32.0).powi(2) >= time)
        .unwrap();
    let left = 10.0 * (f64::from(index - 1) / 32.0).powi(2);
    let right = 10.0 * (f64::from(index) / 32.0).powi(2);
    let index = index as usize;
    if time == right {
        return values[index];
    }
    let slope = (values[index] - values[index - 1]) / (right - left);
    let mut value = slope * (time - left) + values[index - 1];
    if value.is_nan() {
        value = slope * (time - right) + values[index];
        if value.is_nan() && values[index - 1] == values[index] {
            value = values[index - 1];
        }
    }
    value
}
