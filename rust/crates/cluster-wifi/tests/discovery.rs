use openpilot_cluster_wifi::{discover_with, Datagram, Error, Registration};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::VecDeque, io, rc::Rc};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
struct Fake {
    calls: Rc<RefCell<Vec<Value>>>,
    replies: VecDeque<Vec<u8>>,
    fault: String,
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.calls.borrow_mut().push(json!(["close"]));
    }
}
impl Datagram for Fake {
    fn broadcast(&mut self) -> io::Result<()> {
        self.calls.borrow_mut().push(json!(["broadcast"]));
        Ok(())
    }
    fn send_to(&mut self, request: &[u8], target: &str, port: u16) -> io::Result<()> {
        self.calls
            .borrow_mut()
            .push(json!(["send", hex(request), target, port]));
        if self.fault == "send_error" {
            Err(io::Error::other("send failed"))
        } else {
            Ok(())
        }
    }
    fn timeout(&mut self, seconds: f64) -> io::Result<()> {
        self.calls.borrow_mut().push(json!(["timeout", seconds]));
        Ok(())
    }
    fn receive(&mut self, maximum: usize) -> io::Result<(Vec<u8>, String)> {
        self.calls.borrow_mut().push(json!(["receive", maximum]));
        match self.fault.as_str() {
            "timeout" => Err(io::Error::new(io::ErrorKind::TimedOut, "timed out")),
            "receive_error" => Err(io::Error::other("receive failed")),
            _ => Ok((self.replies.pop_front().unwrap(), "192.0.2.8".into())),
        }
    }
}
#[test]
fn original_discovery_order_authentication_errors_and_cleanup() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/discovery.json")).unwrap();
    let cases = fixture["input"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for (index, case) in cases.iter().enumerate() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let fault = case["fault"].as_str().unwrap();
        let registration = Registration::parse(&case["config"].to_string()).unwrap();
        let mut tick = 0;
        let result = discover_with(
            &registration,
            case["timeout"].as_f64().unwrap(),
            &["127.0.0.1", "192.0.2.1"],
            3333,
            || {
                calls.borrow_mut().push(json!(["nonce", 16]));
                if fault == "nonce_error" {
                    return Err(Error::from(io::Error::other("nonce failed")));
                }
                Ok(std::array::from_fn(|i| i as u8))
            },
            || {
                let value = f64::from(tick) / 8.;
                tick += 1;
                calls.borrow_mut().push(json!(["clock", value]));
                value
            },
            || {
                calls.borrow_mut().push(json!(["socket"]));
                if fault == "socket_error" {
                    return Err(io::Error::other("socket failed"));
                }
                Ok(Fake {
                    calls: calls.clone(),
                    fault: fault.into(),
                    replies: case["replies"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| unhex(v.as_str().unwrap()))
                        .collect(),
                })
            },
        );
        let mut result = match result {
            Ok(address) => json!({"address":address}),
            Err(error) => json!({"error":error.to_string()}),
        };
        result["calls"] = json!(*calls.borrow());
        assert_eq!(result, fixture["expected"][index], "source case {index}");
    }
}
