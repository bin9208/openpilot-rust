use crate::core::Error;
use openpilot_messaging::state::State;
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufWriter, Read, Write},
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
    timing: Timing,
    record_started_ns: u64,
    previous_record_finished_ns: Option<u64>,
    schedstats_enabled: Option<bool>,
    schedstat_ns: Option<[u64; 3]>,
}

#[derive(Serialize)]
pub struct Timing {
    pub step_started_ns: u64,
    pub step_finished_ns: u64,
    pub thread_cpu_started_ns: u64,
    pub thread_cpu_finished_ns: u64,
    pub receive_ns: u64,
    pub stage_us: [i128; 10],
}

pub struct TraceInput<'a> {
    pub state: &'a State,
    pub controls_ready: bool,
    pub parser: Option<crate::registry::ParserReadiness>,
    pub timing: Timing,
}

pub struct FrequencyTrace {
    output: BufWriter<File>,
    previous: Option<f64>,
    fence: Option<PathBuf>,
    limit: u64,
    previous_record_finished_ns: Option<u64>,
    schedstats_enabled: Option<bool>,
}
impl FrequencyTrace {
    pub fn open(path: &Path, fence: Option<PathBuf>, limit: u64) -> Result<Self, Error> {
        Ok(Self {
            output: BufWriter::new(File::create(path)?),
            previous: None,
            fence,
            limit,
            previous_record_finished_ns: None,
            schedstats_enabled: schedstats_enabled(Path::new("/proc/sys/kernel/sched_schedstats")),
        })
    }
    pub fn record(&mut self, input: TraceInput<'_>) -> Result<(), Error> {
        let record_started_ns = super::io::monotonic_ns();
        let schedstat_ns = enabled_schedstat(
            self.schedstats_enabled,
            Path::new("/proc/thread-self/schedstat"),
        );
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
            timing: input.timing,
            record_started_ns,
            previous_record_finished_ns: self.previous_record_finished_ns,
            schedstats_enabled: self.schedstats_enabled,
            schedstat_ns,
        };
        serde_json::to_writer(&mut self.output, &sample).map_err(std::io::Error::other)?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        self.previous_record_finished_ns = Some(super::io::monotonic_ns());
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

fn bounded_text(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(129)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 128).then_some(text)
}

fn schedstats_enabled(path: &Path) -> Option<bool> {
    match bounded_text(path)?.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn enabled_schedstat(enabled: Option<bool>, path: &Path) -> Option<[u64; 3]> {
    if enabled != Some(true) {
        return None;
    }
    let text = bounded_text(path)?;
    let values: Vec<u64> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    values.try_into().ok()
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
    fn scheduler_receipts_distinguish_disabled_missing_zero_and_malformed() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("counter");
        assert_eq!(schedstats_enabled(&path), None);
        assert_eq!(enabled_schedstat(Some(true), &path), None);
        for (text, expected) in [
            ("0\n", Some(false)),
            ("1\n", Some(true)),
            ("2", None),
            ("invalid", None),
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(schedstats_enabled(&path), expected);
        }
        fs::write(&path, "0 0 0\n").unwrap();
        assert_eq!(enabled_schedstat(Some(true), &path), Some([0, 0, 0]));
        assert_eq!(enabled_schedstat(Some(false), &path), None);
        assert_eq!(enabled_schedstat(None, &path), None);
        fs::write(&path, "100 200 3\n").unwrap();
        assert_eq!(enabled_schedstat(Some(true), &path), Some([100, 200, 3]));
        for text in [
            "1 2",
            "1 2 3 4",
            "-1 2 3",
            "18446744073709551616 2 3",
            &"1".repeat(129),
        ] {
            fs::write(&path, text).unwrap();
            assert_eq!(enabled_schedstat(Some(true), &path), None);
        }
        assert_eq!(enabled_schedstat(Some(true), root.path()), None);
    }
    #[test]
    fn enabled_trace_preserves_stage_clocks_and_previous_flush_boundary() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("trace.jsonl");
        let mut trace = FrequencyTrace::open(&path, None, 2).unwrap();
        let state = State::new(&["carControl"], Default::default()).unwrap();
        for _ in 0..2 {
            trace
                .record(TraceInput {
                    state: &state,
                    controls_ready: false,
                    parser: None,
                    timing: Timing {
                        step_started_ns: 10,
                        step_finished_ns: 30,
                        thread_cpu_started_ns: 2,
                        thread_cpu_finished_ns: 4,
                        receive_ns: 20,
                        stage_us: [7; 10],
                    },
                })
                .unwrap();
        }
        let rows: Vec<serde_json::Value> = fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(rows[0]["previous_record_finished_ns"].is_null());
        let started = rows[0]["record_started_ns"].as_u64().unwrap();
        let finished = rows[1]["previous_record_finished_ns"].as_u64().unwrap();
        let next = rows[1]["record_started_ns"].as_u64().unwrap();
        assert!(started <= finished && finished <= next);
        for row in rows {
            assert_eq!(row["timing"]["step_started_ns"], 10);
            assert_eq!(row["timing"]["step_finished_ns"], 30);
            assert_eq!(row["timing"]["thread_cpu_started_ns"], 2);
            assert_eq!(row["timing"]["thread_cpu_finished_ns"], 4);
            assert_eq!(row["timing"]["receive_ns"], 20);
            assert_eq!(
                row["timing"]["stage_us"],
                serde_json::to_value([7; 10]).unwrap()
            );
        }
    }
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
