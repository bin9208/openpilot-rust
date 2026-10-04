use std::collections::VecDeque;

use openpilot_can::Packet;
use serde::{Deserialize, Serialize};

pub const MAX_AGE_NS: u64 = 100_000_000;
pub const MAX_CAN_PACKETS: usize = 512;
pub const MAX_STATE_PACKETS: usize = 32;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Ego {
    pub first_can_ns: u64,
    pub last_can_ns: u64,
    pub packet_count: u32,
    pub receive_ns: u64,
    pub v_ego: f64,
    pub a_ego: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    StateOverflow,
    MissingBatchMetadata,
    StaleEgoState,
    InvalidEmptyBatch,
    InvalidBatchMetadata,
    MissingCanPacket,
}

pub struct Batch {
    pub ego: Ego,
    pub packets: Vec<Packet>,
    pub error: Option<Reason>,
}

#[derive(Default)]
pub struct Batches {
    pub can: VecDeque<Packet>,
    pub states: VecDeque<Ego>,
    pub overflowed: bool,
}

impl Batches {
    pub fn add_can(&mut self, packets: impl IntoIterator<Item = Packet>) {
        for packet in packets {
            if self.can.len() == MAX_CAN_PACKETS {
                self.can.pop_front();
            }
            self.can.push_back(packet);
        }
    }

    pub fn add_state(&mut self, state: Ego) {
        if self.states.len() >= MAX_STATE_PACKETS {
            self.states.clear();
            self.overflowed = true;
        }
        self.states.push_back(state);
    }

    fn reject(&mut self, ego: Ego, error: Option<Reason>) -> Batch {
        self.states.pop_front();
        Batch {
            ego,
            packets: Vec::new(),
            error,
        }
    }

    pub fn take(&mut self, now_ns: u64) -> Option<Batch> {
        let ego = *self.states.front()?;
        if self.overflowed {
            self.overflowed = false;
            return Some(self.reject(ego, Some(Reason::StateOverflow)));
        }
        if ego.receive_ns == 0 {
            return Some(self.reject(ego, Some(Reason::MissingBatchMetadata)));
        }
        if i128::from(now_ns) - i128::from(ego.receive_ns) > i128::from(MAX_AGE_NS) {
            return Some(self.reject(ego, Some(Reason::StaleEgoState)));
        }
        if ego.packet_count == 0 {
            let error = (ego.first_can_ns != 0 || ego.last_can_ns != 0)
                .then_some(Reason::InvalidEmptyBatch);
            return Some(self.reject(ego, error));
        }
        if ego.packet_count as usize > MAX_CAN_PACKETS
            || ego.first_can_ns == 0
            || ego.last_can_ns < ego.first_can_ns
        {
            return Some(self.reject(ego, Some(Reason::InvalidBatchMetadata)));
        }
        while self
            .can
            .front()
            .is_some_and(|p| p.mono_time < ego.first_can_ns)
        {
            self.can.pop_front();
        }
        if self
            .can
            .back()
            .is_none_or(|p| p.mono_time < ego.last_can_ns)
        {
            return None;
        }
        let mut packets = Vec::new();
        while self
            .can
            .front()
            .is_some_and(|p| p.mono_time <= ego.last_can_ns)
        {
            packets.push(self.can.pop_front().expect("front was checked"));
        }
        self.states.pop_front();
        if packets.len() != ego.packet_count as usize
            || packets
                .first()
                .is_none_or(|p| p.mono_time != ego.first_can_ns)
            || packets
                .last()
                .is_none_or(|p| p.mono_time != ego.last_can_ns)
        {
            Some(Batch {
                ego,
                packets: Vec::new(),
                error: Some(Reason::MissingCanPacket),
            })
        } else {
            Some(Batch {
                ego,
                packets,
                error: None,
            })
        }
    }
}
