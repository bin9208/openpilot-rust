mod frames;
use frames::{frame, segmented};
use openpilot_can::Frame;
use openpilot_card::{
    firmware_query::StartupIo,
    isotp::Error,
    query::{QueryIo, Target},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize)]
pub struct Reply {
    pub target: Target,
    pub bus: u8,
    pub offset: i64,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
}
#[derive(Default, Deserialize)]
pub struct Input {
    pub replies: Vec<Reply>,
    pub clock_step: f64,
}
#[derive(Serialize)]
pub struct Io {
    #[serde(skip)]
    replies: Vec<Reply>,
    #[serde(skip)]
    pending: Vec<Frame>,
    #[serde(skip)]
    assembly: BTreeMap<(u32, u8), (Target, usize, Vec<u8>)>,
    pub sent: Vec<Frame>,
    pub delays: Vec<f64>,
    pub receives: Vec<bool>,
    pub obd: Vec<bool>,
    pub now: f64,
    pub clock_reads: usize,
    #[serde(skip)]
    clock_step: f64,
}
impl From<Input> for Io {
    fn from(input: Input) -> Self {
        Self {
            replies: input.replies,
            pending: Vec::new(),
            assembly: BTreeMap::new(),
            sent: Vec::new(),
            delays: Vec::new(),
            receives: Vec::new(),
            obd: Vec::new(),
            now: 0.,
            clock_reads: 0,
            clock_step: input.clock_step,
        }
    }
}
impl Io {
    fn respond(&mut self, request: &Frame) -> Result<(), Error> {
        if let Some((target, length, payload)) =
            self.assembly.get_mut(&(request.address, request.bus))
        {
            let offset = usize::from(target.1.is_some());
            if request
                .data
                .get(offset)
                .is_some_and(|header| header >> 4 == 2)
            {
                payload.extend_from_slice(&request.data[offset + 1..]);
                if payload.len() >= *length {
                    let target = *target;
                    let payload = payload[..*length].to_vec();
                    self.assembly.remove(&(request.address, request.bus));
                    self.respond_payload(target, request.bus, &payload)?;
                }
                return Ok(());
            }
        }
        for reply in &self.replies {
            if reply.target.0 != request.address || reply.bus != request.bus {
                continue;
            }
            let offset = usize::from(reply.target.1.is_some());
            if reply
                .target
                .1
                .is_some_and(|sub| request.data.first() != Some(&sub))
            {
                continue;
            }
            let Some(header) = request.data.get(offset) else {
                continue;
            };
            match header >> 4 {
                0 => {
                    let length = usize::from(header & 15);
                    let Some(payload) = request.data.get(offset + 1..offset + 1 + length) else {
                        continue;
                    };
                    if payload == reply.request {
                        let target = reply.target;
                        let payload = payload.to_vec();
                        self.respond_payload(target, request.bus, &payload)?;
                        return Ok(());
                    }
                }
                1 => {
                    let Some(low) = request.data.get(offset + 1) else {
                        continue;
                    };
                    let length = usize::from(header & 15) * 256 + usize::from(*low);
                    let payload = &request.data[offset + 2..];
                    if reply.request.len() == length && reply.request.starts_with(payload) {
                        self.assembly.insert(
                            (request.address, request.bus),
                            (reply.target, length, payload.to_vec()),
                        );
                        self.pending.push(frame(reply, &[0x30, 0, 0])?);
                        return Ok(());
                    }
                }
                2..=15 => (),
                _ => return Err(Error::FrameType(*header >> 4)),
            }
        }
        Ok(())
    }
    fn respond_payload(&mut self, target: Target, bus: u8, payload: &[u8]) -> Result<(), Error> {
        let mut emitted = Vec::new();
        for reply in self
            .replies
            .iter()
            .filter(|reply| reply.target == target && reply.bus == bus && reply.request == payload)
        {
            for frame in segmented(reply)? {
                let key = (frame.address, frame.bus, frame.data.clone());
                if !emitted.contains(&key) {
                    emitted.push(key);
                    self.pending.push(frame);
                }
            }
        }
        Ok(())
    }
}
impl QueryIo for Io {
    fn receive(&mut self, wait_for_one: bool) -> Result<Vec<Vec<Frame>>, Error> {
        self.receives.push(wait_for_one);
        if wait_for_one {
            self.now += self.clock_step;
        }
        let packet = std::mem::take(&mut self.pending);
        Ok(if packet.is_empty() {
            vec![]
        } else {
            vec![packet]
        })
    }
    fn send(&mut self, frames: &[Frame]) -> Result<(), Error> {
        for frame in frames {
            self.sent.push(frame.clone());
            self.respond(frame)?;
        }
        Ok(())
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        self.delays.push(seconds);
        Ok(())
    }
    fn now(&mut self) -> f64 {
        self.clock_reads += 1;
        self.now
    }
}
impl StartupIo for Io {
    fn set_obd_multiplexing(&mut self, enabled: bool) -> Result<(), Error> {
        self.obd.push(enabled);
        Ok(())
    }
}
