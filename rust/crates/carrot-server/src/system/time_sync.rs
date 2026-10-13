use super::{
    time_command::{Failure, Invocation},
    wifi::strip,
};
use crate::{Error, Value};
use chrono::Datelike;
use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use std::{fs, path::PathBuf, sync::Arc};

pub type CommandRecipient = dyn Fn(&Invocation) -> Result<(), Failure> + Send + Sync;

pub struct TimeSync {
    pub localtime: PathBuf,
    pub zoneinfo: PathBuf,
    pub now: Arc<dyn Fn() -> i64 + Send + Sync>,
    pub command: Arc<CommandRecipient>,
}

impl Default for TimeSync {
    fn default() -> Self {
        Self {
            localtime: "/data/etc/localtime".into(),
            zoneinfo: "/usr/share/zoneinfo".into(),
            now: Arc::new(|| chrono::Utc::now().timestamp()),
            command: Arc::new(super::time_command::run),
        }
    }
}

pub struct Request {
    pub epoch_ms: BigInt,
    pub timezone: String,
}

impl Request {
    pub fn parse(body: &Value) -> Result<Self, Error> {
        let Value::Object(_) = body else {
            return Err(Error::Source(format!(
                "'{}' object has no attribute 'get'",
                body.type_name()
            )));
        };
        let zone = body.get("timezone");
        let timezone = if !zone.truth() {
            "UTC".into()
        } else {
            let Value::Text(_) = zone else {
                return Err(Error::Source(format!(
                    "'{}' object has no attribute 'strip'",
                    zone.type_name()
                )));
            };
            let text = zone.string()?;
            if strip(&text).is_empty() {
                "UTC".into()
            } else {
                strip(&text).into()
            }
        };
        let epoch = body.get("epoch_ms");
        if !matches!(epoch, Value::Bool(_) | Value::Integer(_) | Value::Float(_)) {
            return Err(Error::Source("epoch_ms required".into()));
        }
        Ok(Self {
            epoch_ms: epoch.int()?,
            timezone,
        })
    }
}

impl TimeSync {
    fn exists_or_link(&self) -> bool {
        self.localtime.exists()
            || fs::symlink_metadata(&self.localtime).is_ok_and(|meta| meta.file_type().is_symlink())
    }

    pub fn sync(&self, request: &Request) -> Result<Value, Error> {
        let mut target: BigInt = &request.epoch_ms / 1000;
        if request.epoch_ms.is_negative() && !(&request.epoch_ms % BigInt::from(1000)).is_zero() {
            target -= 1;
        }
        let server = (self.now)();
        let diff = &target - server;
        let timezone = strip(&request.timezone);
        let timezone = if timezone.is_empty() { "UTC" } else { timezone };
        let zone = format!("{}/{timezone}", self.zoneinfo.display());
        let mut result = Value::object([
            ("ok", Value::Bool(true)),
            ("applied", Value::Bool(false)),
            ("server_epoch", Value::integer(server)),
            ("target_epoch", Value::Integer(target.clone())),
            ("diff_sec", Value::Integer(diff.clone())),
            ("timezone", Value::text(timezone)),
            ("threshold_sec", Value::integer(10)),
            ("steps", Value::Array(Vec::new())),
        ]);
        println!("[time_sync] request tz={timezone} target_epoch={target} server_epoch={server} diff_sec={diff}");
        if !std::path::Path::new(&zone).exists() {
            crate::json_fields::set(&mut result, "ok", Value::Bool(false))?;
            crate::json_fields::set(
                &mut result,
                "message",
                Value::text(&format!("zoneinfo not found: {zone}")),
            )?;
            return Ok(result);
        }
        let mut steps = Vec::new();
        let current = if self.exists_or_link() {
            fs::canonicalize(&self.localtime).ok()
        } else {
            None
        };
        if current.is_some_and(|path| path.as_os_str() == std::ffi::OsStr::new(&zone)) {
            steps.push(Value::object([(
                "timezone",
                Value::text("already matched"),
            )]));
        } else {
            let link = (|| {
                if self.exists_or_link() {
                    (self.command)(&Invocation::Argv(vec![
                        "sudo".into(),
                        "rm".into(),
                        "-f".into(),
                        self.localtime.to_string_lossy().into(),
                    ]))?;
                    steps.push(Value::object([(
                        "remove_localtime",
                        Value::text(&self.localtime.to_string_lossy()),
                    )]));
                }
                (self.command)(&Invocation::Argv(vec![
                    "sudo".into(),
                    "ln".into(),
                    "-s".into(),
                    zone.clone(),
                    self.localtime.to_string_lossy().into(),
                ]))?;
                steps.push(Value::object([("set_timezone_link", Value::text(&zone))]));
                Ok::<_, Failure>(())
            })();
            if let Err(error) = link {
                return Self::failure(result, steps, error, "timezone");
            }
        }
        let empty = fs::metadata(&self.localtime).map_or(true, |meta| meta.len() == 0);
        if diff.abs() <= BigInt::from(10) && !empty {
            crate::json_fields::set(&mut result, "steps", Value::Array(steps))?;
            crate::json_fields::set(
                &mut result,
                "message",
                Value::text("skip: time diff within threshold"),
            )?;
            return Ok(result);
        }
        let epoch = target
            .to_i64()
            .ok_or_else(|| Error::Source("timestamp out of range for platform time_t".into()))?;
        let date = chrono::DateTime::from_timestamp(epoch, 0)
            .ok_or_else(|| Error::Source("timestamp out of range for platform time_t".into()))?;
        if !(1..=9999).contains(&date.year()) {
            return Err(Error::Source(format!(
                "year {} is out of range",
                date.year()
            )));
        }
        let command = format!("TZ=UTC sudo date -s '{}'", date.format("%Y-%m-%d %H:%M:%S"));
        steps.push(Value::object([("date_cmd", Value::text(&command))]));
        match (self.command)(&Invocation::Shell(command)) {
            Ok(()) => {
                crate::json_fields::set(&mut result, "steps", Value::Array(steps))?;
                crate::json_fields::set(&mut result, "applied", Value::Bool(true))?;
                crate::json_fields::set(&mut result, "message", Value::text("time updated"))?;
                crate::json_fields::set(
                    &mut result,
                    "server_epoch_after",
                    Value::integer((self.now)()),
                )?;
                Ok(result)
            }
            Err(error) => Self::failure(result, steps, error, "date"),
        }
    }

    fn failure(
        mut result: Value,
        steps: Vec<Value>,
        failure: Failure,
        operation: &str,
    ) -> Result<Value, Error> {
        match failure {
            Failure::Boundary(error) => Err(error),
            Failure::Exit(message) => {
                crate::json_fields::set(&mut result, "steps", Value::Array(steps))?;
                crate::json_fields::set(&mut result, "ok", Value::Bool(false))?;
                crate::json_fields::set(
                    &mut result,
                    "message",
                    Value::text(&format!("failed to set {operation}: {message}")),
                )?;
                Ok(result)
            }
        }
    }
}
