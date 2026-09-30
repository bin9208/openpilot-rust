use openpilot_logging::{
    log_site,
    producer::{Delivery, Factory},
    record::{Level, Record},
};
use openpilot_uploader::{
    clear_locks,
    http::{HttpTransfer, SigningKey},
    runtime::RuntimeEvents,
    Attributes, Backoff, Candidate, Event, EventSink, Outcome, Uploader, XattrCache,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    endpoint: String,
    target: String,
    api: String,
    persist: PathBuf,
    version: PathBuf,
    action: String,
    #[serde(default)]
    fail_mark: bool,
    #[serde(default)]
    persistent: bool,
}
struct FaultEvents {
    healthy: RuntimeEvents,
    closed: RuntimeEvents,
    target: String,
    fired: bool,
    attempts: Vec<String>,
    persistent: bool,
}
impl EventSink for FaultEvents {
    fn emit(&mut self, event: Event) -> Result<(), openpilot_logging::Error> {
        let label = match &event {
            Event::Fields { name, .. } => name.to_string(),
            Event::Text { message, .. } => {
                if message.starts_with("upload_url v1.4") {
                    "debug_url".into()
                } else {
                    message.clone()
                }
            }
        };
        self.attempts.push(label.clone());
        if (!self.fired && self.target == label) || (self.fired && self.persistent) {
            self.fired = true;
            self.closed.emit(event)
        } else {
            self.healthy.emit(event)
        }
    }
}
struct Attr {
    cache: XattrCache,
    fail: bool,
}
impl Attributes for Attr {
    fn get(&mut self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        self.cache.get(path)
    }
    fn mark_uploaded(&mut self, path: &Path) -> io::Result<()> {
        if self.fail {
            Err(io::Error::from_raw_os_error(13))
        } else {
            self.cache.mark_uploaded(path)
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let request: Request = serde_json::from_str(&text)?;
    let mut closed = Factory::new(format!("{}-unbound", request.endpoint))?.logger();
    closed.emit(
        log_site!(),
        Record::text(Level::Debug, "prepare closed socket".into()),
    )?;
    closed.close();
    let saturated = request.target == "eagain";
    let endpoint = if saturated {
        format!("{}-saturated", request.endpoint)
    } else {
        request.endpoint
    };
    let mut healthy = Factory::new(endpoint)?.logger();
    if saturated {
        let mut dropped = false;
        for _ in 0..10_000 {
            if healthy.emit(
                log_site!(),
                Record::text(Level::Debug, "queue fixture".into()),
            )? == Delivery::Dropped
            {
                dropped = true;
                break;
            }
        }
        assert!(dropped, "ZMQ queue did not reach EAGAIN");
    }
    let events = FaultEvents {
        healthy: RuntimeEvents::new(healthy),
        closed: RuntimeEvents::new(closed),
        target: request.target,
        fired: false,
        attempts: Vec::new(),
        persistent: request.persistent,
    };
    let transfer = HttpTransfer {
        api_host: request.api,
        dongle_id: "0000000000000000".into(),
        key: SigningKey::load(&request.persist)?,
        version_header: request.version,
        fake_upload: false,
        socket_timeout: Duration::from_secs(10),
    };
    let mut uploader = Uploader::new(
        request.root,
        transfer,
        Attr {
            cache: XattrCache::default(),
            fail: request.fail_mark,
        },
        events,
    );
    let mut result = if request.action == "clear" {
        clear_locks(&uploader.root, &mut uploader.events).map(|()| Outcome::Idle)
    } else if request.action == "upload" {
        uploader.upload(
            &Candidate {
                name: "qlog".into(),
                key: "route--0/qlog.zst".into(),
                path: uploader.root.join("route--0/qlog"),
            },
            1,
            false,
        )
    } else {
        uploader.step(1, false, None)
    };
    if matches!(result, Ok(Outcome::Failure)) && request.action == "step" {
        if let Err(error) = uploader.events.emit(Event::text(
            log_site!(),
            Level::Info,
            "upload backoff 0.1".into(),
        )) {
            result = Err(error.into());
        }
    }
    let (status, error, delay) = match result {
        Ok(value) => (
            format!("{value:?}"),
            None,
            Some(Backoff::default().next(value, false, 0.0)),
        ),
        Err(error) => ("Error".into(), Some(format!("{error:?}")), None),
    };
    println!(
        "{}",
        json!({"status":status,"error":error,"delay":delay,"last":uploader.last_filename,"attempts":uploader.events.attempts,"fired":uploader.events.fired,"saturated":saturated})
    );
    if status == "Error" {
        return Err("uploader operation returned an error (see JSON observation)".into());
    }
    Ok(())
}
