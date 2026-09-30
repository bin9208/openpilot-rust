//! Strict original artifact identity and frame/sequence guard around the v2 peer.
//! Wire peer ported from third_party/jetlink/client.py (MIT, Copyright (c) 2026 Zeph Leggett).
use crate::{
    contract::{self, Identity},
    owner::Backend,
    rpc,
    wire::{self, Header},
    Deadline, Error,
};
use serde_json::{json, Value};
use std::{fs::File, io::Read, time::Duration};
pub struct Message {
    pub header: Header,
    pub payload: Vec<u8>,
}
pub trait Transport: Send + 'static {
    fn send(&mut self, bytes: &[u8], deadline: Deadline) -> Result<(), Error>;
    fn receive(&mut self, deadline: Deadline) -> Result<Message, Error>;
    fn close(&mut self);
}
pub struct Client<T: Transport> {
    transport: T,
    sequence: u32,
    last_frame: Option<u32>,
    ready: bool,
    dead: bool,
}
impl<T: Transport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            sequence: 0,
            last_frame: None,
            ready: false,
            dead: false,
        }
    }
    fn send(&mut self, kind: u16, payload: &[u8], deadline: Deadline) -> Result<u32, Error> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Error::Contract("sequence exhausted"))?;
        self.transport
            .send(&wire::frame(kind, self.sequence, payload, true)?, deadline)?;
        Ok(self.sequence)
    }
    fn expect(&mut self, kind: u16, sequence: u32, deadline: Deadline) -> Result<Message, Error> {
        loop {
            deadline.remaining()?;
            let message = self.transport.receive(deadline)?;
            if message.header.kind == kind && message.header.sequence == sequence {
                return Ok(message);
            }
            if message.header.kind == 14 {
                return Err(Error::Contract("peer error"));
            }
            // Progress, engine notifications, and replies abandoned by an earlier call do not acquire this request's identity.
        }
    }
    fn connect_inner(&mut self) -> Result<Identity, Error> {
        if self.dead {
            return Err(Error::Closed);
        }
        let mut nonce = [0; 4];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let nonce: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
        let deadline = Deadline::after(Duration::from_secs(5))?;
        let seq = self.send(
            1,
            &serde_json::to_vec(&json!({"client":{"name":"jetlinkd-rs","nonce":nonce}}))?,
            deadline,
        )?;
        let hello: Value = serde_json::from_slice(&self.expect(2, seq, deadline)?.payload)?;
        if hello["protocol"] != 2
            || hello["backend"] != "ort"
            || hello["runtime_version"] != "1.22.0"
            || hello["telemetry"]["artifact_sha256"] != contract::SHA256
            || hello["telemetry"]["source_sha256"] != contract::SHA256
        {
            return Err(Error::Contract("peer runtime/artifact"));
        }
        let deadline = Deadline::after(Duration::from_secs(60))?;
        let seq = self.send(
            3,
            &serde_json::to_vec(
                &json!({"sha256":contract::SHA256,"nbytes":766354845,"frame_skip":4}),
            )?,
            deadline,
        )?;
        let mut state: Value = serde_json::from_slice(&self.expect(4, seq, deadline)?.payload)?;
        let deadline = Deadline::after(Duration::from_secs(300))?;
        loop {
            match state["state"].as_str() {
                Some("ready") => break,
                Some("failed" | "need_upload") => {
                    return Err(Error::Contract("peer engine unavailable"))
                }
                _ => {}
            }
            let message = self.transport.receive(deadline)?;
            match message.header.kind {
                4 => {
                    let candidate: Value = serde_json::from_slice(&message.payload)?;
                    if candidate["sha256"].is_null() || candidate["sha256"] == contract::SHA256 {
                        state = candidate;
                    }
                }
                14 => return Err(Error::Contract("peer engine error")),
                _ => {}
            }
        }
        if state["spec"] != contract::contract()? {
            return Err(Error::Contract("peer model contract"));
        }
        self.ready = true;
        let mut identity = Identity::new();
        for key in [
            "device_model",
            "android_api",
            "backend_requested",
            "app_version",
            "artifact_sha256",
        ] {
            let value = &hello["telemetry"][key];
            let text = if value.is_null() {
                String::new()
            } else if let Some(text) = value.as_str() {
                text.to_owned()
            } else {
                value.to_string()
            };
            identity.insert(key.to_owned(), text.chars().take(200).collect());
        }
        identity.insert("runtime_version".to_owned(), "1.22.0".to_owned());
        Ok(identity)
    }
    fn infer_inner(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error> {
        if !self.ready || self.dead {
            return Err(Error::Closed);
        }
        deadline.remaining()?;
        if !reset && self.last_frame.is_some_and(|previous| frame <= previous) {
            return Err(Error::Contract("frame order"));
        }
        contract::check_input(warped, packed)?;
        if self.sequence >= 0xffff_fffe {
            return Err(Error::Contract("sequence exhausted; reconnect offroad"));
        }
        let mut payload = Vec::with_capacity(8 + warped.len() + packed.len() * 4);
        payload.extend(frame.to_le_bytes());
        payload.extend(u32::from(reset).to_le_bytes());
        payload.extend(warped);
        for value in packed {
            payload.extend(value.to_le_bytes());
        }
        let seq = self.send(8, &payload, deadline)?;
        let response = self.expect(9, seq, deadline)?;
        if response.payload.len() < 20 + contract::OUTPUT_FLOATS * 4
            || wire::word(&response.payload, 0) != frame
            || wire::word(&response.payload, 4) != 0
        {
            return Err(Error::Contract("peer inference frame/status/length"));
        }
        let output = rpc::decode_floats(&response.payload[20..20 + contract::OUTPUT_FLOATS * 4])?;
        contract::check_output(&output)?;
        deadline.remaining()?;
        self.last_frame = Some(frame);
        Ok(output)
    }
}
impl<T: Transport> Backend for Client<T> {
    fn connect(&mut self) -> Result<Identity, Error> {
        let result = self.connect_inner();
        if result.is_err() {
            self.close();
        }
        result
    }
    fn infer(
        &mut self,
        frame: u32,
        warped: &[u8],
        packed: &[f32],
        deadline: Deadline,
        reset: bool,
    ) -> Result<Vec<f32>, Error> {
        let result = self.infer_inner(frame, warped, packed, deadline, reset);
        if result.is_err() {
            self.close();
        }
        result
    }
    fn dead(&self) -> bool {
        self.dead
    }
    fn close(&mut self) {
        if !self.dead {
            self.dead = true;
            self.transport.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct WireFixture {
        replies: VecDeque<Message>,
        sent: usize,
        closed: bool,
    }
    impl Transport for WireFixture {
        fn send(&mut self, _: &[u8], _: Deadline) -> Result<(), Error> {
            self.sent += 1;
            Ok(())
        }
        fn receive(&mut self, _: Deadline) -> Result<Message, Error> {
            self.replies.pop_front().ok_or(Error::Closed)
        }
        fn close(&mut self) {
            self.closed = true;
        }
    }
    fn reply(sequence: u32, frame: u32, status: u32, values: &[f32]) -> Message {
        let mut payload = Vec::new();
        for value in [frame, status, 0, 0, 0] {
            payload.extend(value.to_le_bytes());
        }
        for value in values {
            payload.extend(value.to_le_bytes());
        }
        Message {
            header: Header {
                kind: 9,
                sequence,
                flags: 0,
                length: u32::try_from(payload.len()).unwrap(),
                reserved: 0,
            },
            payload,
        }
    }
    fn client(replies: Vec<Message>) -> Client<WireFixture> {
        let mut client = Client::new(WireFixture {
            replies: replies.into(),
            sent: 0,
            closed: false,
        });
        client.ready = true;
        client
    }
    #[test]
    fn sequence_exhaustion_closes_before_sending() {
        // Given the final safe wire sequence has already been used.
        let mut client = client(vec![]);
        client.sequence = 0xffff_fffe;
        // When a new frame is requested, then no sequence can wrap and reuse an identity.
        assert!(client
            .infer(
                42,
                &vec![0; contract::WARPED_BYTES],
                &[0.0; 12],
                Deadline::after(Duration::from_millis(50)).unwrap(),
                false
            )
            .is_err());
        assert!(client.dead());
        assert_eq!(client.transport.sent, 0);
    }
    #[test]
    fn reset_explicitly_allows_frame_restart_and_stale_sequence_is_discarded() {
        // Given an earlier frame and an unrelated old reply ahead of this result.
        let values = vec![0.25; contract::OUTPUT_FLOATS];
        let mut client = client(vec![reply(99, 42, 0, &values), reply(1, 42, 0, &values)]);
        client.last_frame = Some(100);
        // When a reset is explicit, then only the new request's sequence can supply output.
        assert_eq!(
            client
                .infer(
                    42,
                    &vec![0; contract::WARPED_BYTES],
                    &[0.0; 12],
                    Deadline::after(Duration::from_millis(50)).unwrap(),
                    true
                )
                .unwrap(),
            values
        );
        assert_eq!(client.last_frame, Some(42));
    }
    #[test]
    fn nonfinite_input_is_rejected_before_usb_send() {
        // Given a nonfinite scalar that otherwise has valid dimensions.
        let mut client = client(vec![]);
        let mut packed = [0.0; 12];
        packed[9] = f32::NAN;
        // When inference crosses the wire boundary, then the session dies without transmitting it.
        assert!(client
            .infer(
                42,
                &vec![0; contract::WARPED_BYTES],
                &packed,
                Deadline::after(Duration::from_millis(50)).unwrap(),
                true
            )
            .is_err());
        assert!(client.dead());
        assert_eq!(client.transport.sent, 0);
    }
    #[test]
    fn invalid_outputs_close_without_advancing_last_frame() {
        for (frame, status, count, value) in [
            (41, 0, contract::OUTPUT_FLOATS, 0.0),
            (42, 4, contract::OUTPUT_FLOATS, 0.0),
            (42, 0, 10, 0.0),
            (42, 0, contract::OUTPUT_FLOATS, f32::INFINITY),
        ] {
            // Given a mismatched, failed, truncated, or nonfinite peer response.
            let mut client = client(vec![reply(1, frame, status, &vec![value; count])]);
            // When this frame completes, then none of those responses gains a valid frame identity.
            assert!(client
                .infer(
                    42,
                    &vec![0; contract::WARPED_BYTES],
                    &[0.0; 12],
                    Deadline::after(Duration::from_millis(50)).unwrap(),
                    true
                )
                .is_err());
            assert!(client.dead());
            assert_eq!(client.last_frame, None);
        }
    }
}
