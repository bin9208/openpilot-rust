use crate::Error;
use bytes::BytesMut;
use rtc::{
    data_channel::{RTCDataChannelId, RTCDataChannelState},
    peer_connection::RTCPeerConnection,
};
use std::{collections::VecDeque, time::Instant};

#[derive(Default)]
pub(crate) struct Channel {
    pub id: Option<RTCDataChannelId>,
    pub buffered: usize,
    queued: VecDeque<(BytesMut, bool)>,
}

impl Channel {
    pub fn open(&self, peer: &mut RTCPeerConnection) -> bool {
        self.id
            .and_then(|id| peer.data_channel(id))
            .is_some_and(|channel| channel.ready_state() == RTCDataChannelState::Open)
    }

    pub fn enqueue(
        &mut self,
        peer: &mut RTCPeerConnection,
        bytes: BytesMut,
        text: bool,
    ) -> Result<(), Error> {
        if !self.open(peer) {
            return Err(Error::Contract("data channel is not open"));
        }
        self.buffered = self.buffered.saturating_add(bytes.len());
        self.queued.push_back((bytes, text));
        Ok(())
    }

    pub fn flush(&mut self, peer: &mut RTCPeerConnection) {
        let Some(id) = self.id else { return };
        while let Some((bytes, text)) = self.queued.pop_front() {
            self.buffered = self.buffered.saturating_sub(bytes.len());
            if let Some(mut channel) = peer.data_channel(id) {
                let result = if text {
                    match std::str::from_utf8(&bytes) {
                        Ok(text) => channel.send_text(Instant::now(), text),
                        Err(error) => {
                            eprintln!("WebRTC notify UTF-8 failed: {error}");
                            continue;
                        }
                    }
                } else {
                    channel.send(Instant::now(), bytes)
                };
                if let Err(error) = result {
                    eprintln!("WebRTC outgoing data failed: {error}");
                }
            }
        }
    }
}
