use super::{lock, Error};
use crate::{
    report,
    state::{Finish, Progress},
    worker::{Event, Output, Settings},
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Default)]
pub(super) struct Transfer {
    pub(super) total_bytes: u64,
    pub(super) completed: usize,
    pub(super) logical: u64,
    pub(super) transmitted: u64,
    pub(super) by_file: HashMap<usize, HashMap<String, u64>>,
    pub(super) samples: VecDeque<(f64, u64)>,
    pub(super) results: Vec<Option<Value>>,
}
impl Transfer {
    pub(super) fn record_progress(
        &mut self,
        index: usize,
        total: usize,
        bytes: u64,
        event: openpilot_web_upload::Progress<'_>,
        clock: impl FnOnce() -> f64,
    ) -> Result<Progress, Error> {
        let files = self.by_file.entry(index).or_default();
        let current = event.sent.min(event.size);
        let previous = match files.get_mut(event.filename) {
            Some(previous) => {
                let old = *previous;
                *previous = old.max(current);
                old
            }
            None => {
                files.insert(event.filename.to_owned(), current);
                0
            }
        };
        let current = previous.max(current);
        self.logical += current - previous;
        self.transmitted += event.chunk as u64;
        let now = clock();
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
        Ok(Progress {
            percent: Some(self.percent(total, bytes)),
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
    pub(super) fn percent(&self, total: usize, bytes: u64) -> f64 {
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
        (8.0 + (byte_ratio.max(step_ratio).clamp(0.0, 1.0) * 89.0).round_ties_even()).min(97.0)
    }
    pub(super) fn results(&self) -> Vec<Value> {
        self.results.iter().flatten().cloned().collect()
    }
}

#[cfg(test)]
#[path = "progress_allocation_tests.rs"]
mod progress_allocation_tests;
pub(super) struct Context {
    pub(super) settings: Settings,
    pub(super) webdav: Option<openpilot_web_upload::WebDav>,
    pub(super) segments: Vec<String>,
    pub(super) canceled: Arc<AtomicBool>,
    pub(super) output: Output,
    pub(super) transfer: Mutex<Transfer>,
    pub(super) remote: String,
    pub(super) device: String,
}
impl Context {
    pub(super) fn target(&self) -> &'static str {
        if self.webdav.is_some() {
            "webdav"
        } else {
            "web"
        }
    }
    pub(super) fn check(&self) -> Result<(), Error> {
        if self.canceled.load(Ordering::Acquire) {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    }
    pub(super) fn progress(&self, patch: Progress) -> Result<(), Error> {
        self.output.emit(Event::Progress { patch })
    }
    pub(super) fn append(&self, text: String) -> Result<(), Error> {
        self.output.emit(Event::Append { text })
    }
    pub(super) fn notified(&self, message: &str, percent: f64, current: i64) -> Result<(), Error> {
        self.progress(Progress {
            message: Some(message.into()),
            percent: Some(percent),
            current: Some(self.segments.len() as i64),
            total: Some(self.segments.len() as i64),
            phase: Some("notifying".into()),
            phase_current: Some(current),
            phase_total: Some(2),
            ..Progress::default()
        })
    }
    pub(super) fn cancel_result(&self) -> Result<Finish, Error> {
        let results = lock(&self.transfer)?.results();
        let uploaded = results.iter().filter(|item| item["ok"] == true).count();
        let mut result = json!({"ok":false,"canceled":true,"uploaded":uploaded,"total":self.segments.len(),"uploadedAt":chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),"target":self.target(),"remoteBasePath":self.remote,"meta":self.settings.metadata,"results":results,"message":format!("Canceled {uploaded}/{}",self.segments.len()),"error":"upload canceled"});
        result["shareText"] = json!(report::share_text(&result));
        self.append("CANCELED".into())?;
        self.progress(Progress {
            message: Some("Upload canceled".into()),
            percent: Some(0.0),
            phase: Some("canceled".into()),
            ..Progress::default()
        })?;
        Ok(Finish {
            ok: false,
            result: Some(result),
            error: Some("upload canceled".into()),
            status: Some("canceled".into()),
        })
    }
}
