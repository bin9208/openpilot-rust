mod answer;
mod feedback;
mod metadata;
mod owner;
pub(crate) use answer::prepare;

use crate::{Error, channel::Channel, sender::Sender};
use bytes::BytesMut;
use rtc::{
    peer_connection::{
        RTCPeerConnection,
        event::{RTCDataChannelEvent, RTCPeerConnectionEvent},
        message::RTCMessage,
        state::RTCPeerConnectionState,
    },
    sansio::Protocol,
    shared::{TaggedBytesMut, TransportContext, TransportProtocol},
};
use std::{
    collections::VecDeque,
    net::{SocketAddr, UdpSocket},
    time::Instant,
};

pub(crate) struct Peer {
    cname: String,
    mdns: Option<crate::network::Lease>,
    pub index: usize,
    pub rtc: RTCPeerConnection,
    pub sockets: Vec<UdpSocket>,
    pub state: RTCPeerConnectionState,
    pub active: bool,
    pub gathered: bool,
    pub senders: Vec<Sender>,
    pub channel: Channel,
    queued: VecDeque<(BytesMut, SocketAddr, SocketAddr)>,
}

impl Peer {
    fn socket(&self, address: SocketAddr) -> Result<&UdpSocket, Error> {
        self.sockets
            .iter()
            .find(|socket| socket.local_addr().is_ok_and(|local| local == address))
            .ok_or(Error::Contract("datagram local candidate has no socket"))
    }

    fn flush(&mut self) -> Result<(), Error> {
        while let Some((bytes, local, recipient)) = self.queued.front() {
            match self.socket(*local)?.send_to(bytes, recipient) {
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    break;
                }
                Err(error) => eprintln!("WebRTC datagram send failed: {error}"),
            }
            self.queued.pop_front();
        }
        Ok(())
    }

    pub fn transmit(&mut self) -> Result<(), Error> {
        self.flush()?;
        while let Some(packet) = self.rtc.poll_write() {
            if self.queued.is_empty() {
                match self
                    .socket(packet.transport.local_addr)?
                    .send_to(&packet.message, packet.transport.peer_addr)
                {
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        self.queued.push_back((
                            packet.message.clone(),
                            packet.transport.local_addr,
                            packet.transport.peer_addr,
                        ));
                    }
                    Err(error) => eprintln!("WebRTC datagram send failed: {error}"),
                }
            } else {
                self.queued.push_back((
                    packet.message.clone(),
                    packet.transport.local_addr,
                    packet.transport.peer_addr,
                ));
            }
            for sender in &mut self.senders {
                sender.accepted(&packet.message)?;
            }
        }
        Ok(())
    }

    pub fn drive(&mut self, buffer: &mut [u8]) -> Result<Vec<BytesMut>, Error> {
        self.channel.flush(&mut self.rtc);
        self.transmit()?;
        for socket in &self.sockets {
            loop {
                match socket.recv_from(buffer) {
                    Ok((length, peer_addr)) => self.rtc.handle_read(TaggedBytesMut {
                        now: Instant::now(),
                        transport: TransportContext {
                            local_addr: socket.local_addr()?,
                            peer_addr,
                            ecn: None,
                            transport_protocol: TransportProtocol::UDP,
                        },
                        message: BytesMut::from(&buffer[..length]),
                    })?,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if self
            .rtc
            .poll_timeout()
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            self.rtc.handle_timeout(Instant::now())?;
        }
        while let Some(event) = self.rtc.poll_event() {
            match event {
                RTCPeerConnectionEvent::OnConnectionStateChangeEvent(state) => self.state = state,
                RTCPeerConnectionEvent::OnDataChannel(RTCDataChannelEvent::OnOpen(id)) => {
                    if self.channel.id.is_none()
                        && self
                            .rtc
                            .data_channel(id)
                            .is_some_and(|channel| channel.label() == "data")
                    {
                        self.channel.id = Some(id);
                    }
                }
                _ => {}
            }
        }
        let messages = self.messages()?;
        if self.state == RTCPeerConnectionState::Connected {
            for sender in &mut self.senders {
                if sender.active {
                    sender.report(&mut self.rtc)?;
                }
                if !sender.active || !sender.ready_for_recv() {
                    continue;
                }
                if let Some(bytes) = sender.sync_packet() {
                    if self.channel.buffered <= 4096 && self.channel.open(&mut self.rtc) {
                        if let Err(error) = self.channel.enqueue(&mut self.rtc, bytes, false) {
                            eprintln!("WebRTC frame sync failed: {error}");
                        }
                    }
                }
                match sender.receive(&mut self.rtc) {
                    Ok(Some(frame)) => {
                        if let Err(error) = sender.frame(&mut self.rtc, frame) {
                            sender.stopped = true;
                            eprintln!("WebRTC video sender stopped: {error}");
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if sender.track.is_debug() {
                            sender.stopped = true;
                        }
                        eprintln!("WebRTC could not build video packet: {error}");
                    }
                }
            }
        }
        Ok(messages)
    }

    fn messages(&mut self) -> Result<Vec<BytesMut>, Error> {
        let mut messages = Vec::new();
        while let Some(packet) = self.rtc.poll_read() {
            match packet.message {
                RTCMessage::DataChannelMessage(id, message) if self.channel.id == Some(id) => {
                    messages.push(message.data);
                }
                RTCMessage::RtcpPacket(_, packets) => {
                    for sender in &mut self.senders {
                        sender.feedback(&mut self.rtc, &packets)?;
                    }
                }
                _ => {}
            }
        }
        Ok(messages)
    }

    async fn drain(&mut self) {
        if let Err(error) = self.transmit() {
            eprintln!("WebRTC final transport output failed: {error}");
        }
        while !self.queued.is_empty() {
            if let Err(error) = self.flush() {
                eprintln!("WebRTC final queued output failed: {error}");
                break;
            }
            if !self.queued.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
    }

    pub async fn close(&mut self) {
        for sender in &mut self.senders {
            if let Err(error) = sender.goodbye(&mut self.rtc) {
                eprintln!("WebRTC RTCP goodbye failed: {error}");
            }
        }
        self.drain().await;
        if let Err(error) = self.rtc.prepare_dtls_shutdown(Instant::now()) {
            eprintln!("WebRTC DTLS shutdown failed: {error}");
        }
        self.drain().await;
        if let Err(error) = self.rtc.close() {
            eprintln!("WebRTC peer close failed: {error}");
        }
        self.sockets.clear();
        self.senders.clear();
        self.mdns = None;
    }
}
