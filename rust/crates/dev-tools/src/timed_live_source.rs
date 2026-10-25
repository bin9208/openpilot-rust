//! Actual IPC execution of the independently ported original timed function.
use super::*;
use std::io::BufRead;
pub(super) fn run() -> Result<()> {
    let line = std::io::stdin()
        .lock()
        .lines()
        .next()
        .ok_or("missing live source configuration")??;
    let config = JsonValue::parse(&line)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    let directory = PathBuf::from(
        config
            .get("params_root")
            .and_then(|value| value.to_utf8())
            .ok_or("missing source root")?,
    );
    let mut source = timed_reference::Source::new(config, directory.join(&prefix))?;
    source.connect_logger(&format!("ipc:///tmp/logmessage{prefix}"))?;
    let service = source.service()?;
    let mut publisher = Publisher::for_runtime("clocks", CAPACITY)?;
    let mut subscriber = Subscriber::for_runtime(service, true, CAPACITY)?;
    let mut gps = json!({"updated":false,"log_mono_time":0,"has_fix":false,"longitude":0.,"unix_timestamp_millis":0});
    loop {
        let raw = subscriber.receive(Duration::from_secs(1))?;
        gps["updated"] = json!(raw.is_some());
        if let Some(raw) = raw {
            let mut bytes = raw.as_slice();
            let message = capnp::serialize::read_message_from_flat_slice(
                &mut bytes,
                capnp::message::ReaderOptions::new(),
            )?;
            let event = message.get_root::<event::Reader<'_>>()?;
            let value = match event.which()? {
                event::Which::GpsLocation(value) if service == "gpsLocation" => value?,
                event::Which::GpsLocationExternal(value) if service == "gpsLocationExternal" => {
                    value?
                }
                _ => return Err("unexpected source GPS event".into()),
            };
            gps = json!({"updated":true,"log_mono_time":event.get_log_mono_time(),"has_fix":value.get_has_fix(),"longitude":value.get_longitude(),"unix_timestamp_millis":value.get_unix_timestamp_millis()});
        }
        let row = JsonValue::parse(&serde_json::to_string(
            &json!({"kind":"step","monotonic":monotonic_ns()?,"gps":gps}),
        )?)?;
        let sleeps = source.sleeps.len();
        source.step_publish(&row, |clock| {
            let wire = clock_wire(
                clock["wall"]
                    .as_u64()
                    .ok_or_else(|| timed_reference::Failure {
                        kind: "OverflowError".into(),
                        message: "clock wall outside UInt64".into(),
                    })?,
                monotonic_ns().map_err(source_error)?,
                clock["valid"].as_bool().unwrap_or(false),
            );
            publisher.send(&wire).map_err(source_error)
        })?;
        if source.sleeps.len() > sleeps {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
}
fn source_error(error: impl std::fmt::Display) -> timed_reference::Failure {
    timed_reference::Failure {
        kind: "OSError".into(),
        message: error.to_string(),
    }
}
