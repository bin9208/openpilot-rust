use libloading::Library;
use openpilot_panda_usb::{Api, Enumerator, Error, Log, Session};
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};
#[path = "usb_fixture/concurrent.rs"]
mod concurrent;

struct Fixture {
    _library: Library,
    begin: unsafe extern "C" fn(*const c_char),
    operation: unsafe extern "C" fn(*const c_char),
    log: unsafe extern "C" fn(i32, *const c_char),
    consumed: unsafe extern "C" fn() -> i32,
    finish: unsafe extern "C" fn() -> *const c_char,
    concurrent_start: unsafe extern "C" fn(),
    concurrent_result: unsafe extern "C" fn() -> *const c_char,
}
impl Fixture {
    fn load(path: &PathBuf) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // SAFETY: this owned test library exports the declared fixture ABI; retaining
        // the Library also retains every callback stored in the Arc logger.
        let library = unsafe { Library::new(path)? };
        let owner = unsafe {
            Self {
                begin: *library.get(b"fixture_begin\0")?,
                operation: *library.get(b"fixture_operation\0")?,
                log: *library.get(b"fixture_log\0")?,
                consumed: *library.get(b"fixture_consumed\0")?,
                finish: *library.get(b"fixture_finish\0")?,
                concurrent_start: *library.get(b"fixture_concurrent_start\0")?,
                concurrent_result: *library.get(b"fixture_concurrent_result\0")?,
                _library: library,
            }
        };
        Ok(Arc::new(owner))
    }

    fn record(&self, log: Log) {
        let (level, text) = match log {
            Log::Initialization => (40, "libusb initialization error".to_owned()),
            Log::DeviceList => (40, "libusb can't get device list".to_owned()),
            Log::Issue {
                code,
                description,
                operation,
            } => (
                40,
                format!("usb error {code} \"{description}\" in {operation}"),
            ),
            Log::Disconnected => (40, "lost connection".to_owned()),
            Log::TransmitFull => (30, "Transmit buffer full".to_owned()),
            Log::Overflow(count) => (40, format!("overflow got 0x{:x}", count as u32)),
        };
        let text = CString::new(text).expect("fixture messages contain no nul");
        // SAFETY: the fixture copies this live C string before returning.
        unsafe { (self.log)(level, text.as_ptr()) };
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}
fn bytes(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    if value.is_null() {
        Ok(Vec::new())
    } else {
        serde_json::from_value(value.clone())
    }
}

fn run(
    input: Value,
    api: Arc<Api>,
    fixture: Arc<Fixture>,
    enumeration: &mut Option<Enumerator>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let raw = CString::new(serde_json::to_string(&input)?)?;
    // SAFETY: owned valid JSON remains live until the synchronous fixture copy.
    unsafe { (fixture.begin)(raw.as_ptr()) };
    let logger = Arc::clone(&fixture);
    if input["mode"] == "list" {
        if enumeration.is_none() {
            *enumeration = Some(Enumerator::new(
                api,
                Arc::new(move |event| logger.record(event)),
            )?);
        }
        let mut results = Vec::new();
        for _ in 0..number(&input, "repeats") {
            results.push(enumeration.as_ref().ok_or("missing enumeration")?.list()?);
        }
        // SAFETY: fixture-owned JSON is copied before any next mutation.
        let mut output: Value =
            serde_json::from_slice(unsafe { CStr::from_ptr((fixture.finish)()) }.to_bytes())?;
        output["failed"] = json!(false);
        output["serial"] = Value::Null;
        output["results"] = json!(results);
        return Ok(output);
    }
    let session = Session::open(
        api,
        &bytes(&input["serial"])?,
        Arc::new(move |event| logger.record(event)),
    );
    let (failed, serial, results) = match session {
        Err(Error::Connection) => (true, Value::Null, Vec::new()),
        Err(error) => return Err(error.into()),
        Ok(session) => {
            let mut results = Vec::new();
            if input["mode"] == "concurrent" {
                results.push(concurrent::run(
                    &session,
                    &fixture,
                    number(&input, "threads") as usize,
                    number(&input, "transfers"),
                )?);
            }
            for operation in input["operations"].as_array().into_iter().flatten() {
                let raw = CString::new(serde_json::to_string(operation)?)?;
                // SAFETY: the fixture copies this operation before transfer callbacks.
                unsafe { (fixture.operation)(raw.as_ptr()) };
                let mut buffer = vec![0; number(operation, "length") as usize];
                let initial = bytes(&operation["data"])?;
                buffer
                    .get_mut(..initial.len())
                    .ok_or("input exceeds fixture buffer")?
                    .copy_from_slice(&initial);
                let req = number(operation, "request") as u8;
                let value = number(operation, "value") as u16;
                let index = number(operation, "index") as u16;
                let endpoint = number(operation, "endpoint") as u8;
                let timeout = number(operation, "timeout") as u32;
                let result = match operation["op"]
                    .as_str()
                    .ok_or("missing fixture operation")?
                {
                    "disconnect" => {
                        session.disconnect();
                        0
                    }
                    "control_write" => session.control_write(req, value, index, timeout)?,
                    "control_read" => {
                        session.control_read(req, value, index, &mut buffer, timeout)?
                    }
                    "bulk_write" => session.bulk_write(endpoint, &mut buffer, timeout)?,
                    "bulk_read" => session.bulk_read(endpoint, &mut buffer, timeout)?,
                    _ => return Err("unknown fixture operation".into()),
                };
                // SAFETY: reads the fixture's count after the synchronous transfer.
                if unsafe { (fixture.consumed)() } != 1 {
                    return Err("unconsumed transfer script".into());
                }
                results.push(json!({"result": result, "data": buffer, "connected": session.connected(), "healthy": session.healthy()}));
            }
            (false, json!(session.serial()), results)
        }
    };
    // SAFETY: finish returns fixture-owned JSON, copied before any next mutation.
    let mut output: Value =
        serde_json::from_slice(unsafe { CStr::from_ptr((fixture.finish)()) }.to_bytes())?;
    output["failed"] = json!(failed);
    output["serial"] = serial;
    output["results"] = json!(results);
    Ok(output)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("missing fixture library")?,
    );
    // SAFETY: the owned comparison fixture is compiled against libusb.h and
    // supplies the synchronous ABI; no real device library is used by this tool.
    let api = unsafe { Api::load(&path)? };
    let fixture = Fixture::load(&path)?;
    let mut enumeration = None;
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        serde_json::to_writer(
            &mut out,
            &run(
                serde_json::from_str(&line?)?,
                Arc::clone(&api),
                Arc::clone(&fixture),
                &mut enumeration,
            )?,
        )?;
        writeln!(out)?;
    }
    Ok(())
}
