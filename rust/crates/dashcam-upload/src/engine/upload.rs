use super::{catalog, lock, source_error, worker, Context, Error, Progress};
use crate::{catalog::SourceFile, worker::Event};
use openpilot_web_upload::{FolderUpload, Observer};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

pub(super) fn upload_one(
    context: &Context,
    index: usize,
    prepared: &Result<Vec<SourceFile>, String>,
) -> Result<(), Error> {
    if context.canceled.load(Ordering::Acquire) {
        return Ok(());
    }
    let bytes = lock(&context.transfer)?.total_bytes;
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
                    let patch = transfer.record_progress(index, total, bytes, event, || {
                        worker::clock().monotonic
                    })?;
                    context.progress(patch)
                };
                update().map_err(|error| openpilot_web_upload::Error::Source(error.to_string()))
            };
        let filenames = files
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        let mut observer = Observer {
            cancel: Some(&mut should_cancel),
            progress: Some(&mut progress),
        };
        let result = if let Some(webdav) = &context.webdav {
            let car = context
                .settings
                .metadata
                .get("carName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("none");
            webdav.upload_folder(
                openpilot_web_upload::LocalFiles {
                    folder: &directory,
                    names: &filenames,
                },
                openpilot_web_upload::RouteSegment {
                    directory: &format!("{car} {}", context.device),
                    name: segment,
                },
                &mut observer,
            )
        } else {
            FolderUpload {
                local_folder: &directory,
                directory: &context.device,
                remote_path: segment,
                base_url: &context.settings.base_url,
                token: &context.settings.token,
                filenames: Some(&filenames),
            }
            .run(&mut observer)
        }
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
