use super::{Error, Sender};
use bytes::BytesMut;
use rtc::{
    peer_connection::{
        message::{RTCMessage, TaggedRTCMessage},
        RTCPeerConnection,
    },
    rtcp::payload_feedbacks::{
        full_intra_request::FullIntraRequest, picture_loss_indication::PictureLossIndication,
        receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    },
    rtcp::{transport_feedbacks::transport_layer_nack::TransportLayerNack, Packet},
    rtp::Packet as RtpPacket,
    sansio::Protocol,
};

fn rtx(packet: &mut RtpPacket, payload_type: u8, ssrc: u32, sequence: u16) {
    let mut payload = BytesMut::with_capacity(2 + packet.payload.len());
    payload.extend_from_slice(&packet.header.sequence_number.to_be_bytes());
    payload.extend_from_slice(&packet.payload);
    packet.payload = payload.freeze();
    packet.header.payload_type = payload_type;
    packet.header.ssrc = ssrc;
    packet.header.sequence_number = sequence;
}

impl Sender {
    pub fn feedback(
        &mut self,
        peer: &mut RTCPeerConnection,
        packets: &[Box<dyn Packet>],
    ) -> Result<(), Error> {
        for packet in packets {
            if packet
                .as_any()
                .downcast_ref::<PictureLossIndication>()
                .is_some_and(|pli| pli.media_ssrc == self.ssrc)
                || packet
                    .as_any()
                    .downcast_ref::<FullIntraRequest>()
                    .is_some_and(|fir| fir.media_ssrc == self.ssrc)
            {
                self.track.keyframe();
            }
            if let Some(remb) = packet
                .as_any()
                .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
            {
                if remb.ssrcs.contains(&self.ssrc) {
                    let bitrate = num_traits::ToPrimitive::to_u32(&remb.bitrate.min(3_000_000.0))
                        .ok_or(Error::Contract("invalid REMB bitrate"))?;
                    self.track.bitrate(bitrate);
                }
            }
            let Some(nack) = packet.as_any().downcast_ref::<TransportLayerNack>() else {
                continue;
            };
            if nack.media_ssrc != self.ssrc {
                continue;
            }
            for sequence in nack.nacks.iter().flat_map(|pair| pair.into_iter()) {
                let Some(mut packet) = self.control.retransmission(sequence) else {
                    continue;
                };
                let mut sender = peer
                    .rtp_sender(self.id)
                    .ok_or(Error::Contract("camera sender disappeared"))?;
                let parameters = sender.get_parameters();
                let codec = parameters.rtp_parameters.codecs.iter().find(|codec| {
                    codec.rtp_codec.mime_type.eq_ignore_ascii_case("video/rtx")
                        && codec
                            .rtp_codec
                            .sdp_fmtp_line
                            .split(';')
                            .filter_map(|parameter| parameter.trim().strip_prefix("apt="))
                            .any(|apt| {
                                apt.parse::<u8>()
                                    .is_ok_and(|apt| apt == packet.header.payload_type)
                            })
                });
                let ssrc = parameters
                    .encodings
                    .first()
                    .and_then(|encoding| encoding.rtp_coding_parameters.rtx.as_ref())
                    .map(|rtx| rtx.ssrc);
                if let (Some(codec), Some(ssrc)) = (codec, ssrc) {
                    if packet.header.extensions.iter().any(|extension| {
                        !parameters
                            .rtp_parameters
                            .header_extensions
                            .iter()
                            .any(|negotiated| u8::try_from(negotiated.id) == Ok(extension.id))
                    }) {
                        return Err(Error::Contract("cached RTX extension is not negotiated"));
                    }
                    rtx(&mut packet, codec.payload_type, ssrc, self.rtx_sequence);
                    self.rtx_sequence = self.rtx_sequence.wrapping_add(1);
                    let track = sender.track().track_id().clone();
                    // The provider's write_rtp guard recognizes primary SSRCs only.
                    peer.handle_write(TaggedRTCMessage {
                        now: std::time::Instant::now(),
                        message: RTCMessage::RtpPacket(track, packet),
                    })?;
                } else {
                    sender.write_rtp(std::time::Instant::now(), packet)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::rtx;
    use rtc::rtp::{header::Header, Packet};

    #[test]
    fn rtx_wrap_preserves_original_sequence_and_header_metadata() {
        let mut packet = Packet {
            header: Header {
                sequence_number: 65535,
                ssrc: 1,
                timestamp: 240,
                marker: true,
                ..Default::default()
            },
            payload: bytes::Bytes::from_static(b"NAL"),
        };
        rtx(&mut packet, 100, 2, 0);
        assert_eq!(
            (
                packet.header.sequence_number,
                packet.header.ssrc,
                packet.header.payload_type
            ),
            (0, 2, 100)
        );
        assert_eq!(packet.header.timestamp, 240);
        assert!(packet.header.marker);
        assert_eq!(packet.payload.as_ref(), b"\xff\xffNAL");
    }
}
