use crate::{data::Data, numerics::Numerics, point::Point, track::Track, Error};
use indexmap::IndexMap;
use std::collections::VecDeque;

pub struct Base {
    pub pts: IndexMap<u64, Point>,
    pub tracks: IndexMap<u64, Track>,
    pub frame: u64,
    pub v_ego_hist: VecDeque<f64>,
    pub a_ego_hist: VecDeque<f64>,
    pub v_ego: f64,
    pub a_ego: f64,
    pub last_timestamp: Option<f64>,
    pub dt: Option<f64>,
    pub init_samples: Vec<f64>,
    pub init_done: bool,
    pub history_maxlen: usize,
    configured_period: f32,
    numpy_period: bool,
}

impl Base {
    pub fn new(delay: f32, period: f32) -> Result<Self, Error> {
        let samples = (f64::from(delay) / 0.01).round_ties_even();
        if samples.is_nan() {
            return Err(Error::NanInteger);
        }
        if !samples.is_finite() {
            return Err(Error::InfiniteInteger);
        }
        if samples < isize::MIN as f64 || samples >= isize::MAX as f64 {
            return Err(Error::IntegerOverflow);
        }
        if samples + 1. < 0. {
            return Err(Error::NegativeHistory);
        }
        let history_maxlen = (samples as i64 + 1) as usize;
        let mut history = VecDeque::new();
        if history_maxlen != 0 {
            history.push_back(0.);
        }
        Ok(Self {
            pts: IndexMap::new(),
            tracks: IndexMap::new(),
            frame: 0,
            v_ego_hist: history.clone(),
            a_ego_hist: history,
            v_ego: 0.,
            a_ego: 0.,
            last_timestamp: None,
            dt: None,
            init_samples: Vec::new(),
            init_done: false,
            history_maxlen,
            configured_period: period,
            numpy_period: false,
        })
    }

    pub fn push_ego(&mut self, v_ego: f64, a_ego: f64) -> Result<(), Error> {
        push_history(&mut self.v_ego_hist, v_ego, self.history_maxlen);
        self.v_ego = *self.v_ego_hist.front().ok_or(Error::EmptyHistory)?;
        push_history(&mut self.a_ego_hist, a_ego, self.history_maxlen);
        self.a_ego = *self.a_ego_hist.front().ok_or(Error::EmptyHistory)?;
        Ok(())
    }

    pub fn fallback(&mut self) -> Option<Data> {
        self.frame += 1;
        self.frame.is_multiple_of(5).then(Data::default)
    }

    fn estimate_dt(&mut self, time: f64, emit: &mut impl FnMut(&str)) -> Result<(), Error> {
        if self.configured_period > 0. {
            let dt = f64::from(self.configured_period);
            self.dt = Some(dt);
            self.init_done = true;
            emit(&format!("Using radar dt: {} sec\n", float_text(dt)?));
        } else if self.init_samples.len() > 100 {
            let differences: Vec<f64> = self.init_samples[50..]
                .windows(2)
                .map(|w| w[1] - w[0])
                .collect();
            let dt = pairwise(&differences) / differences.len() as f64;
            self.dt = Some(dt);
            self.init_done = true;
            self.numpy_period = true;
            emit(&format!("Estimated radar dt: {} sec\n", float_text(dt)?));
        } else {
            self.init_samples.push(time);
        }
        Ok(())
    }

    pub fn finish(
        &mut self,
        mut result: Option<Data>,
        time: f64,
        numerics: &mut Numerics,
        emit: &mut impl FnMut(&str),
    ) -> Result<Option<Data>, Error> {
        let Some(data) = result.as_mut() else {
            return Ok(None);
        };
        if !self.init_done {
            self.estimate_dt(time, emit)?;
            return Ok(None);
        }
        let dt = self
            .dt
            .ok_or(Error::Contract("initialized radar period absent"))?;
        let mut new_tracks = IndexMap::new();
        let mut order = Vec::new();
        for point in self.pts.values_mut() {
            let id = point.track_id;
            if !order.contains(&id) {
                order.push(id);
            }
            if let Some(track) = self.tracks.get_mut(&id) {
                track.update(point, self.a_ego, numerics)?;
                track.write_acceleration(point);
                point.y_rel = track.y_rel as f32;
                point.yv_rel = track.yv_rel as f32;
            } else {
                let mut track = Track::new_for_period(id, point, dt, self.numpy_period)?;
                track.update(point, self.a_ego, numerics)?;
                track.write_acceleration(point);
                point.y_rel = track.y_rel as f32;
                point.yv_rel = track.yv_rel as f32;
                new_tracks.insert(id, track);
            }
        }
        let mut next = IndexMap::new();
        for id in order {
            let track = self
                .tracks
                .shift_remove(&id)
                .or_else(|| new_tracks.shift_remove(&id))
                .ok_or(Error::Contract("processed track absent"))?;
            next.insert(id, track);
        }
        self.tracks = next;
        for point in &mut data.points {
            if let Some(track) = self.tracks.get(&point.track_id) {
                track.write_acceleration(point);
            }
        }
        Ok(result)
    }
}

fn push_history(values: &mut VecDeque<f64>, value: f64, maximum: usize) {
    if maximum == 0 {
        return;
    }
    if values.len() == maximum {
        values.pop_front();
    }
    values.push_back(value);
}

fn float_text(value: f64) -> Result<String, Error> {
    if value.is_nan() {
        return Ok("nan".into());
    }
    if value.is_infinite() {
        return Ok(if value > 0. { "inf" } else { "-inf" }.into());
    }
    let mut text = String::new();
    openpilot_runtime_core::python_float::write_float(value, &mut text)?;
    Ok(text)
}

fn pairwise(values: &[f64]) -> f64 {
    if values.len() < 8 {
        return values.iter().fold(-0., |sum, value| sum + value);
    }
    if values.len() <= 128 {
        let mut sums = [0.; 8];
        sums.copy_from_slice(&values[..8]);
        let mut index = 8;
        while index + 8 <= values.len() {
            for offset in 0..8 {
                sums[offset] += values[index + offset];
            }
            index += 8;
        }
        let total = ((sums[0] + sums[1]) + (sums[2] + sums[3]))
            + ((sums[4] + sums[5]) + (sums[6] + sums[7]));
        return values[index..].iter().fold(total, |sum, value| sum + value);
    }
    let middle = (values.len() / 2) / 8 * 8;
    pairwise(&values[..middle]) + pairwise(&values[middle..])
}
