use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Clock {
    pub wall: f64,
    pub monotonic: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub action: &'static str,
    pub status: String,
    pub cancel_requested: bool,
    pub log: String,
    pub progress: i64,
    pub revision: u64,
    pub phase: String,
    pub message: String,
    pub step_current: u64,
    pub step_total: u64,
    pub phase_current: u64,
    pub phase_total: u64,
    pub bytes_current: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
    pub error: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
    pub result: Option<Value>,
    #[serde(skip)]
    pub segments: Vec<String>,
    #[serde(skip)]
    pub activity_at: f64,
    #[serde(skip)]
    pub task_done: bool,
    #[serde(skip)]
    pub partial_results: Vec<Value>,
    #[serde(skip)]
    pub upload_meta: Value,
    #[serde(skip)]
    pub remote_base_path: String,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Progress {
    pub message: Option<String>,
    pub current: Option<i64>,
    pub total: Option<i64>,
    pub percent: Option<f64>,
    pub phase: Option<String>,
    pub phase_current: Option<i64>,
    pub phase_total: Option<i64>,
    pub bytes_current: Option<i64>,
    pub bytes_total: Option<i64>,
    pub bytes_per_second: Option<i64>,
}

#[derive(Deserialize, Serialize)]
pub struct Finish {
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("unsupported upload phase: {0}")]
pub struct InvalidPhase(String);

fn assign<T: PartialEq>(field: &mut T, value: T) -> bool {
    let changed = *field != value;
    *field = value;
    changed
}

impl Job {
    pub fn new(id: String, segments: Vec<String>, clock: Clock) -> Self {
        let total = segments.len() as u64;
        Self {
            id,
            action: "dashcam_upload",
            status: "running".into(),
            cancel_requested: false,
            log: String::new(),
            progress: 0,
            revision: 0,
            phase: "queued".into(),
            message: String::new(),
            step_current: 0,
            step_total: total,
            phase_current: 0,
            phase_total: total,
            bytes_current: 0,
            bytes_total: 0,
            bytes_per_second: 0,
            error: None,
            result: None,
            created_at: clock.wall,
            updated_at: clock.wall,
            segments,
            activity_at: clock.monotonic,
            task_done: false,
            partial_results: Vec::new(),
            upload_meta: Value::Null,
            remote_base_path: String::new(),
        }
    }
    pub fn touch(&mut self, clock: Clock) {
        self.updated_at = clock.wall;
        self.activity_at = clock.monotonic;
    }
    pub fn append(&mut self, text: Option<&str>, clock: Clock) {
        let Some(text) = text else { return };
        let chunk = text.replace("\r\n", "\n").replace('\r', "\n");
        if chunk.is_empty() {
            return;
        }
        if !self.log.is_empty() && !self.log.ends_with('\n') && !chunk.starts_with('\n') {
            self.log.push('\n');
        }
        self.log.push_str(&chunk);
        let excess = self.log.chars().count().saturating_sub(60_000);
        if excess > 0 {
            if let Some((boundary, _)) = self.log.char_indices().nth(excess) {
                self.log.drain(..boundary);
            }
        }
        self.touch(clock);
    }
    pub fn update(&mut self, patch: Progress, clock: Clock) -> Result<(), InvalidPhase> {
        let mut changed = false;
        if let Some(message) = patch.message {
            changed |= assign(&mut self.message, message);
        }
        if let Some(phase) = patch.phase {
            let normalized = phase.trim().to_lowercase();
            if ![
                "queued",
                "preparing",
                "uploading",
                "notifying",
                "canceling",
                "complete",
                "canceled",
                "failed",
            ]
            .contains(&normalized.as_str())
            {
                return Err(InvalidPhase(phase));
            }
            changed |= assign(&mut self.phase, normalized);
        }
        for (field, value) in [
            (&mut self.step_current, patch.current),
            (&mut self.step_total, patch.total),
            (&mut self.phase_current, patch.phase_current),
            (&mut self.phase_total, patch.phase_total),
            (&mut self.bytes_current, patch.bytes_current),
            (&mut self.bytes_total, patch.bytes_total),
            (&mut self.bytes_per_second, patch.bytes_per_second),
        ] {
            if let Some(value) = value {
                changed |= assign(field, value.max(0) as u64);
            }
        }
        let mut percent = patch
            .percent
            .unwrap_or(self.progress as f64)
            .round_ties_even()
            .clamp(0.0, 100.0) as i64;
        if self.status == "running" {
            percent = percent.max(self.progress);
        }
        changed |= assign(&mut self.progress, percent);
        if changed {
            self.revision += 1;
        }
        self.touch(clock);
        Ok(())
    }
    pub fn done(&self) -> bool {
        matches!(self.status.as_str(), "done" | "failed" | "canceled")
    }
    pub fn snapshot(&self) -> Result<Value, serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        value["ok"] = json!(true);
        value["done"] = json!(self.done());
        Ok(value)
    }
    pub fn cancel(&mut self, clock: Clock) -> Result<Value, serde_json::Error> {
        if self.done() {
            let mut response = self.snapshot()?;
            response["already_done"] = json!(true);
            return Ok(response);
        }
        self.cancel_requested = true;
        let changed = assign(&mut self.message, "Canceling upload".into())
            | assign(&mut self.phase, "canceling".into());
        if changed {
            self.revision += 1;
        }
        self.touch(clock);
        self.append(Some("Cancel requested"), clock);
        self.snapshot()
    }
    pub fn finish(&mut self, finish: Finish, clock: Clock) {
        self.status = finish
            .status
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| if finish.ok { "done" } else { "failed" }.into());
        self.phase = if self.status == "canceled" {
            "canceled"
        } else if finish.ok {
            "complete"
        } else {
            "failed"
        }
        .into();
        let result = finish
            .result
            .filter(|value| !value.is_null() && value.as_object().is_none_or(|v| !v.is_empty()))
            .unwrap_or_else(|| json!({"ok":finish.ok}));
        self.error = finish.error.filter(|s| !s.is_empty()).or_else(|| {
            if finish.ok {
                None
            } else {
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }
        });
        self.result = Some(result);
        if finish.ok {
            self.progress = 100;
            self.phase_current = 1;
            self.phase_total = 1;
        }
        self.revision += 1;
        self.touch(clock);
    }
    pub fn fail_running(&mut self, error: &str, clock: Clock) {
        if self.status != "running" {
            return;
        }
        let uploaded = self
            .partial_results
            .iter()
            .filter(|r| r.get("ok").and_then(Value::as_bool) == Some(true))
            .count();
        let result = json!({"ok":false,"error":error,"uploaded":uploaded,"total":self.segments.len(),"results":self.partial_results,"message":format!("Upload failed: {error}")});
        self.append(Some(&format!("FAILED: {error}")), clock);
        self.finish(
            Finish {
                ok: false,
                result: Some(result),
                error: Some(error.into()),
                status: None,
            },
            clock,
        );
    }
}

