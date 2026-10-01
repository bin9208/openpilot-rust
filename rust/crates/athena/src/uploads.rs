use crate::{
    logging, methods,
    policy::{self, UploadItem},
    state::{self, Shared, Stop},
    upload_http, Error,
};
use num_traits::ToPrimitive;
use openpilot_logging::producer::Logger;
use openpilot_messaging::{runtime::SubMaster, state::Options};
use serde_json::json;
use std::{io, sync::Arc, time::Duration};

pub fn run(shared: Arc<Shared>, stop: Stop, worker: usize) -> Result<(), Error> {
    let mut subscriber = SubMaster::for_runtime(&["deviceState"], Options::default())?;
    let mut logger = shared.factory.logger();
    while !stop.requested() {
        let result = step(&shared, &stop, worker, &mut subscriber, &mut logger);
        if let Err(error) = result {
            logging::failure(&mut logger, "athena.upload_handler.exception", &error);
        }
    }
    Ok(())
}
fn step(
    shared: &Shared,
    stop: &Stop,
    worker: usize,
    subscriber: &mut SubMaster,
    logger: &mut Logger,
) -> Result<(), Error> {
    let mut state = shared.uploads()?;
    state.set_current(worker, None);
    if state.queued.0.is_empty() {
        state = shared
            .available
            .wait_timeout_while(state, Duration::from_secs(1), |state| {
                state.queued.0.is_empty()
            })
            .map_err(|_| Error::Contract("upload queue poisoned"))?
            .0;
    }
    let Some(mut item) = state.queued.pop() else {
        return Ok(());
    };
    item.current = true;
    state.set_current(worker, Some(item.clone()));
    if state.cancelled.remove(&item.id) {
        return Ok(());
    }
    drop(state);
    if item.expired(state::now_ms()?) {
        logging::event(
            logger,
            "athena.upload_handler.expired",
            json!({"item":item,"error":true}),
        )?;
        return Ok(());
    }
    subscriber.update(Duration::ZERO)?;
    if crate::ipc::device(subscriber)?.get_network_metered() && !item.allow_cellular {
        return retry(shared, stop, worker, &item, false, logger);
    }
    let device = crate::ipc::device(subscriber)?;
    let network = device.get_network_type().map_or(0, u16::from);
    let size = std::fs::metadata(&item.path)
        .ok()
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
        .unwrap_or(-1);
    let fields = json!({"fn":item.path,"sz":size,"network_type":network,"metered":device.get_network_metered(),"retry_count":item.retry_count});
    logging::event(logger, "athena.upload_handler.upload_start", fields.clone())?;
    let mut aborted = false;
    let result = upload_http::upload(&shared.upload_agent, &item, |size, current| {
        if let Err(error) = progress(shared, subscriber, &item, worker, stop, size, current) {
            aborted = matches!(error, Error::Stopped);
            return Err(io::Error::other(error.to_string()));
        }
        Ok(())
    });
    match result {
        _ if aborted => {
            logging::event(logger, "athena.upload_handler.abort", fields)?;
            retry(shared, stop, worker, &item, false, logger)?;
        }
        Ok(status) => {
            if policy::completed_status(status) {
                logging::event(logger, "athena.upload_handler.success", fields)?;
            } else {
                let mut fields = fields;
                fields["status_code"] = status.into();
                logging::event(logger, "athena.upload_handler.retry", fields)?;
                retry(shared, stop, worker, &item, true, logger)?;
            }
            methods::cache(shared, logger);
        }
        Err(error) if transient(&error) => {
            logging::event(logger, "athena.upload_handler.timeout", fields)?;
            retry(shared, stop, worker, &item, true, logger)?;
        }
        Err(error) => return Err(error),
    }
    Ok(())
}
fn retry(
    shared: &Shared,
    stop: &Stop,
    worker: usize,
    item: &UploadItem,
    increase: bool,
    logger: &mut Logger,
) -> Result<(), Error> {
    if let Some(item) = item.retry(increase) {
        shared.uploads()?.queued.push(item);
        shared.available.notify_one();
        methods::cache(shared, logger);
        shared.uploads()?.set_current(worker, None);
        stop.wait(Duration::from_secs(policy::RETRY_DELAY_SECONDS));
    }
    Ok(())
}
fn progress(
    shared: &Shared,
    subscriber: &mut SubMaster,
    item: &UploadItem,
    worker: usize,
    stop: &Stop,
    size: u64,
    current: u64,
) -> Result<(), Error> {
    if !item.allow_cellular {
        let now = state::mono_ns()?
            .to_f64()
            .ok_or(Error::Contract("clock range"))?
            / 1e9;
        if now - subscriber.state.topic("deviceState")?.receive_time > 1. {
            subscriber.update(Duration::ZERO)?;
            if crate::ipc::device(subscriber)?.get_network_metered() {
                return Err(Error::Stopped);
            }
        }
    }
    if stop.requested() {
        return Err(Error::Stopped);
    }
    let progress = if size == 0 {
        1.
    } else {
        current.to_f64().ok_or(Error::Contract("upload length"))?
            / size.to_f64().ok_or(Error::Contract("upload length"))?
    };
    shared.uploads()?.set_current(
        worker,
        Some(UploadItem {
            progress,
            ..item.clone()
        }),
    );
    Ok(())
}
fn transient(error: &Error) -> bool {
    match error {
        Error::Http(
            ureq::Error::Timeout(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Tls(_)
            | ureq::Error::Rustls(_),
        ) => true,
        Error::Http(ureq::Error::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof => true,
        Error::Http(ureq::Error::Io(error)) | Error::Io(error) => matches!(
            error.kind(),
            io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
                | io::ErrorKind::ConnectionRefused
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::NotConnected
        ),
        _ => false,
    }
}
