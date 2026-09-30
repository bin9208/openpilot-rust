use crate::{
    catalog::{self, SourceFile},
    metadata, report,
    state::{Finish, Progress},
    worker::{self, Event, Output, Settings},
    Error,
};
use openpilot_web_upload::{FolderUpload, Observer, SessionMode};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
};

fn lock<T>(value: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, Error> {
    value
        .lock()
        .map_err(|error| Error::Runtime(error.to_string()))
}
fn source_error(error: Error) -> String {
    match error {
        Error::Http { status: 400, .. } => "Bad Request".into(),
        Error::Http { status: 404, .. } => "Not Found".into(),
        Error::Http { status: 409, .. } => "Conflict".into(),
        error => error.to_string(),
    }
}
fn pool<T: Send + 'static>(
    total: usize,
    concurrency: usize,
    action: impl Fn(usize) -> T + Send + Sync + 'static,
) -> mpsc::Receiver<(usize, T)> {
    let (sender, receiver) = mpsc::channel();
    let next = Arc::new(AtomicUsize::new(0));
    let action = Arc::new(action);
    for _ in 0..concurrency.min(total) {
        let (next, action, sender) = (next.clone(), action.clone(), sender.clone());
        thread::spawn(move || loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= total {
                break;
            }
            if sender.send((index, action(index))).is_err() {
                break;
            }
        });
    }
    receiver
}
#[derive(Default)]
struct Transfer {
    completed: usize,
    logical: u64,
    transmitted: u64,
    by_file: HashMap<(usize, String), u64>,
    samples: VecDeque<(f64, u64)>,
    results: Vec<Option<Value>>,
}
impl Transfer {
    fn percent(&self, total: usize, bytes: u64) -> f64 {
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
    fn results(&self) -> Vec<Value> {
        self.results.iter().flatten().cloned().collect()
    }
}
struct Context {
    settings: Settings,
    segments: Vec<String>,
    canceled: Arc<AtomicBool>,
    output: Output,
    transfer: Mutex<Transfer>,
    remote: String,
    device: String,
}
impl Context {
    fn check(&self) -> Result<(), Error> {
        if self.canceled.load(Ordering::Acquire) {
            Err(Error::Canceled)
        } else {
            Ok(())
        }
    }
    fn progress(&self, patch: Progress) -> Result<(), Error> {
        self.output.emit(Event::Progress { patch })
    }
    fn append(&self, text: String) -> Result<(), Error> {
        self.output.emit(Event::Append { text })
    }
    fn notified(&self, message: &str, percent: f64, current: i64) -> Result<(), Error> {
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
    fn cancel_result(&self) -> Result<Finish, Error> {
        let results = lock(&self.transfer)?.results();
        let uploaded = results.iter().filter(|item| item["ok"] == true).count();
        let mut result = json!({"ok":false,"canceled":true,"uploaded":uploaded,"total":self.segments.len(),"uploadedAt":chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),"target":"web","remoteBasePath":self.remote,"meta":self.settings.metadata,"results":results,"message":format!("Canceled {uploaded}/{}",self.segments.len()),"error":"upload canceled"});
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
pub(crate) fn run(
    mut settings: Settings,
    segments: Vec<String>,
    canceled: Arc<AtomicBool>,
    output: Output,
) -> Result<Finish, Error> {
    let fields = metadata::fields(&settings.metadata)?;
    let device = openpilot_web_upload::device_id(&fields)?;
    let car = settings
        .metadata
        .get("carName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("none");
    let directory = format!("{car} {device}").trim().to_owned();
    if settings.token.is_empty() {
        settings.token =
            openpilot_web_upload::create_session(&settings.base_url, &fields, SessionMode::Async)?;
    }
    let remote = format!("{}/routes/{directory}/", settings.base_url).replace('\\', "/");
    let total = segments.len();
    let context = Arc::new(Context {
        settings,
        segments,
        canceled,
        output,
        transfer: Mutex::new(Transfer {
            results: vec![None; total],
            ..Transfer::default()
        }),
        remote,
        device,
    });
    context.output.emit(Event::Context {
        metadata: context.settings.metadata.clone(),
        remote_base_path: context.remote.clone(),
    })?;
    context.output.emit(Event::Partial {
        results: Vec::new(),
    })?;
    context.progress(Progress {
        message: Some("Preparing upload".into()),
        current: Some(0),
        total: Some(total as i64),
        percent: Some(0.0),
        phase: Some("preparing".into()),
        phase_current: Some(0),
        phase_total: Some(total as i64),
        ..Progress::default()
    })?;
    match execute(context.clone()) {
        Ok(result) => Ok(Finish {
            ok: result["ok"] == true,
            result: Some(result),
            error: None,
            status: None,
        }),
        Err(Error::Canceled) => context.cancel_result(),
        Err(error) => Err(error),
    }
}
fn execute(context: Arc<Context>) -> Result<Value, Error> {
    context.check()?;
    let total = context.segments.len();
    let concurrency = context.settings.concurrency.clamp(1, 6);
    let prepare = context.clone();
    let receiver = pool(total, concurrency, move |index| {
        catalog::segment_dir(&prepare.settings.root, &prepare.segments[index])
            .and_then(|path| catalog::file_summary(&path))
            .map_err(source_error)
    });
    let mut prepared = vec![Err("segment preparation did not complete".to_owned()); total];
    let mut count = 0;
    for (index, result) in receiver {
        prepared[index] = result;
        count += 1;
        context.check()?;
        context.progress(Progress {
            message: Some(format!("Prepared {count}/{total}")),
            current: Some(count as i64),
            total: Some(total as i64),
            percent: Some(if total > 0 {
                (count as f64 / total as f64 * 8.0).round_ties_even()
            } else {
                8.0
            }),
            phase: Some("preparing".into()),
            phase_current: Some(count as i64),
            phase_total: Some(total as i64),
            ..Progress::default()
        })?;
    }
    let bytes = prepared
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .flatten()
        .try_fold(0u64, |sum, file| sum.checked_add(file.size))
        .ok_or_else(|| Error::Runtime("upload size overflow".into()))?;
    context.progress(Progress {
        message: Some("Uploading files".into()),
        current: Some(0),
        total: Some(total as i64),
        percent: Some(8.0),
        phase: Some("uploading".into()),
        phase_current: Some(0),
        phase_total: Some(if bytes > 0 { bytes } else { total as u64 } as i64),
        bytes_current: Some(0),
        bytes_total: Some(bytes as i64),
        bytes_per_second: Some(0),
    })?;
    lock(&context.transfer)?
        .samples
        .push_back((worker::clock().monotonic, 0));
    let upload = context.clone();
    let receiver = pool(total, concurrency, move |index| {
        upload_one(&upload, index, &prepared[index], bytes)
    });
    for (_, result) in receiver {
        result?;
    }
    context.check()?;
    let results = lock(&context.transfer)?.results();
    let uploaded = results.iter().filter(|item| item["ok"] == true).count();
    let mut result = json!({"ok":uploaded==results.len(),"uploaded":uploaded,"total":results.len(),"uploadedAt":chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),"target":"web","deviceId":context.device,"remoteBasePath":context.remote,"meta":context.settings.metadata,"results":results,"message":format!("{uploaded}/{} uploaded",results.len())});
    result["shareText"] = json!(report::share_text(&result));
    context.notified("Sending notification", 98.0, 0)?;
    context.check()?;
    result["webComplete"] = openpilot_web_upload::send_complete(
        &context.settings.base_url,
        &context.settings.token,
        &metadata::fields(&result)?,
    );
    context.notified("Sending notification", 99.0, 1)?;
    result["discord"] = metadata::send_webhook(&context.settings.webhook, &result);
    context.notified("Finalizing upload", 99.0, 2)?;
    Ok(result)
}
fn upload_one(
    context: &Context,
    index: usize,
    prepared: &Result<Vec<SourceFile>, String>,
    bytes: u64,
) -> Result<(), Error> {
    if context.canceled.load(Ordering::Acquire) {
        return Ok(());
    }
    let total = context.segments.len();
    let segment = &context.segments[index];
    let label = format!("[{}/{}] {segment}", index + 1, total);
    context.append(label.clone())?;
    let files = prepared.as_ref().map_or(&[][..], Vec::as_slice);
    let result = (|| -> Result<bool, String> {
        if let Err(error) = prepared {
            return Err(error.clone());
        }
        let directory =
            catalog::segment_dir(&context.settings.root, segment).map_err(source_error)?;
        let mut transport_error = None;
        let mut should_cancel = || {
            if let Err(error) = context.output.emit(Event::Touch) {
                transport_error = Some(error);
                return true;
            }
            context.canceled.load(Ordering::Acquire)
        };
        let mut progress =
            |event: openpilot_web_upload::Progress<'_>| -> Result<(), openpilot_web_upload::Error> {
                let update = || -> Result<(), Error> {
                    let mut transfer = lock(&context.transfer)?;
                    let key = (index, event.filename.to_owned());
                    let previous = transfer.by_file.get(&key).copied().unwrap_or(0);
                    let current = previous.max(event.sent.min(event.size));
                    transfer.by_file.insert(key, current);
                    transfer.logical += current - previous;
                    transfer.transmitted += event.chunk as u64;
                    let now = worker::clock().monotonic;
                    let sent = transfer.transmitted;
                    transfer.samples.push_back((now, sent));
                    while transfer.samples.len() > 2
                        && transfer
                            .samples
                            .front()
                            .is_some_and(|(at, _)| now - at > 3.0)
                    {
                        transfer.samples.pop_front();
                    }
                    let (sample_time, sample_bytes) = transfer
                        .samples
                        .front()
                        .copied()
                        .ok_or_else(|| Error::Runtime("speed sample unavailable".into()))?;
                    let rate = ((sent - sample_bytes) as f64 / (now - sample_time).max(0.001))
                        .round_ties_even();
                    let logical = transfer.logical.min(bytes);
                    context.progress(Progress {
                        percent: Some(transfer.percent(total, bytes)),
                        phase: Some("uploading".into()),
                        phase_current: Some(if bytes > 0 {
                            logical
                        } else {
                            transfer.completed as u64
                        } as i64),
                        phase_total: Some(if bytes > 0 { bytes } else { total as u64 } as i64),
                        bytes_current: Some(logical as i64),
                        bytes_total: Some(bytes as i64),
                        bytes_per_second: Some(rate as i64),
                        ..Progress::default()
                    })
                };
                update().map_err(|error| openpilot_web_upload::Error::Source(error.to_string()))
            };
        let filenames = files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let result = FolderUpload {
            local_folder: &directory,
            directory: &context.device,
            remote_path: segment,
            base_url: &context.settings.base_url,
            token: &context.settings.token,
            filenames: Some(&filenames),
        }
        .run(&mut Observer {
            cancel: Some(&mut should_cancel),
            progress: Some(&mut progress),
        })
        .map_err(|error| error.to_string());
        if let Some(error) = transport_error {
            return Err(error.to_string());
        }
        result
    })();
    if result.is_err() && context.canceled.load(Ordering::Acquire) {
        return Ok(());
    }
    let mut item = json!({"segment":segment,"route":catalog::route_name(segment),"segmentIndex":catalog::segment_index(segment),"ok":result.as_ref().is_ok_and(|ok|*ok),"remotePath":format!("{}{segment}",context.remote),"files":files});
    match result {
        Ok(_) => context.append(format!("{label} OK"))?,
        Err(error) => {
            item["error"] = json!(error);
            context.append(format!("{label} FAILED: {error}"))?;
        }
    }
    let mut transfer = lock(&context.transfer)?;
    transfer.results[index] = Some(item);
    transfer.completed += 1;
    context.output.emit(Event::Partial {
        results: transfer.results(),
    })?;
    context.progress(Progress {
        message: Some(format!("Uploaded {}/{total}", transfer.completed)),
        current: Some(transfer.completed as i64),
        total: Some(total as i64),
        percent: Some(transfer.percent(total, bytes)),
        phase: Some("uploading".into()),
        phase_current: Some(if bytes > 0 {
            transfer.logical.min(bytes)
        } else {
            transfer.completed as u64
        } as i64),
        phase_total: Some(if bytes > 0 { bytes } else { total as u64 } as i64),
        ..Progress::default()
    })
}
