use crate::{value::round3, Error, Fields, Number, Value};
use indexmap::IndexMap;

struct Metric {
    total: f64,
    maximum: Number,
    count: u64,
}

pub struct Diagnostics {
    component: String,
    interval: f64,
    started: f64,
    frames: u64,
    samples: IndexMap<String, Metric>,
    scheduler: Option<[u64; 3]>,
    schedstats_enabled: Option<bool>,
    pid: u32,
}

impl Diagnostics {
    pub fn new(
        component: &str,
        interval: f64,
        started: f64,
        scheduler: Option<[u64; 3]>,
        schedstats_enabled: Option<bool>,
        pid: u32,
    ) -> Self {
        Self {
            component: component.into(),
            interval,
            started,
            frames: 0,
            samples: IndexMap::new(),
            scheduler,
            schedstats_enabled,
            pid,
        }
    }

    pub fn record_with(
        &mut self,
        values: impl IntoIterator<Item = (String, Number)>,
        context: Fields,
        clock: impl FnOnce() -> f64,
        scheduler: impl FnOnce() -> Option<[u64; 3]>,
    ) -> Result<Option<Fields>, Error> {
        self.frames = self.frames.checked_add(1).ok_or(Error::CountOverflow)?;
        let values: IndexMap<_, _> = values.into_iter().collect();
        for (name, value) in values {
            if !value.as_float().is_finite() {
                continue;
            }
            let metric = self.samples.entry(name).or_insert(Metric {
                total: 0.0,
                maximum: Number::Float(f64::NEG_INFINITY),
                count: 0,
            });
            metric.total += value.as_float();
            if value.greater_than(metric.maximum) {
                metric.maximum = value;
            }
            metric.count = metric.count.checked_add(1).ok_or(Error::CountOverflow)?;
        }
        let now = clock();
        if now - self.started < self.interval {
            return Ok(None);
        }
        let mut metrics = Fields::new();
        for (name, metric) in &self.samples {
            metrics.insert(
                name.clone(),
                Value::Object(
                    [
                        (
                            "mean".into(),
                            Value::Float(round3(metric.total / metric.count as f64)?),
                        ),
                        ("max".into(), metric.maximum.rounded()?),
                        ("count".into(), Value::Integer(i128::from(metric.count))),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );
        }
        let current = scheduler();
        let mut scheduling = Fields::new();
        if let (Some(before), Some(after)) = (self.scheduler, current) {
            let delta =
                std::array::from_fn::<_, 3, _>(|i| i128::from(after[i]) - i128::from(before[i]));
            scheduling.insert("cpu_ms".into(), Value::Float(delta[0] as f64 / 1e6));
            scheduling.insert(
                "runqueue_wait_ms".into(),
                Value::Float(delta[1] as f64 / 1e6),
            );
            scheduling.insert("timeslices".into(), Value::Integer(delta[2]));
        }
        self.scheduler = current;
        let seconds = now - self.started;
        let frames = self.frames;
        self.started = now;
        self.frames = 0;
        self.samples.clear();
        let mut event: Fields = [
            ("component".into(), Value::Text(self.component.clone())),
            ("pid".into(), Value::Integer(i128::from(self.pid))),
            ("mono_time".into(), Value::Float(now)),
            ("seconds".into(), Value::Float(round3(seconds)?)),
            ("frames".into(), Value::Integer(i128::from(frames))),
            ("metrics".into(), Value::Object(metrics)),
            ("scheduler".into(), Value::Object(scheduling)),
            (
                "schedstats_enabled".into(),
                self.schedstats_enabled.map_or(Value::Null, Value::Bool),
            ),
        ]
        .into_iter()
        .collect();
        if context.contains_key("event") || context.keys().any(|key| event.contains_key(key)) {
            return Ok(None);
        }
        event.extend(
            context
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        Ok(Some(event))
    }
}
