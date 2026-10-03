use crate::core::Error;
use openpilot_messaging::state::State;
use serde::Serialize;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

#[derive(Serialize)]
struct Sample {
    frame: i64,
    receive_time: f64,
    interval: Option<f64>,
    average_frequency: Option<f64>,
    min_frequency: f64,
    max_frequency: f64,
    tracker_valid: bool,
    frequency_ok: bool,
    controls_ready: bool,
    parser: Option<crate::registry::ParserReadiness>,
}

pub struct TraceInput<'a> {
    pub state: &'a State,
    pub controls_ready: bool,
    pub parser: Option<crate::registry::ParserReadiness>,
}

pub struct FrequencyTrace {
    output: BufWriter<File>,
    previous: Option<f64>,
}
impl FrequencyTrace {
    pub fn open(path: &Path) -> Result<Self, Error> {
        Ok(Self {
            output: BufWriter::new(File::create(path)?),
            previous: None,
        })
    }
    pub fn record(&mut self, input: TraceInput<'_>) -> Result<(), Error> {
        let state = input.state;
        let topic = state.topic("carControl")?;
        let tracker = &topic.tracker;
        let interval = if topic.updated {
            let interval = self.previous.map(|previous| topic.receive_time - previous);
            self.previous = Some(topic.receive_time);
            interval
        } else {
            None
        };
        let sample = Sample {
            frame: state.frame(),
            receive_time: topic.receive_time,
            interval,
            average_frequency: if tracker.average.count > 0 {
                Some(1. / tracker.average.average())
            } else {
                None
            },
            min_frequency: tracker.min_frequency,
            max_frequency: tracker.max_frequency,
            tracker_valid: tracker
                .valid()
                .map_err(openpilot_messaging::state::Error::from)?,
            frequency_ok: topic.frequency_ok,
            controls_ready: input.controls_ready,
            parser: input.parser,
        };
        serde_json::to_writer(&mut self.output, &sample).map_err(std::io::Error::other)?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        Ok(())
    }
}
