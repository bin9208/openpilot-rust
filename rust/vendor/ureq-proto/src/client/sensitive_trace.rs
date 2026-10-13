use std::sync::{Mutex, Once};
use std::thread::{self, ThreadId};

use http::{HeaderValue, Request};
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::client::Call;

struct TraceCapture(Mutex<Vec<(ThreadId, Vec<String>)>>);

static CAPTURE: TraceCapture = TraceCapture(Mutex::new(Vec::new()));
static LOGGER: Once = Once::new();

impl Log for TraceCapture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        if record.level() != Level::Trace || record.target() != "ureq_proto::util" {
            return;
        }
        let mut captures = self.0.lock().unwrap();
        if let Some((_, rows)) = captures
            .iter_mut()
            .find(|(id, _)| *id == thread::current().id())
        {
            rows.push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

fn start_capture() {
    LOGGER.call_once(|| {
        log::set_logger(&CAPTURE).unwrap();
        log::set_max_level(LevelFilter::Trace);
    });
    CAPTURE
        .0
        .lock()
        .unwrap()
        .push((thread::current().id(), Vec::new()));
}

fn take_rows() -> Vec<String> {
    let mut captures = CAPTURE.0.lock().unwrap();
    let index = captures
        .iter()
        .position(|(id, _)| *id == thread::current().id())
        .unwrap();
    captures.swap_remove(index).1
}

fn serialize(name: &str, sensitive: bool) -> Vec<u8> {
    let mut value = HeaderValue::from_static("Basic c3ludGhldGljOnNlY3JldA==");
    value.set_sensitive(sensitive);
    let request = Request::builder()
        .uri("https://example.test/synthetic-only")
        .header(name, value)
        .body(())
        .unwrap();
    let mut call = Call::new(request).unwrap().proceed();
    let mut bytes = Vec::new();
    while !call.can_proceed() {
        let mut output = [0_u8; 80];
        let used = call.write(&mut output).unwrap();
        assert!(used > 0);
        bytes.extend_from_slice(&output[..used]);
    }
    bytes
}

#[test]
fn sensitive_request_suppresses_raw_trace_and_preserves_wire_bytes() {
    for name in ["authorization", "x-private"] {
        start_capture();
        let wire = serialize(name, true);
        let rows = take_rows();
        assert!(
            String::from_utf8(wire)
                .unwrap()
                .contains(&format!("{name}: Basic c3ludGhldGljOnNlY3JldA==\r\n"))
        );
        assert!(rows.is_empty(), "sensitive request wrote raw TRACE rows");
    }
}

#[test]
fn nonsensitive_request_keeps_raw_trace() {
    start_capture();
    serialize("x-public", false);
    assert!(!take_rows().is_empty());
}
