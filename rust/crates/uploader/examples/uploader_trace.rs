use openpilot_logging::producer::Factory;
use openpilot_uploader::{
    http::{HttpTransfer, SigningKey},
    runtime::RuntimeEvents,
    Attributes, Backoff, Candidate, Error, Event, EventSink, Outcome, Transfer, TransferError,
    UploadResponse, Uploader, XattrCache,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Deserialize)]
struct Request {
    root: PathBuf,
    #[serde(default)]
    metered: bool,
    requested: Option<String>,
    #[serde(default = "status")]
    status: u16,
    #[serde(default = "length")]
    length: String,
    #[serde(default)]
    fail_transfer: bool,
    #[serde(default)]
    fail_mark: bool,
    #[serde(default)]
    steps: usize,
    upload: Option<PathBuf>,
    http: Option<Http>,
    backoff: Option<Vec<(Option<bool>, bool, f64)>>,
    log_endpoint: Option<String>,
}
fn status() -> u16 {
    200
}
fn length() -> String {
    "42".into()
}
#[derive(Deserialize)]
struct Http {
    api_host: String,
    persist: PathBuf,
    version: PathBuf,
    #[serde(default)]
    fake: bool,
    #[serde(default = "timeout")]
    timeout_ms: u64,
}
fn timeout() -> u64 {
    10_000
}
enum Transport {
    Fake {
        status: u16,
        length: String,
        fail: bool,
    },
    Http(HttpTransfer),
}
impl Transfer for Transport {
    fn upload(
        &mut self,
        key: &Path,
        path: &Path,
        events: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        match self {
            Self::Fake {
                status,
                length,
                fail,
            } => {
                if *fail {
                    Err(TransferError::Contract("fixture failure"))
                } else {
                    Ok(UploadResponse {
                        status: *status,
                        content_length: length.clone(),
                    })
                }
            }
            Self::Http(http) => http.upload(key, path, events),
        }
    }
}
struct TraceEvents {
    records: Vec<Event>,
    logger: Option<RuntimeEvents>,
}
impl EventSink for TraceEvents {
    fn emit(&mut self, event: Event) -> Result<(), openpilot_logging::Error> {
        if let Some(logger) = &mut self.logger {
            logger.emit(event.clone())?;
        }
        self.records.push(event);
        Ok(())
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
fn candidate(value: &Candidate) -> Value {
    json!([value.name.to_string_lossy(), value.key.to_string_lossy()])
}
fn result(result: Result<Outcome, Error>) -> Value {
    match result {
        Ok(Outcome::Idle) => Value::Null,
        Ok(Outcome::Success) => json!(true),
        Ok(Outcome::Failure) => json!(false),
        Err(Error::UninitializedLastException(_)) => json!({"error":"UnboundLocalError"}),
        Err(Error::ContentLength(_)) => json!({"error":"ValueError"}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    if let Some(values) = request.backoff {
        let mut backoff = Backoff::default();
        return Ok(json!(values
            .into_iter()
            .map(|(success, offroad, jitter)| backoff.next(
                match success {
                    None => Outcome::Idle,
                    Some(true) => Outcome::Success,
                    Some(false) => Outcome::Failure,
                },
                offroad,
                jitter
            ))
            .collect::<Vec<_>>()));
    }
    let transfer = if let Some(http) = request.http {
        Transport::Http(HttpTransfer {
            api_host: http.api_host,
            dongle_id: "0000000000000000".into(),
            key: SigningKey::load(&http.persist)?,
            version_header: http.version,
            fake_upload: http.fake,
            socket_timeout: Duration::from_millis(http.timeout_ms),
        })
    } else {
        Transport::Fake {
            status: request.status,
            length: request.length,
            fail: request.fail_transfer,
        }
    };
    let mut uploader = Uploader::new(
        request.root,
        transfer,
        Attr {
            cache: XattrCache::default(),
            fail: request.fail_mark,
        },
        TraceEvents {
            records: Vec::new(),
            logger: request
                .log_endpoint
                .map(|endpoint| {
                    Factory::new(endpoint).map(|factory| RuntimeEvents::new(factory.logger()))
                })
                .transpose()?,
        },
    );
    let files = uploader
        .list_upload_files(request.metered, request.requested.as_deref())?
        .iter()
        .map(candidate)
        .collect::<Vec<_>>();
    let next = uploader
        .next_file(request.metered, request.requested.as_deref())?
        .map(|file| candidate(&file));
    let mut results = Vec::new();
    if let Some(path) = request.upload {
        results.push(result(uploader.upload(
            &Candidate {
                name: path.file_name().ok_or("missing name")?.to_owned(),
                key: path.clone(),
                path: uploader.root.join(path),
            },
            1,
            request.metered,
        )));
    }
    for _ in 0..request.steps {
        let value = result(uploader.step(1, request.metered, request.requested.as_deref()));
        let failed = value.is_object();
        results.push(value);
        if failed {
            break;
        }
    }
    let events = uploader
        .events
        .records
        .iter()
        .filter_map(|event| event.name())
        .collect::<Vec<_>>();
    let last = uploader
        .last_filename
        .strip_prefix(&uploader.root)
        .unwrap_or(&uploader.last_filename)
        .to_string_lossy();
    Ok(json!({"files":files,"next":next,"results":results,"last":last,"events":events}))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        println!("{}", run(serde_json::from_str(&line?)?)?);
    }
    Ok(())
}
