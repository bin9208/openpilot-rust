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
    /// The metric whose largest frame of the interval is kept whole.
    peak_metric: Option<String>,
    /// That frame: its value of `peak_metric` and the `peak_sample` object.
    peak_sample: Option<(Number, Fields)>,
}

impl Diagnostics {
    /// carrot's `RuntimeDiagnostics(..., peak_metric=...)`: besides the
    /// per-metric mean and maximum, the one frame with the largest value of
    /// this metric is reported with all of its own values, so the stages of
    /// the worst frame can be read together instead of as separate maxima.
    pub fn with_peak_metric(mut self, metric: &str) -> Self {
        self.peak_metric = Some(metric.to_owned());
        self
    }

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
            peak_metric: None,
            peak_sample: None,
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
        for (name, value) in &values {
            let (name, value) = (name.clone(), *value);
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
        if let Some(peak) = self
            .peak_metric
            .as_ref()
            .and_then(|metric| values.get(metric))
            .copied()
            .filter(|peak| peak.as_float().is_finite())
        {
            if self
                .peak_sample
                .as_ref()
                .is_none_or(|(largest, _)| peak.greater_than(*largest))
            {
                let metrics: Fields = values
                    .iter()
                    .filter(|(_, value)| value.as_float().is_finite())
                    .map(|(name, value)| {
                        (
                            name.clone(),
                            match value {
                                Number::Integer(value) => Value::Integer(i128::from(*value)),
                                Number::Float(value) => Value::Float(*value),
                            },
                        )
                    })
                    .collect();
                let sample: Fields = [
                    ("mono_time".to_owned(), Value::Float(now)),
                    ("context".to_owned(), Value::Object(context.clone())),
                    ("metrics".to_owned(), Value::Object(metrics)),
                ]
                .into_iter()
                .collect();
                self.peak_sample = Some((peak, sample));
            }
        }
        if now - self.started < self.interval {
            return Ok(None);
        }
        let peak_sample = self.peak_sample.take();
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
        if context.contains_key("event")
            || context.keys().any(|key| event.contains_key(key))
            || (peak_sample.is_some() && context.contains_key("peak_sample"))
        {
            return Ok(None);
        }
        event.extend(
            context
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        if let Some((_, sample)) = peak_sample {
            event.insert("peak_sample".into(), Value::Object(sample));
        }
        Ok(Some(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(exchange: f64, reply: f64) -> Vec<(String, Number)> {
        vec![
            ("usb_exchange_ms".to_owned(), Number::Float(exchange)),
            ("ipc_reply_ms".to_owned(), Number::Float(reply)),
        ]
    }
    fn context(frame_id: i128) -> Fields {
        [("frame_id".to_owned(), Value::Integer(frame_id))]
            .into_iter()
            .collect()
    }

    #[test]
    fn peak_sample_is_the_one_frame_with_the_largest_peak_metric() {
        let mut diagnostics = Diagnostics::new("jetlinkd", 1.0, 0.0, None, None, 7)
            .with_peak_metric("usb_exchange_ms");
        let none = || None;
        // Three frames inside one interval; the slow one is not the last.
        assert!(diagnostics
            .record_with(frame(40.0, 0.2), context(1), || 0.1, none)
            .unwrap()
            .is_none());
        assert!(diagnostics
            .record_with(frame(63.5, 0.1), context(2), || 0.5, none)
            .unwrap()
            .is_none());
        let event = diagnostics
            .record_with(frame(41.0, 0.9), context(3), || 1.2, none)
            .unwrap()
            .expect("interval elapsed");
        let Value::Object(peak) = &event["peak_sample"] else {
            panic!("peak sample missing");
        };
        assert_eq!(peak["mono_time"], Value::Float(0.5));
        assert_eq!(peak["context"], Value::Object(context(2)));
        let Value::Object(metrics) = &peak["metrics"] else {
            panic!("peak metrics missing");
        };
        // The frame's own values, not the interval's separate maxima.
        assert_eq!(metrics["usb_exchange_ms"], Value::Float(63.5));
        assert_eq!(metrics["ipc_reply_ms"], Value::Float(0.1));
        // The next interval starts without a remembered peak.
        let next = diagnostics
            .record_with(frame(30.0, 0.1), context(4), || 2.3, none)
            .unwrap()
            .expect("second interval elapsed");
        let Value::Object(peak) = &next["peak_sample"] else {
            panic!("second peak sample missing");
        };
        assert_eq!(peak["context"], Value::Object(context(4)));
    }

    #[test]
    fn no_peak_sample_without_a_peak_metric() {
        let mut diagnostics = Diagnostics::new("modeld", 1.0, 0.0, None, None, 7);
        let event = diagnostics
            .record_with(frame(40.0, 0.2), context(1), || 1.5, || None)
            .unwrap()
            .expect("interval elapsed");
        assert!(!event.contains_key("peak_sample"));
    }
}
