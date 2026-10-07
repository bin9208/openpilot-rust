use crate::core::Error;
use openpilot_messaging::state::State;
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
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
    fence: Option<PathBuf>,
    limit: u64,
}
impl FrequencyTrace {
    pub fn open(path: &Path, fence: Option<PathBuf>, limit: u64) -> Result<Self, Error> {
        Ok(Self {
            output: BufWriter::new(File::create(path)?),
            previous: None,
            fence,
            limit,
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

    pub fn completed_step(&mut self, frame: i64) -> Result<(), Error> {
        if let Some(path) = &self.fence {
            if fence_due(path, frame, self.limit)? {
                self.output.flush()?;
                stop_fixture()?;
            }
        }
        Ok(())
    }
}

fn fence_due(path: &Path, frame: i64, limit: u64) -> std::io::Result<bool> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let target: i64 = text.trim().parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fixture phase fence requires an ASCII frame integer",
        )
    })?;
    if target < 0 || target as u64 >= limit || frame > target {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fixture phase fence is outside the remaining step bound",
        ));
    }
    Ok(frame == target)
}

#[allow(unsafe_code)]
fn stop_fixture() -> std::io::Result<()> {
    // SAFETY: SIGSTOP has no handler or pointer argument and suspends this
    // diagnostic fixture process until its owning harness explicitly resumes it.
    if unsafe { libc::raise(libc::SIGSTOP) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_explicit_fence_waits_for_its_exact_completed_frame() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("phase");
        assert!(!fence_due(&path, 4, 10).unwrap());
        fs::write(&path, "5\n").unwrap();
        assert!(!fence_due(&path, 4, 10).unwrap());
        assert!(fence_due(&path, 5, 10).unwrap());
        assert!(fence_due(&path, 6, 10).is_err());
        for invalid in ["-1", "10", "invalid", ""] {
            fs::write(&path, invalid).unwrap();
            assert!(fence_due(&path, 4, 10).is_err());
        }
        fs::remove_file(&path).unwrap();
        assert!(!fence_due(&path, 6, 10).unwrap());
    }
}
