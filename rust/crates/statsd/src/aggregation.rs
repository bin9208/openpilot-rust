use crate::{number, sort, Error};
use indexmap::IndexMap;
use openpilot_runtime_version::{python_str, BuildMetadata};

pub struct Tags {
    metadata: BuildMetadata,
    origin: openpilot_runtime_version::JsonValue,
    device_type: String,
}
impl Tags {
    /// Normalize origin before querying hardware; defer Python str until a metric is rendered.
    pub fn new(
        metadata: &BuildMetadata,
        device_type: impl FnOnce() -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let origin = metadata.openpilot.git_normalized_origin()?;
        Ok(Self {
            metadata: metadata.clone(),
            origin,
            device_type: device_type()?,
        })
    }
    fn prefix(&self, output: &mut Vec<u32>, measurement: (&str, bool)) -> Result<(), Error> {
        append(
            output,
            &format!("{},started={}", measurement.0, boolean(measurement.1)),
        );
        for (key, value) in [
            ("version", &self.metadata.openpilot.version),
            ("branch", &self.metadata.channel),
        ] {
            append(output, &format!(",{key}="));
            output.extend(python_str(value)?);
        }
        append(
            output,
            &format!(
                ",dirty={},origin=",
                boolean(self.metadata.openpilot.is_dirty)
            ),
        );
        output.extend(python_str(&self.origin)?);
        append(output, &format!(",deviceType={} ", self.device_type));
        Ok(())
    }
}
fn append(output: &mut Vec<u32>, text: &str) {
    output.extend(text.chars().map(u32::from));
}
fn boolean(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum MetricEvent<'a> {
    Accepted,
    Malformed(&'a str),
    Unknown(&'a str),
}
#[derive(Default)]
pub struct Metrics {
    gauges: IndexMap<String, f64>,
    samples: IndexMap<String, Vec<f64>>,
}
impl Metrics {
    pub fn ingest<'a>(&mut self, metric: &'a str) -> MetricEvent<'a> {
        let Some(kind) = metric.split('|').nth(1) else {
            return MetricEvent::Malformed(metric);
        };
        let name = metric.split(':').next().unwrap_or("");
        let Some(value) = metric
            .split('|')
            .next()
            .and_then(|first| first.split(':').nth(1))
            .and_then(number::parse)
        else {
            return MetricEvent::Malformed(metric);
        };
        match kind {
            "g" => {
                self.gauges.insert(name.into(), value);
            }
            "sa" => {
                self.samples.entry(name.into()).or_default().push(value);
            }
            kind => return MetricEvent::Unknown(kind),
        }
        MetricEvent::Accepted
    }
    /// Build output before clearing: serialization failures retain accumulated metrics.
    pub fn render(&mut self, tags: &Tags, flush: &Flush) -> Result<Vec<u32>, Error> {
        let mut output = Vec::new();
        let suffix = format!(
            "dongle_id=\"{}\" {}\n",
            flush.dongle_id.as_deref().unwrap_or("None"),
            flush.timestamp_ns
        );
        for (key, &value) in &self.gauges {
            tags.prefix(&mut output, (&format!("gauge.{key}"), flush.started))?;
            append(&mut output, &format!("value={},", number::repr(value)?));
            append(&mut output, &suffix);
        }
        for (key, values) in &mut self.samples {
            sort::sort(values);
            let count = values.len();
            tags.prefix(&mut output, (&format!("sample.{key}"), flush.started))?;
            append(&mut output, &format!("count={count},"));
            for (name, value) in [
                ("min", values[0]),
                ("max", values[count - 1]),
                ("mean", number::sum(values) / count as f64),
            ] {
                append(&mut output, &format!("{name}={},", number::repr(value)?));
            }
            for (percentile, label) in [(0.05, "p5"), (0.5, "p50"), (0.95, "p95")] {
                let index = (percentile * (count - 1) as f64).round_ties_even() as usize;
                append(
                    &mut output,
                    &format!("{label}={},", number::repr(values[index])?),
                );
            }
            append(&mut output, &suffix);
        }
        self.gauges.clear();
        self.samples.clear();
        Ok(output)
    }
}
pub struct Flush {
    pub started: bool,
    pub timestamp_ns: i128,
    pub dongle_id: Option<String>,
}
