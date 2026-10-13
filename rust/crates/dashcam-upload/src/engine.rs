mod context;
mod upload;
use context::{Context, Transfer};

use crate::{
    catalog, metadata, report,
    state::{Finish, Progress},
    worker::{self, Event, Output, Settings},
    Error,
};
use openpilot_web_upload::SessionMode;
use serde_json::{json, Value};
use std::{
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
pub(crate) fn run(
    mut settings: Settings,
    segments: Vec<String>,
    canceled: Arc<AtomicBool>,
    output: Output,
) -> Result<Finish, Error> {
    let webdav = settings
        .private_upload
        .as_deref()
        .map(openpilot_web_upload::WebDav::load)
        .transpose()
        .map_err(openpilot_web_upload::Error::from)?
        .flatten();
    let fields = metadata::fields(&settings.metadata)?;
    let device = openpilot_web_upload::device_id(&fields)?;
    let car = settings
        .metadata
        .get("carName")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("none");
    let directory = format!("{car} {device}").trim().to_owned();
    if webdav.is_none() && settings.token.is_empty() {
        settings.token =
            openpilot_web_upload::create_session(&settings.base_url, &fields, SessionMode::Async)?;
    }
    let remote = if webdav.is_some() {
        format!("routes/{directory}/")
    } else {
        format!("{}/routes/{directory}/", settings.base_url).replace('\\', "/")
    };
    let total = segments.len();
    let context = Arc::new(Context {
        settings,
        webdav,
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
    {
        let mut transfer = lock(&context.transfer)?;
        transfer.total_bytes = bytes;
        transfer.samples.push_back((worker::clock().monotonic, 0));
    }
    let upload = context.clone();
    let receiver = pool(total, concurrency, move |index| {
        upload::upload_one(&upload, index, &prepared[index])
    });
    for (_, result) in receiver {
        result?;
    }
    context.check()?;
    let results = lock(&context.transfer)?.results();
    let uploaded = results.iter().filter(|item| item["ok"] == true).count();
    let mut result = json!({"ok":uploaded==results.len(),"uploaded":uploaded,"total":results.len(),"uploadedAt":chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),"target":context.target(),"deviceId":context.device,"remoteBasePath":context.remote,"meta":context.settings.metadata,"results":results,"message":format!("{uploaded}/{} uploaded",results.len())});
    result["shareText"] = json!(report::share_text(&result));
    context.notified("Sending notification", 98.0, 0)?;
    context.check()?;
    if context.webdav.is_none() {
        result["webComplete"] = openpilot_web_upload::send_complete(
            &context.settings.base_url,
            &context.settings.token,
            &metadata::fields(&result)?,
        );
    } else {
        result["storageOnly"] = json!(true);
    }
    context.notified("Sending notification", 99.0, 1)?;
    result["discord"] = metadata::send_webhook(&context.settings.webhook, &result);
    context.notified("Finalizing upload", 99.0, 2)?;
    Ok(result)
}
