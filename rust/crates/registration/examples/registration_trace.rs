//! Synthetic dependency adapter for unchanged-source comparisons; never selects device hardware.
use openpilot_logging::{
    log_site,
    producer::Factory,
    record::{Level, Record},
};
use openpilot_params::Params;
use openpilot_registration::{Clock, Error, Hardware, Registration, Spinner};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, BufRead, Write},
    path::Path,
    rc::Rc,
    time::Duration,
};
type Trace = Rc<RefCell<Vec<Value>>>;
struct FixtureHardware {
    trace: Trace,
    serial: Value,
    imeis: VecDeque<Value>,
}
impl Hardware for FixtureHardware {
    fn serial(&mut self) -> Result<String, Error> {
        self.trace.borrow_mut().push(json!(["serial"]));
        self.serial
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::Hardware("serial fixture failure".into()))
    }
    fn imei(&mut self, slot: usize) -> Result<Option<String>, Error> {
        self.trace.borrow_mut().push(json!(["imei", slot]));
        match self.imeis.pop_front() {
            Some(Value::Null) => Ok(None),
            Some(Value::String(value)) => Ok(Some(value)),
            _ => Err(Error::Hardware("IMEI fixture failure".into())),
        }
    }
}
struct FixtureClock {
    trace: Trace,
    mono: f64,
    step: f64,
    utc: i64,
    sleeps: u64,
    max_sleeps: u64,
}
impl Clock for FixtureClock {
    fn monotonic(&mut self) -> f64 {
        let value = self.mono;
        self.mono += self.step;
        self.trace.borrow_mut().push(json!(["monotonic", value]));
        value
    }
    fn unix_seconds(&mut self) -> Result<i64, Error> {
        self.trace.borrow_mut().push(json!(["now", self.utc]));
        Ok(self.utc)
    }
    fn sleep(&mut self, duration: Duration) -> Result<(), Error> {
        self.trace
            .borrow_mut()
            .push(json!(["sleep", duration.as_secs()]));
        self.sleeps += 1;
        if self.sleeps > self.max_sleeps {
            return Err(Error::Contract("fixture stop"));
        }
        self.mono += duration.as_secs_f64();
        Ok(())
    }
}
struct FixtureSpinner {
    trace: Trace,
    fail: Option<String>,
}
impl FixtureSpinner {
    fn action(&mut self, name: &str, value: Option<&str>) -> Result<(), Error> {
        self.trace.borrow_mut().push(if let Some(value) = value {
            json!([name, value])
        } else {
            json!([name])
        });
        if self.fail.as_deref() == Some(name) {
            Err(Error::Spinner(format!("{name} fixture failure")))
        } else {
            Ok(())
        }
    }
}
impl Spinner for FixtureSpinner {
    fn start(&mut self) -> Result<(), Error> {
        self.action("spinner_start", None)
    }
    fn update(&mut self, text: &str) -> Result<(), Error> {
        self.action("spinner_update", Some(text))
    }
    fn close(&mut self) -> Result<(), Error> {
        self.action("spinner_close", None)
    }
}
fn category(error: &Error) -> &'static str {
    match error {
        Error::IdentityType => "identity_type",
        Error::Logging(_) | Error::TypedParams(openpilot_params_typed::Error::Logging(_)) => {
            "logging"
        }
        Error::Io(error) if error.kind() == io::ErrorKind::InvalidData => "decode",
        Error::Io(_) => "io",
        Error::Hardware(_) => "hardware",
        Error::Spinner(_) => "spinner",
        Error::Contract("fixture stop") => "stop",
        _ => "other",
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = io::stdin().lock().lines();
    let config: Value = serde_json::from_str(&lines.next().ok_or("missing config")??)?;
    let trace: Trace = Rc::new(RefCell::new(Vec::new()));
    let mut hardware = FixtureHardware {
        trace: Rc::clone(&trace),
        serial: config
            .get("serial")
            .cloned()
            .unwrap_or(json!("synthetic-serial")),
        imeis: config["imeis"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![json!("synthetic-imei"), Value::Null])
            .into(),
    };
    let mut clock = FixtureClock {
        trace: Rc::clone(&trace),
        mono: 0.0,
        step: config["step"].as_f64().unwrap_or(0.0),
        utc: config["utc"].as_i64().unwrap_or(1_700_000_000),
        sleeps: 0,
        max_sleeps: config["max_sleeps"].as_u64().unwrap_or(40),
    };
    let mut spinner = FixtureSpinner {
        trace: Rc::clone(&trace),
        fail: config["spinner_fail"].as_str().map(str::to_owned),
    };
    let params = Params::open(Path::new(config["params"].as_str().ok_or("params")?), "d")?;
    let endpoint = config["endpoint"].as_str().ok_or("endpoint")?;
    let closed = config["closed_log"].as_bool().unwrap_or(false);
    let mut logger = Factory::new(if closed {
        format!("{endpoint}-closed")
    } else {
        endpoint.into()
    })?
    .logger();
    if closed {
        logger.emit(
            log_site!(),
            Record::text(Level::Debug, "prepare closed socket".into()),
        )?;
        logger.close();
    }
    let mut registration = Registration {
        params: &params,
        persist: Path::new(config["persist"].as_str().ok_or("persist")?),
        source_root: Path::new(config["source_root"].as_str().ok_or("source_root")?),
        api_host: config["api_host"].as_str().ok_or("api_host")?,
        logger: &mut logger,
    };
    let result = if config["mode"] == "is_registered" {
        registration
            .is_registered_device()
            .map(|value| json!({"value":value}))
    } else {
        registration
            .register(
                &mut hardware,
                &mut clock,
                if config["spinner"].as_bool().unwrap_or(false) {
                    Some(&mut spinner)
                } else {
                    None
                },
            )
            .map(|value| json!({"value_json":value.to_json().expect("JSON formatting")}))
    };
    let outcome = match result {
        Ok(value) => value,
        Err(error) => json!({"error":category(&error),"detail":error.to_string()}),
    };
    if closed {
        logger = Factory::new(endpoint.into())?.logger();
    }
    logger.emit(
        log_site!(),
        Record::text(Level::Debug, "registration-fixture-end".into()),
    )?;
    println!(
        "{}",
        json!({"outcome":outcome,"trace":*trace.borrow(),"params_hex":params.get("DongleId").ok().flatten().map(|bytes|bytes.iter().map(|byte|format!("{byte:02x}")).collect::<String>())})
    );
    io::stdout().flush()?;
    let _ = lines.next();
    Ok(())
}
