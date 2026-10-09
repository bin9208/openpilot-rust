use crate::{
    video::{
        pack,
        track::{Frame, Payload, Track},
    },
    Error,
};
use bytes::{Bytes, BytesMut};
use rtc::{
    peer_connection::RTCPeerConnection,
    rtp::{header::Header, Packet},
    rtp_transceiver::RTCRtpSenderId,
};
use std::{collections::VecDeque, time::Instant};

mod feedback;
mod rtcp;
use rtcp::{ntp_time, Control};

const MID_EXTENSION: &str = "urn:ietf:params:rtp-hdrext:sdes:mid";
const ABS_SEND_EXTENSION: &str = "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time";

pub(crate) struct Sender {
    pub id: RTCRtpSenderId,
    pub track: Track,
    pub stopped: bool,
    pub active: bool,
    pub sync_enabled: bool,
    pub last_source: Option<u32>,
    pub last_sync: Option<u32>,
    pub timestamp: u32,
    control: Control,
    origin: u32,
    sequence: u16,
    rtx_sequence: u16,
    ssrc: u32,
    payload_type: Option<u8>,
    mid: String,
    pending: VecDeque<(u16, u32)>,
    pending_timestamp: Option<u32>,
}

impl Sender {
    pub fn new(
        id: RTCRtpSenderId,
        track: Track,
        mid: &str,
        ssrc: u32,
        sync_enabled: bool,
    ) -> Result<Self, Error> {
        let random = uuid::Uuid::new_v4();
        let bytes = random.as_bytes();
        Ok(Self {
            id,
            track,
            stopped: false,
            active: false,
            sync_enabled,
            last_source: None,
            last_sync: None,
            timestamp: 0,
            control: Control::new(ssrc),
            origin: u32::from_be_bytes(bytes[0..4].try_into()?),
            sequence: u16::from_be_bytes(bytes[4..6].try_into()?) & 0x7fff,
            rtx_sequence: u16::from_be_bytes(bytes[8..10].try_into()?) & 0x7fff,
            ssrc,
            payload_type: None,
            mid: mid.to_owned(),
            pending: VecDeque::new(),
            pending_timestamp: None,
        })
    }

    pub fn ready_for_recv(&self) -> bool {
        self.pending.is_empty() && !self.stopped
    }

    pub fn sync_packet(&mut self) -> Option<BytesMut> {
        if !self.sync_enabled || !self.pending.is_empty() {
            return None;
        }
        let frame = self.last_source?;
        if self.last_sync == Some(frame) {
            return None;
        }
        self.last_sync = Some(frame);
        let mut bytes = BytesMut::from(&b"CVF1"[..]);
        bytes.extend_from_slice(&frame.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        Some(bytes)
    }

    pub fn accepted(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() < 12 || bytes[0] >> 6 != 2 || self.payload_type != Some(bytes[1] & 0x7f) {
            return Ok(());
        }
        let ssrc = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let timestamp = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let sequence = u16::from_be_bytes([bytes[2], bytes[3]]);
        if ssrc == self.ssrc
            && self.pending_timestamp == Some(timestamp)
            && self
                .pending
                .front()
                .is_some_and(|(pending, _)| *pending == sequence)
        {
            self.timestamp = timestamp;
            if let Some((_, length)) = self.pending.pop_front() {
                self.control.accepted(length, ntp_time()?);
            }
        }
        Ok(())
    }

    pub fn frame(&mut self, peer: &mut RTCPeerConnection, frame: Frame) -> Result<(), Error> {
        self.last_source = frame.source_id;
        let payloads = match frame.payload {
            Payload::H264(data) => {
                pack(&data).map_err(|_| Error::Contract("invalid H264 NAL unit"))?
            }
            Payload::Encoded(payloads) => payloads,
        };
        let timestamp = self
            .origin
            .wrapping_add(u32::try_from(frame.pts.rem_euclid(1_i64 << 32))?);
        let count = payloads.len();
        self.pending_timestamp = Some(timestamp);
        let mut sender = peer
            .rtp_sender(self.id)
            .ok_or(Error::Contract("camera sender disappeared"))?;
        let parameters = sender.get_parameters().clone();
        let payload_type = parameters
            .rtp_parameters
            .codecs
            .first()
            .ok_or(Error::Contract("missing negotiated codec"))?
            .payload_type;
        self.payload_type = Some(payload_type);
        for (index, payload) in payloads.into_iter().enumerate() {
            let ntp = ntp_time()?;
            let mut header = Header {
                version: 2,
                payload_type,
                ssrc: self.ssrc,
                timestamp,
                sequence_number: self.sequence,
                marker: index + 1 == count,
                ..Default::default()
            };
            for uri in [MID_EXTENSION, ABS_SEND_EXTENSION] {
                let Some(extension) = parameters
                    .rtp_parameters
                    .header_extensions
                    .iter()
                    .find(|extension| extension.uri == uri)
                else {
                    continue;
                };
                let bytes = if uri == MID_EXTENSION {
                    Bytes::copy_from_slice(self.mid.as_bytes())
                } else {
                    Bytes::copy_from_slice(
                        &u32::try_from((ntp >> 14) & 0x00ff_ffff)?.to_be_bytes()[1..],
                    )
                };
                header.set_extension(u8::try_from(extension.id)?, bytes)?;
            }
            let length = u32::try_from(payload.len())?;
            let packet = Packet {
                header,
                payload: Bytes::from(payload),
            };
            self.control.remember(&packet);
            sender.write_rtp(Instant::now(), packet)?;
            self.pending.push_back((self.sequence, length));
            self.sequence = self.sequence.wrapping_add(1);
        }
        Ok(())
    }

    pub fn set_cname(&mut self, cname: &str) {
        self.control.cname.clear();
        self.control.cname.push_str(cname);
    }

    pub fn receive(&mut self, peer: &mut RTCPeerConnection) -> Result<Option<Frame>, Error> {
        let mut sender = peer
            .rtp_sender(self.id)
            .ok_or(Error::Contract("camera sender disappeared"))?;
        let codec = sender
            .get_parameters()
            .rtp_parameters
            .codecs
            .first()
            .ok_or(Error::Contract("missing negotiated codec"))?;
        self.track.receive(&codec.rtp_codec.mime_type)
    }

    pub fn report(&mut self, peer: &mut RTCPeerConnection) -> Result<(), Error> {
        if let Some(packets) = self.control.report(Instant::now(), self.timestamp)? {
            peer.rtp_sender(self.id)
                .ok_or(Error::Contract("camera sender disappeared"))?
                .write_rtcp(Instant::now(), packets)?;
        }
        Ok(())
    }

    pub fn goodbye(&mut self, peer: &mut RTCPeerConnection) -> Result<(), Error> {
        if let Some(packets) = self.control.goodbye() {
            peer.rtp_sender(self.id)
                .ok_or(Error::Contract("camera sender disappeared"))?
                .write_rtcp(Instant::now(), packets)?;
        }
        Ok(())
    }
}
