use serde::Serialize;
use std::collections::VecDeque;
pub const CAPACITY: usize = 300;
#[derive(Default)]
struct MinMax {
    minimum: VecDeque<(i64, f64)>,
    maximum: VecDeque<(i64, f64)>,
}
impl MinMax {
    fn push(&mut self, time: i64, value: f64, oldest: i64) {
        while self
            .minimum
            .back()
            .is_some_and(|(_, previous)| *previous > value)
        {
            self.minimum.pop_back();
        }
        while self
            .maximum
            .back()
            .is_some_and(|(_, previous)| *previous < value)
        {
            self.maximum.pop_back();
        }
        self.minimum.push_back((time, value));
        self.maximum.push_back((time, value));
        while self.minimum.front().is_some_and(|(time, _)| *time < oldest) {
            self.minimum.pop_front();
        }
        while self.maximum.front().is_some_and(|(time, _)| *time < oldest) {
            self.maximum.pop_front();
        }
    }
    fn minimum(&self) -> f64 {
        self.minimum.front().map_or(0.0, |(_, value)| *value)
    }
    fn maximum(&self) -> f64 {
        self.maximum.front().map_or(0.0, |(_, value)| *value)
    }
}
pub struct Samples {
    values: [[f64; CAPACITY]; 3],
    bounds: [MinMax; 3],
    pub size: i32,
    pub index: i32,
    pub time: i64,
    pub minimum: f64,
    pub maximum: f64,
    pub last_sample: Option<f64>,
}
impl Default for Samples {
    fn default() -> Self {
        Self {
            values: [[0.0; CAPACITY]; 3],
            bounds: std::array::from_fn(|_| MinMax::default()),
            size: 0,
            index: -1,
            time: -1,
            minimum: 0.0,
            maximum: 0.0,
            last_sample: None,
        }
    }
}
#[derive(Serialize)]
pub struct Snapshot {
    pub size: i32,
    pub index: i32,
    pub time: i64,
    pub minimum: f64,
    pub maximum: f64,
    pub last_sample: Option<f64>,
    pub latest: [f64; 3],
}
impl Samples {
    pub fn value(&self, series: usize, back: i32) -> f64 {
        let index = (self.index - back).rem_euclid(300);
        self.values[series][usize::try_from(index).unwrap_or(0)]
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            size: self.size,
            index: self.index,
            time: self.time,
            minimum: self.minimum,
            maximum: self.maximum,
            last_sample: self.last_sample,
            latest: std::array::from_fn(|series| self.value(series, 0)),
        }
    }
    pub fn sample(&mut self, now: f64, values: [f64; 3]) -> bool {
        use num_traits::ToPrimitive;
        if self.last_sample.is_some_and(|last| {
            (now - last)
                .partial_cmp(&0.05)
                .is_none_or(|order| order.is_lt())
        }) {
            return false;
        }
        self.last_sample = Some(match self.last_sample {
            None => now,
            Some(last) => {
                let steps = ((now - last) / 0.05).trunc().clamp(1.0, 5.0);
                last + steps * 0.05
            }
        });
        self.time += 1;
        self.index = (self.index + 1) % 300;
        self.size = (self.size + 1).min(300);
        let oldest = self.time - i64::from(self.size - 1);
        let index = self.index.to_usize().unwrap_or(0);
        for (series, value) in values.into_iter().enumerate() {
            self.values[series][index] = value;
            self.bounds[series].push(self.time, value, oldest);
        }
        let minimum = self
            .bounds
            .iter()
            .map(MinMax::minimum)
            .reduce(|a, b| if b < a { b } else { a })
            .unwrap_or(0.0);
        let maximum = self
            .bounds
            .iter()
            .map(MinMax::maximum)
            .reduce(|a, b| if b > a { b } else { a })
            .unwrap_or(0.0);
        self.minimum = if minimum > -2.0 { -2.0 } else { minimum };
        self.maximum = if maximum < 2.0 { 2.0 } else { maximum };
        true
    }
}
