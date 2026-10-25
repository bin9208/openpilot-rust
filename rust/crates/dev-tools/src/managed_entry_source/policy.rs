//! Source Sentry ordering, driven by independent Version/Git and actual Params.
use super::{log::Logger, params::Params, Error};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
pub(super) struct Sdk {
    root: PathBuf,
    prefix: String,
    failure: Option<String>,
    pub(super) calls: Vec<Value>,
    pub(super) enabled: bool,
    real: bool,
    tags: Map<String, Value>,
    user: Value,
    release: Value,
    environment: Value,
    pub(super) captured: Vec<Value>,
}
fn version(
    session: &mut crate::version_source::Session,
    operation: &str,
    path: &Path,
    cancelled: &Arc<AtomicBool>,
    deadline: Instant,
) -> Result<Value, Error> {
    let raw = serde_json::to_vec(&json!({"op":operation,"path":path}))
        .map_err(|error| Error::new("ValueError", error.to_string()))?;
    // One absolute budget covers the source initialization and all its Git
    // calls, matching the original managed controller's 15s ready-row limit.
    let value = session
        .request_controlled(&raw, deadline, Arc::clone(cancelled))
        .map_err(|error| Error::new(error.kind(), error.to_string()))?;
    let value: Value = serde_json::from_str(
        &value
            .to_json()
            .map_err(|error| Error::new("ValueError", error.to_string()))?,
    )
    .map_err(|error| Error::new("ValueError", error.to_string()))?;
    if let Some(error) = value.get("error") {
        return Err(Error::new(
            error.as_str().unwrap_or("Exception"),
            "source version request",
        ));
    }
    value
        .get("value")
        .cloned()
        .ok_or_else(|| Error::new("ValueError", "missing independent version value"))
}
fn property(metadata: &Value, key: &str) -> Result<Value, Error> {
    let value = &metadata["properties"][key];
    if let Some(error) = value.get("error") {
        return Err(Error::new(
            error.as_str().unwrap_or("Exception"),
            "source metadata property",
        ));
    }
    value
        .get("value")
        .cloned()
        .ok_or_else(|| Error::new("ValueError", format!("missing metadata property {key}")))
}
impl Sdk {
    pub(super) fn new(
        root: &Path,
        prefix: &str,
        request: &Value,
        environment: &BTreeMap<OsString, OsString>,
        cancelled: &Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        let mut session = crate::version_source::Session::new(root, environment)
            .map_err(|error| Error::new(error.kind(), error.to_string()))?;
        let base = root.join("base");
        let deadline = Instant::now() + Duration::from_secs(15);
        let metadata = version(&mut session, "metadata", &base, cancelled, deadline)?;
        let origin = metadata["openpilot"]["git_origin"].clone();
        let mut sdk = Self {
            root: root.into(),
            prefix: prefix.into(),
            failure: None,
            calls: Vec::new(),
            enabled: false,
            real: request["real_sdk"] == true,
            tags: Map::new(),
            user: Value::Null,
            release: Value::Null,
            environment: Value::Null,
            captured: Vec::new(),
        };
        if property(&metadata, "comma_remote")? == true
            && origin.as_str().is_some_and(|text| text.contains("commaai"))
        {
            let params = Params::new(&root.join("params"), prefix)?;
            // Params Cython catches STRING decoding ValueError and returns its
            // None default. Its warning belongs to the old Worker logger bound
            // when params_pyx was imported, not the later managed socket logger.
            let dongle = String::from_utf8(params.get("DongleId")).ok();
            if let Some(dongle) =
                dongle.filter(|value| !value.is_empty() && value != "UnregisteredDevice")
            {
                sdk.release = version(&mut session, "version", &base, cancelled, deadline)?;
                sdk.environment = json!(if property(&metadata, "tested_channel")? == true {
                    "release"
                } else {
                    "master"
                });
                sdk.call("init", json!({}))?;
                sdk.user = json!({"id":dongle});
                sdk.call("set_user", sdk.user.clone())?;
                for (key, value) in [
                    ("dirty", metadata["openpilot"]["is_dirty"].clone()),
                    ("origin", origin),
                    ("branch", metadata["channel"].clone()),
                    ("commit", metadata["openpilot"]["git_commit"].clone()),
                    ("device", json!("tici")),
                ] {
                    sdk.tag(key, value)?;
                }
                sdk.enabled = true;
            }
        }
        // managed_entry_reference installs failures only after init and clears
        // initialization observations. No fixture answers are read here.
        sdk.calls.clear();
        sdk.failure = request
            .get("sdk_failure")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(sdk)
    }
    fn call(&mut self, operation: &str, fields: Value) -> Result<(), Error> {
        let path = self
            .root
            .join("params")
            .join(&self.prefix)
            .join("CarrotException");
        let current = if path.is_file() {
            json!(fs::read_to_string(path)
                .map_err(Error::io)?
                .replace("\r\n", "\n")
                .replace('\r', "\n"))
        } else {
            Value::Null
        };
        self.calls
            .push(json!({"op":operation,"fields":fields,"carrot_exception":current}));
        if self.failure.as_deref() == Some(operation) {
            return Err(Error::new(
                "RuntimeError",
                format!("{operation} fixture failure"),
            ));
        }
        Ok(())
    }
    pub(super) fn tag(&mut self, key: &str, value: Value) -> Result<(), Error> {
        let raw = serde_json::to_string(&value)
            .map_err(|error| Error::new("ValueError", error.to_string()))?;
        self.call(
            if key == "daemon" {
                "daemon_tag"
            } else {
                "set_tag"
            },
            json!({"key":key,"value_json":raw}),
        )?;
        self.tags.insert(key.into(), value);
        Ok(())
    }
    pub(super) fn report(
        &mut self,
        logger: &Logger,
        error: &Error,
        params_root: &Path,
    ) -> Result<(), Error> {
        logger.emit(40, "crash", Some(error))?;
        let params = Params::new(params_root, &self.prefix)?;
        if params.get("CarrotExceptionSent") != b"1" {
            params.put("CarrotException", b"exception");
        }
        let mut report = || -> Result<(), Error> {
            self.call(
                "capture_exception",
                json!({"kind":error.kind,"message":error.message,"traceback":error.trace()}),
            )?;
            if self.real && self.enabled {
                // Explicit external SDK field boundary: selected fields of the
                // original SDK2.55.0 event, NOT a claim of executing that SDK.
                // The outer controller separately requires actual native HTTP.
                self.captured.push(json!({"tags":self.tags,"user":self.user,"release":self.release,"environment":self.environment,"platform":"python"}));
            }
            self.call("flush", Value::Null)
        };
        if let Err(sdk_error) = report() {
            logger.emit(40, "sentry exception", Some(&sdk_error))?;
        }
        Ok(())
    }
}
