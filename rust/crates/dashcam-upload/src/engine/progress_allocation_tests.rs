//! #449: compare the production progress path with the previous flat-key path.
use super::Transfer;
use crate::{state::Progress, Error};
use openpilot_web_upload::Progress as UploadProgress;
use stats_alloc::{Stats, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::{
    alloc::System,
    collections::{BTreeMap, HashMap, VecDeque},
    ffi::OsStr,
    fs::{self, File},
    hint::black_box,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;
const MEASURED_NAME: &str = "long-lived-recording-rlog-name";
const WARM_EVENTS: usize = 1000;
const CHILD_ENV: &str = "OPENPILOT_DASHCAM_PROGRESS_ALLOCATION_CHILD";

#[derive(Default)]
struct Prior {
    logical: u64,
    transmitted: u64,
    completed: usize,
    by_file: HashMap<(usize, String), u64>,
    samples: VecDeque<(f64, u64)>,
}
impl Prior {
    // The pre-#449 upload_one closure's bookkeeping, retained independently
    // here so the accepted progress patches and retry high-water remain visible.
    fn record_progress(
        &mut self,
        index: usize,
        total: usize,
        bytes: u64,
        event: UploadProgress<'_>,
        now: f64,
    ) -> Result<Progress, Error> {
        let key = (index, event.filename.to_owned());
        let previous = self.by_file.get(&key).copied().unwrap_or(0);
        let current = previous.max(event.sent.min(event.size));
        self.by_file.insert(key, current);
        self.logical += current - previous;
        self.transmitted += event.chunk as u64;
        let sent = self.transmitted;
        self.samples.push_back((now, sent));
        while self.samples.len() > 2 && self.samples.front().is_some_and(|(at, _)| now - at > 3.0) {
            self.samples.pop_front();
        }
        let (sample_time, sample_bytes) = self
            .samples
            .front()
            .copied()
            .ok_or_else(|| Error::Runtime("speed sample unavailable".into()))?;
        let rate =
            ((sent - sample_bytes) as f64 / (now - sample_time).max(0.001)).round_ties_even();
        let logical = self.logical.min(bytes);
        let byte_ratio = if bytes > 0 {
            self.logical as f64 / bytes as f64
        } else {
            0.0
        };
        let step_ratio = if total > 0 {
            self.completed as f64 / total as f64
        } else {
            0.0
        };
        let percent =
            (8.0 + (byte_ratio.max(step_ratio).clamp(0.0, 1.0) * 89.0).round_ties_even()).min(97.0);
        Ok(Progress {
            percent: Some(percent),
            phase: Some("uploading".into()),
            phase_current: Some(if bytes > 0 {
                logical
            } else {
                self.completed as u64
            } as i64),
            phase_total: Some(if bytes > 0 { bytes } else { total as u64 } as i64),
            bytes_current: Some(logical as i64),
            bytes_total: Some(bytes as i64),
            bytes_per_second: Some(rate as i64),
            ..Progress::default()
        })
    }
}

fn new_files(transfer: &Transfer) -> BTreeMap<(usize, String), u64> {
    transfer
        .by_file
        .iter()
        .flat_map(|(index, files)| {
            files
                .iter()
                .map(move |(name, sent)| ((*index, name.clone()), *sent))
        })
        .collect()
}

#[test]
fn production_progress_preserves_prior_chunk_retry_and_segment_records() {
    let mut old = Prior::default();
    let mut new = Transfer::default();
    old.samples.push_back((0.0, 0));
    new.samples.push_back((0.0, 0));
    let events = [
        (0, "rlog", 0, 10, 0, 0.1, 2, 25),
        (0, "rlog", 5, 10, 5, 0.2, 2, 25),
        (0, "rlog", 0, 10, 0, 0.3, 2, 25), // retry begins
        (0, "rlog", 4, 10, 4, 0.4, 2, 25), // below prior high-water
        (0, "rlog", 10, 10, 6, 0.5, 2, 25),
        (0, "rlog", 10, 10, 0, 0.6, 2, 25), // duplicate publication
        (0, "qlog", 3, 5, 3, 4.0, 2, 25),
        (1, "rlog", 7, 10, 7, 4.1, 2, 25), // same name, other segment
        (1, "", 0, 0, 0, 4.2, 2, 25),      // historical empty-name bookkeeping
        (1, "rlog", 12, 10, 2, 4.3, 2, 25), // clamp sent to size
        (1, "rlog", 12, 10, 2, 4.4, 0, 0), // zero-byte total fallback
    ];
    for (index, filename, sent, size, chunk, at, total, bytes) in events {
        if index == 1 && filename == "rlog" && sent == 7 {
            old.completed = 1;
            new.completed = 1;
        }
        let event = UploadProgress {
            filename,
            sent,
            size,
            chunk,
        };
        let original = old.record_progress(index, total, bytes, event, at).unwrap();
        let current = new
            .record_progress(index, total, bytes, event, || at)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(&original).unwrap(),
            "event {index}/{filename} at {at}"
        );
        assert_eq!(new.logical, old.logical);
        assert_eq!(new.transmitted, old.transmitted);
        assert_eq!(new.completed, old.completed);
        assert_eq!(new.samples, old.samples);
        assert_eq!(new_files(&new), old.by_file.clone().into_iter().collect());
    }
}

fn measured_prior(state: &mut Prior, count: usize, first: u64) -> (Stats, u64) {
    let before = GLOBAL.stats();
    let mut fingerprint = 0xcbf2_9ce4_8422_2325_u64;
    for i in 0..count {
        let sent = first + i as u64;
        let event = UploadProgress {
            filename: MEASURED_NAME,
            sent,
            size: 10_000,
            chunk: 1,
        };
        let patch = state
            .record_progress(0, 1, 10_000, event, 10.0 + sent as f64 * 0.01)
            .unwrap();
        for byte in black_box(serde_json::to_vec(&patch).unwrap()) {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    (GLOBAL.stats() - before, fingerprint)
}
fn measured_current(state: &mut Transfer, count: usize, first: u64) -> (Stats, u64) {
    let before = GLOBAL.stats();
    let mut fingerprint = 0xcbf2_9ce4_8422_2325_u64;
    for i in 0..count {
        let sent = first + i as u64;
        let event = UploadProgress {
            filename: MEASURED_NAME,
            sent,
            size: 10_000,
            chunk: 1,
        };
        let patch = state
            .record_progress(0, 1, 10_000, event, || 10.0 + sent as f64 * 0.01)
            .unwrap();
        for byte in black_box(serde_json::to_vec(&patch).unwrap()) {
            fingerprint = (fingerprint ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    (GLOBAL.stats() - before, fingerprint)
}

#[test]
fn production_progress_cold_and_warmed_allocations_are_measured_without_cpu_claim() {
    if std::env::var_os(CHILD_ENV).as_deref() != Some(OsStr::new("1")) {
        let directory = tempfile::tempdir().unwrap();
        let stdout = directory.path().join("stdout");
        let stderr = directory.path().join("stderr");
        let module = module_path!().split_once("::").unwrap().1;
        let name = format!(
            "{module}::production_progress_cold_and_warmed_allocations_are_measured_without_cpu_claim"
        );
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(&name)
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(CHILD_ENV, "1")
            .stdout(Stdio::from(File::create(&stdout).unwrap()))
            .stderr(Stdio::from(File::create(&stderr).unwrap()))
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(30) {
                let _ = child.kill();
                child.wait().unwrap();
                panic!("isolated progress allocation test exceeded 30 seconds");
            }
            thread::sleep(Duration::from_millis(20));
        };
        let stdout_bytes = fs::read(stdout).unwrap();
        std::io::stdout().write_all(&stdout_bytes).unwrap();
        std::io::stderr()
            .write_all(&fs::read(stderr).unwrap())
            .unwrap();
        assert!(
            status.success(),
            "isolated progress allocation test failed: {status}"
        );
        let text = std::str::from_utf8(&stdout_bytes).unwrap();
        assert!(
            text.contains("running 1 test") && text.contains("\"old_warm_allocations\""),
            "isolated progress allocation measurement did not execute exactly one test"
        );
        return;
    }
    let mut old = Prior::default();
    let mut new = Transfer::default();
    old.samples.push_back((0.0, 0));
    new.samples.push_back((0.0, 0));
    let (old_cold, old_cold_fingerprint) = measured_prior(&mut old, 1, 1);
    let (new_cold, new_cold_fingerprint) = measured_current(&mut new, 1, 1);
    assert_eq!(old_cold_fingerprint, new_cold_fingerprint);
    let (old_warm, old_warm_fingerprint) = measured_prior(&mut old, WARM_EVENTS, 2);
    let (new_warm, new_warm_fingerprint) = measured_current(&mut new, WARM_EVENTS, 2);
    assert_eq!(old_warm_fingerprint, new_warm_fingerprint);
    assert_eq!(new.logical, old.logical);
    assert_eq!(new.transmitted, old.transmitted);
    assert_eq!(new_files(&new), old.by_file.into_iter().collect());
    // Both paths perform identical progress serialization and maintain the
    // same warmed state. The prior path alone copies this name each callback.
    assert_eq!(
        old_warm.allocations.checked_sub(new_warm.allocations),
        Some(WARM_EVENTS)
    );
    assert_eq!(
        old_warm
            .bytes_allocated
            .checked_sub(new_warm.bytes_allocated),
        Some(WARM_EVENTS * MEASURED_NAME.len())
    );
    println!(
        "{}",
        serde_json::json!({
            "scope":"actual Transfer::record_progress, matching JSON observer serialization; no network or CPU claim",
            "cold_events":1,
            "warm_events":WARM_EVENTS,
            "old_cold_allocations":old_cold.allocations,
            "new_cold_allocations":new_cold.allocations,
            "old_cold_bytes":old_cold.bytes_allocated,
            "new_cold_bytes":new_cold.bytes_allocated,
            "old_warm_allocations":old_warm.allocations,
            "new_warm_allocations":new_warm.allocations,
            "old_warm_bytes":old_warm.bytes_allocated,
            "new_warm_bytes":new_warm.bytes_allocated,
            "semantic_fingerprint":old_warm_fingerprint
        })
    );
}
