use openpilot_cluster_wifi::{Channel, Error, Queue};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::VecDeque, io, rc::Rc};
#[derive(Default)]
struct State {
    calls: Vec<Value>,
    sequence: u64,
    replies: VecDeque<(u64, Vec<u8>)>,
    fault: Option<String>,
}
struct Fake(Rc<RefCell<State>>);
impl Channel for Fake {
    fn send(&mut self, payload: &[u8], expect_reply: bool) -> Result<u64, Error> {
        let mut s = self.0.borrow_mut();
        let hex: String = payload.iter().map(|b| format!("{b:02x}")).collect();
        s.calls.push(json!(["send", hex, expect_reply]));
        if s.fault.as_deref() == Some("send") {
            return Err(io::Error::other("send failed").into());
        }
        s.sequence += 1;
        let sequence = s.sequence;
        let offset = u64::from(s.fault.as_deref() == Some("sequence"));
        s.replies
            .push_back((sequence + offset, vec![sequence as u8]));
        Ok(sequence)
    }
    fn result(&mut self) -> Result<(u64, Vec<u8>), Error> {
        let mut s = self.0.borrow_mut();
        s.calls.push(json!(["result"]));
        if s.fault.as_deref() == Some("result") {
            return Err(io::Error::other("result failed").into());
        }
        Ok(s.replies.pop_front().unwrap())
    }
    fn close(&mut self) {
        self.0.borrow_mut().calls.push(json!(["close"]));
    }
}
#[test]
fn original_frame_backpressure_command_order_and_disconnect_state() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/display.json")).unwrap();
    for (index, operations) in fixture["input"].as_array().unwrap().iter().enumerate() {
        let state = Rc::new(RefCell::new(State::default()));
        let mut queue = Queue::new(Some(Fake(state.clone())));
        for (step, operation) in operations.as_array().unwrap().iter().enumerate() {
            let kind = operation["kind"].as_str().unwrap();
            if kind == "closed" {
                queue = Queue::new(None);
            }
            state.borrow_mut().fault = operation["fault"].as_str().map(str::to_owned);
            state.borrow_mut().calls.push(json!(["lock"]));
            let result = queue.transaction(
                &vec![(step + 1) as u8; 512],
                kind == "frame",
                kind != "no_ack",
                |value| state.borrow_mut().calls.push(json!(["connected", value])),
            );
            state.borrow_mut().calls.push(json!(["unlock"]));
            let mut result = match result {
                Ok(bytes) => {
                    json!({"response":bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()})
                }
                Err(error) => json!({"error":error.to_string(), "cause":error.cause.to_string()}),
            };
            result["pending"] = json!(queue.pending());
            result["disconnected"] = json!(queue.disconnected());
            result["connected"] = json!(queue.connected());
            result["calls"] = json!(state.borrow().calls);
            assert_eq!(
                result, fixture["expected"][index][step],
                "scenario {index}, transaction {step}"
            );
            state.borrow_mut().calls.clear();
        }
    }
}
