//! Scalar RunningStat/RunningStatFilter port of common/stat_live.py. Priors
//! store the source's sum of squared deviations (S), not a variance.
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RunningStat {
    #[serde(rename = "M")]
    pub mean: f64,
    #[serde(rename = "S")]
    pub squared_deviations: f64,
    pub n: u32,
    max_trackable: Option<u32>,
}
impl RunningStat {
    pub fn new(priors: (f64, f64, u32), max_trackable: Option<u32>) -> Self {
        Self {
            mean: priors.0,
            squared_deviations: priors.1,
            n: priors.2,
            max_trackable,
        }
    }
    fn std(&self) -> f64 {
        if self.n >= 2 {
            (self.squared_deviations / (f64::from(self.n) - 1.)).sqrt()
        } else {
            0.
        }
    }
    fn push(&mut self, value: f64) {
        if self.max_trackable.is_none_or(|max| self.n < max) {
            self.n += 1;
        }
        let previous = self.mean;
        self.mean = previous + (value - previous) / f64::from(self.n);
        self.squared_deviations += (value - previous) * (value - self.mean);
    }
}
#[derive(Debug, Serialize)]
pub struct RunningStatFilter {
    pub raw_stat: RunningStat,
    pub filtered_stat: RunningStat,
}
impl RunningStatFilter {
    pub fn new(priors: (f64, f64, u32), max_trackable: Option<u32>) -> Self {
        Self {
            raw_stat: RunningStat::new(priors, None),
            filtered_stat: RunningStat::new((0., 0., 0), max_trackable),
        }
    }
    pub fn push(&mut self, value: f64) {
        let previous_std = self.raw_stat.std();
        self.raw_stat.push(value);
        if self.raw_stat.std() - previous_std <= 0. {
            self.filtered_stat.push(value);
        }
    }
}
