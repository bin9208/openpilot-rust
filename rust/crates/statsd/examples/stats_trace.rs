use openpilot_runtime_version::{build_metadata_from_dict, JsonValue};
use openpilot_statsd::{
    aggregation::{Flush, Metrics, Tags},
    Error,
};
use serde::Deserialize;
use std::io::{self, BufRead, Write};
#[derive(Deserialize)]
struct Frame {
    metrics: Vec<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata = build_metadata_from_dict(&JsonValue::parse(
        r#"{"channel":"test","openpilot":{"version":"v","git_origin":"https://github.com/test/openpilot.git"}}"#,
    )?)?;
    let tags = Tags::new(&metadata, || Ok("pc".into()))?;
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        let frame: Frame = serde_json::from_str(&line?)?;
        let mut metrics = Metrics::default();
        for metric in frame.metrics {
            metrics.ingest(&metric);
        }
        let points = metrics.render(
            &tags,
            &Flush {
                started: false,
                timestamp_ns: 123,
                dongle_id: Some("test".into()),
            },
        )?;
        let text = points
            .into_iter()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .ok_or(Error::Unicode)?;
        writeln!(output, "{}", serde_json::to_string(&text)?)?;
    }
    Ok(())
}
