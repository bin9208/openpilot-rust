use crate::{
    buckets::Buckets,
    history::{History, Input},
    random::RandomState,
    Error, Fit,
};
use openpilot_runtime_core::filters::FirstOrderFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    pub fingerprint: String,
    pub tuning: u16,
    pub torque: Option<[f64; 2]>,
}
#[derive(Clone, Debug)]
pub struct Car {
    pub identity: Identity,
    pub allowed_brand: bool,
}
#[derive(Debug)]
pub struct Packet {
    pub raw: [f64; 3],
    pub filtered: [f64; 3],
    pub live_valid: bool,
    pub use_params: bool,
    pub points: Option<Vec<[f64; 3]>>,
    pub count: usize,
    pub percent: i8,
    pub decay: f64,
    pub resets: f64,
}
pub struct Estimator {
    pub car: Car,
    pub buckets: Buckets,
    pub history: History,
    pub decay: f64,
    pub resets: f64,
    pub all_points: Vec<[f64; 2]>,
    pub filters: [FirstOrderFilter; 3],
    decimated: bool,
    track_all: bool,
    bounds: [[f64; 2]; 2],
    random: RandomState,
}
impl Estimator {
    pub fn new(car: Car, mode: (bool, bool), random: RandomState) -> Self {
        let (decimated, track_all) = mode;
        let [friction, factor] = car.identity.torque.unwrap_or([0.; 2]);
        let sanity = if decimated { [0.5, 0.8] } else { [0.3, 0.5] };
        Self {
            car,
            buckets: Buckets::new(decimated),
            history: History::default(),
            decay: 50.,
            resets: 1.,
            all_points: Vec::new(),
            filters: [factor, 0., friction].map(|x| FirstOrderFilter::new(x, 50., 0.05, true)),
            decimated,
            track_all,
            bounds: [
                [(1. - sanity[0]) * factor, (1. + sanity[0]) * factor],
                [(1. - sanity[1]) * friction, (1. + sanity[1]) * friction],
            ],
            random,
        }
    }
    pub fn reset(&mut self) {
        self.resets += 1.;
        self.decay = 50.;
        self.buckets = Buckets::new(self.decimated);
        self.history.clear_samples();
        self.all_points.clear();
    }
    pub fn handle(&mut self, input: Input) -> Result<Option<[f64; 2]>, Error> {
        let point = self.history.handle(input)?;
        if let Some([x, y]) = point {
            if y.abs() <= 1. {
                self.buckets.add(x, y);
            }
            if self.track_all {
                self.all_points.push([x, y]);
            }
        }
        Ok(point)
    }
    pub fn message(&mut self, fit: &mut impl Fit, points: bool) -> Result<Packet, Error> {
        let mut raw = [0.; 3];
        let mut live_valid = false;
        if self.buckets.calculable() {
            raw = fit.estimate(&self.buckets.sample(&mut self.random)?)?;
            if self.buckets.valid() {
                if raw.iter().any(|v| v.is_nan()) {
                    self.reset();
                } else {
                    live_valid = true;
                    let value = self.decay + 0.05;
                    self.decay = if value > 250. { 250. } else { value };
                    let values = [
                        clip(raw[0], self.bounds[0]),
                        raw[1],
                        clip(raw[2], self.bounds[1]),
                    ];
                    for (filter, value) in self.filters.iter_mut().zip(values) {
                        filter.update(value);
                        filter.update_alpha(self.decay);
                    }
                }
            }
        }
        Ok(Packet {
            raw,
            filtered: std::array::from_fn(|i| self.filters[i].value()),
            live_valid,
            use_params: self.car.allowed_brand && self.car.identity.torque.is_some(),
            points: points.then(|| self.buckets.points()),
            count: self.buckets.len(),
            percent: self.buckets.percent(),
            decay: self.decay,
            resets: self.resets,
        })
    }
}
fn clip(value: f64, [low, high]: [f64; 2]) -> f64 {
    if low.is_nan() || high.is_nan() {
        f64::NAN
    } else {
        value.max(low).min(high)
    }
}
