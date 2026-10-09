use super::{Publishers, Session};
use crate::{cereal, Error};
use bytes::BytesMut;
use rtc::peer_connection::state::RTCPeerConnectionState;
use std::time::{Duration, Instant};

impl Session {
    pub fn state(&self) -> RTCPeerConnectionState {
        let Some(graph) = &self.graph else {
            return RTCPeerConnectionState::New;
        };
        let states: Vec<_> = self
            .peers
            .iter()
            .filter(|peer| !graph.owners[peer.index].stopped)
            .map(|peer| peer.state)
            .collect();
        if states.contains(&RTCPeerConnectionState::Failed) {
            RTCPeerConnectionState::Failed
        } else if states.iter().all(|state| {
            matches!(
                state,
                RTCPeerConnectionState::New | RTCPeerConnectionState::Closed
            )
        }) {
            RTCPeerConnectionState::New
        } else if states.contains(&RTCPeerConnectionState::Disconnected) {
            RTCPeerConnectionState::Disconnected
        } else if states.iter().any(|state| {
            matches!(
                state,
                RTCPeerConnectionState::New | RTCPeerConnectionState::Connecting
            )
        }) {
            RTCPeerConnectionState::Connecting
        } else {
            RTCPeerConnectionState::Connected
        }
    }

    pub fn reclaim(&mut self, now: Instant, prune: bool) -> bool {
        let state = self.state();
        if self.lifecycle.closed {
            return true;
        }
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            return self.lifecycle.ready
                || !self.lifecycle.connected_once
                || self.is_carrot() && prune;
        }
        if state == RTCPeerConnectionState::Disconnected {
            let since = *self.disconnected.get_or_insert(now);
            return (self.lifecycle.ready || self.is_carrot() && prune)
                && now.duration_since(since)
                    >= Duration::from_secs(if self.is_carrot() && prune { 3 } else { 8 });
        }
        self.disconnected = None;
        self.is_carrot()
            && prune
            && matches!(
                state,
                RTCPeerConnectionState::New | RTCPeerConnectionState::Connecting
            )
            && now.duration_since(self.created) >= Duration::from_secs(12)
    }

    pub fn drive(&mut self, publishers: &mut Publishers, buffer: &mut [u8]) -> Result<(), Error> {
        if self.lifecycle.closed {
            return Ok(());
        }
        if self.graph.is_none() {
            return Ok(());
        }
        while let Some(index) = self.activation.get(self.cursor) {
            let peer = self
                .peers
                .iter_mut()
                .find(|peer| peer.index == *index)
                .ok_or(Error::Contract("missing owner"))?;
            if !peer.gathered {
                self.cursor += 1;
                continue;
            }
            if !peer.active {
                peer.rtc.activate_transports(Instant::now())?;
                peer.active = true;
            }
            if peer.state != RTCPeerConnectionState::Connected {
                break;
            }
            self.cursor += 1;
        }
        let mut messages = Vec::new();
        for peer in &mut self.peers {
            messages.extend(peer.drive(buffer)?);
        }
        let channels = self
            .peers
            .iter_mut()
            .filter(|peer| peer.channel.id.is_some())
            .count();
        let state = self.state();
        if state == RTCPeerConnectionState::Connected {
            self.lifecycle.connected_once = true;
        }
        if !self.lifecycle.ready
            && state == RTCPeerConnectionState::Connected
            && self.expected_incoming == channels
            && self
                .peers
                .iter_mut()
                .all(|peer| peer.channel.id.is_none() || peer.channel.open(&mut peer.rtc))
        {
            if channels > 0 {
                publishers.add(&self.incoming)?;
            }
            self.lifecycle.ready = true;
        }
        if self.lifecycle.ready && !self.incoming.is_empty() {
            for message in messages {
                if let Err(error) = publishers.send(&message) {
                    eprintln!("WebRTC incoming proxy failure: {error}");
                }
            }
        }
        if self.lifecycle.ready
            && channels > 0
            && self.outgoing_alive
            && self.last_outgoing.elapsed() >= Duration::from_millis(10)
        {
            self.last_outgoing = Instant::now();
            if let Some(outgoing) = &mut self.outgoing {
                outgoing.update(Duration::ZERO)?;
                for topic in outgoing.state.topics().iter().filter(|topic| topic.updated) {
                    let encoded = cereal::outgoing_event(topic.event()?)?;
                    for peer in &mut self.peers {
                        if peer.channel.id.is_some()
                            && peer
                                .channel
                                .enqueue(&mut peer.rtc, BytesMut::from(encoded.as_bytes()), false)
                                .is_err()
                        {
                            self.outgoing_alive = false;
                        }
                    }
                }
            }
        }
        if self.lifecycle.ready {
            if let Some(compact) = &mut self.compact {
                if let Some(peer) = self.peers.iter_mut().find(|peer| peer.channel.id.is_some()) {
                    compact.update(peer)?;
                }
            }
        }
        Ok(())
    }

    pub fn notify(&mut self, text: &str) {
        for peer in &mut self.peers {
            if peer.channel.id.is_some() {
                if let Err(error) =
                    peer.channel
                        .enqueue(&mut peer.rtc, BytesMut::from(text.as_bytes()), true)
                {
                    eprintln!("WebRTC notify failed: {error}");
                }
            }
        }
    }

    pub async fn close(&mut self) {
        if self.lifecycle.closed {
            return;
        }
        self.lifecycle.closed = true;
        for peer in &mut self.peers {
            peer.close().await;
        }
        self.peers.clear();
        self.tracks.clear();
        self.outgoing = None;
        self.compact = None;
    }
}