#[derive(Default)]
pub struct Jobs(pub Vec<Job>);
impl Jobs {
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Job> {
        self.0.iter_mut().find(|job| job.id == id)
    }
    pub fn create(&mut self, id: String, segments: Vec<String>, clock: Clock) {
        if let Some(job) = self.get_mut(&id) {
            *job = Job::new(id, segments, clock);
        } else {
            self.0.push(Job::new(id, segments, clock));
        }
        self.prune();
    }
    pub fn prune(&mut self) {
        let mut finished = self.0.iter().filter(|job| job.done()).collect::<Vec<_>>();
        finished.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        let remove = finished
            .into_iter()
            .skip(12)
            .map(|job| job.id.clone())
            .collect::<Vec<_>>();
        self.0.retain(|job| !remove.contains(&job.id));
    }
    pub fn expire(&mut self, now: Option<f64>, clock: Clock) {
        let now = now.unwrap_or(clock.monotonic);
        for job in &mut self.0 {
            if job.status != "running" {
                continue;
            }
            let activity = if job.activity_at == 0.0 {
                now
            } else {
                job.activity_at
            };
            if job.task_done || now - activity >= 1800.0 {
                job.fail_running(
                    if job.task_done {
                        "upload task ended without a final state"
                    } else {
                        "upload job expired after 30 minutes without activity"
                    },
                    clock,
                );
            }
        }
        self.prune();
    }
}
