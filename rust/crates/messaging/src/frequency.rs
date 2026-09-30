use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid frequency or window capacity")]
    Configuration,
    #[error("zero average receive interval")]
    ZeroInterval,
}

#[derive(Serialize)]
pub struct MovingAverage {
    pub buffer: Vec<f64>,
    pub index: usize,
    pub count: usize,
    pub sum: f64,
}

impl MovingAverage {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            index: 0,
            count: 0,
            sum: 0.0,
        }
    }

    fn add(&mut self, value: f64) {
        self.sum -= self.buffer[self.index];
        self.buffer[self.index] = value;
        self.sum += value;
        self.index = (self.index + 1) % self.buffer.len();
        self.count = (self.count + 1).min(self.buffer.len());
    }

    pub fn average(&self) -> f64 {
        if self.count == 0 {
            f64::NAN
        } else {
            self.sum / self.count as f64
        }
    }
}

#[derive(Serialize)]
pub struct FrequencyTracker {
    pub min_frequency: f64,
    pub max_frequency: f64,
    pub average: MovingAverage,
    pub recent: MovingAverage,
    pub previous_time: f64,
}

impl FrequencyTracker {
    pub fn new(
        service: f64,
        update: f64,
        is_poll: bool,
        min_update: Option<f64>,
        range: Option<(f64, f64)>,
    ) -> Result<Self, Error> {
        if !service.is_finite()
            || service < 0.0
            || !update.is_finite()
            || update < 0.0
            || min_update.is_some_and(|value| !value.is_finite() || value < 0.0)
            || range.is_some_and(|(min, max)| {
                !min.is_finite() || !max.is_finite() || min <= 0.0 || min > max
            })
        {
            return Err(Error::Configuration);
        }
        let frequency = service.min(update).max(1.0);
        if frequency > 100_000.0 {
            return Err(Error::Configuration);
        }
        let (min, max) = if let Some((min, max)) = range {
            if is_poll {
                (min, max)
            } else {
                (min.min(update), max.min(update))
            }
        } else if is_poll {
            (frequency, frequency)
        } else if let Some(min_update) = min_update {
            (service.min(min_update).max(1.0), frequency.min(update))
        } else {
            let min = if service >= 2.0 * update {
                update
            } else if update >= 2.0 * service {
                frequency
            } else {
                frequency.min(frequency / 2.0)
            };
            (min, frequency.min(update))
        };
        Ok(Self {
            min_frequency: min * 0.8,
            max_frequency: max * 1.2,
            average: MovingAverage::new((10.0 * frequency) as usize),
            recent: MovingAverage::new(frequency as usize),
            previous_time: 0.0,
        })
    }

    pub fn record(&mut self, time: f64) {
        if self.previous_time > 1e-5 {
            let interval = time - self.previous_time;
            self.average.add(interval);
            self.recent.add(interval);
        }
        self.previous_time = time;
    }

    pub fn valid(&self) -> Result<bool, Error> {
        if self.average.count == 0 {
            return Ok(false);
        }
        let average = self.average.average();
        if average == 0.0 {
            return Err(Error::ZeroInterval);
        }
        let frequency = 1.0 / average;
        if self.min_frequency <= frequency && frequency <= self.max_frequency {
            return Ok(true);
        }
        let recent = self.recent.average();
        if recent == 0.0 {
            return Err(Error::ZeroInterval);
        }
        let frequency = 1.0 / recent;
        Ok(self.min_frequency <= frequency && frequency <= self.max_frequency)
    }
}
