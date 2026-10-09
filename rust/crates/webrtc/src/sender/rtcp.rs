use crate::Error;
use bytes::BytesMut;
use rtc::{
    rtcp::{goodbye::Goodbye, raw_packet::RawPacket, sender_report::SenderReport, Packet},
    rtp::Packet as RtpPacket,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) fn ntp_time() -> Result<u64, Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Contract("NTP clock predates epoch"))?;
    Ok(((now.as_secs() + 2_208_988_800) << 32)
        + ((u64::from(now.subsec_nanos()) << 32) / 1_000_000_000))
}

fn interval() -> Duration {
    let random = uuid::Uuid::new_v4();
    let bytes = random.as_bytes();
    let high = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let low = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) >> 11;
    let fraction = (f64::from(high) * 2_097_152.0 + f64::from(low)) / 9_007_199_254_740_992.0;
    Duration::from_secs_f64(0.5 + fraction)
}

fn sdes(ssrc: u32, cname: &str) -> Result<RawPacket, Error> {
    let mut payload = BytesMut::new();
    payload.extend_from_slice(&ssrc.to_be_bytes());
    payload.extend_from_slice(&[1, u8::try_from(cname.len())?]);
    payload.extend_from_slice(cname.as_bytes());
    payload.extend_from_slice(&[0, 0]);
    while !payload.len().is_multiple_of(4) {
        payload.extend_from_slice(&[0]);
    }
    let mut packet = BytesMut::from(&[0x81, 202][..]);
    packet.extend_from_slice(&u16::try_from(payload.len() / 4)?.to_be_bytes());
    packet.extend_from_slice(&payload);
    Ok(RawPacket(packet.freeze()))
}

pub(super) struct Control {
    pub cname: String,
    ssrc: u32,
    history: Vec<Option<RtpPacket>>,
    packets: u32,
    octets: u32,
    ntp: u64,
    next_report: Option<Instant>,
}

impl Control {
    pub(super) fn new(ssrc: u32) -> Self {
        Self {
            cname: String::new(),
            ssrc,
            history: vec![None; 128],
            packets: 0,
            octets: 0,
            ntp: 0,
            next_report: None,
        }
    }

    pub(super) fn remember(&mut self, packet: &RtpPacket) {
        self.history[usize::from(packet.header.sequence_number) % 128] = Some(packet.clone());
    }

    pub(super) fn retransmission(&self, sequence: u16) -> Option<RtpPacket> {
        self.history[usize::from(sequence) % 128]
            .as_ref()
            .filter(|packet| packet.header.sequence_number == sequence)
            .cloned()
    }

    pub(super) fn accepted(&mut self, length: u32, ntp: u64) {
        self.ntp = ntp;
        self.packets = self.packets.wrapping_add(1);
        self.octets = self.octets.wrapping_add(length);
    }

    pub(super) fn report(
        &mut self,
        now: Instant,
        timestamp: u32,
    ) -> Result<Option<Vec<Box<dyn Packet>>>, Error> {
        let deadline = self.next_report.get_or_insert_with(|| now + interval());
        if now < *deadline {
            return Ok(None);
        }
        *deadline = now + interval();
        Ok(Some(vec![
            Box::new(SenderReport {
                ssrc: self.ssrc,
                ntp_time: self.ntp,
                rtp_time: timestamp,
                packet_count: self.packets,
                octet_count: self.octets,
                ..Default::default()
            }),
            Box::new(sdes(self.ssrc, &self.cname)?),
        ]))
    }

    pub(super) fn goodbye(&self) -> Option<Vec<Box<dyn Packet>>> {
        self.next_report.map(|_| {
            vec![Box::new(Goodbye {
                sources: vec![self.ssrc],
                ..Default::default()
            }) as Box<dyn Packet>]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Control;
    use rtc::{
        rtcp::sender_report::SenderReport,
        rtp::{header::Header, Packet},
    };
    use std::time::{Duration, Instant};

    #[test]
    fn history_has128_slots_and_retransmission_does_not_change_counts() {
        let mut control = Control::new(240);
        let packet = Packet {
            header: Header {
                sequence_number: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        control.remember(&packet);
        assert!(control.retransmission(1).is_some());
        let mut replacement = packet;
        replacement.header.sequence_number = 129;
        control.remember(&replacement);
        assert!(control.retransmission(1).is_none());
        assert!(control.retransmission(129).is_some());
        control.accepted(123, 456);
        let now = Instant::now();
        assert!(matches!(control.report(now, 789), Ok(None)));
        let Ok(Some(reports)) = control.report(now + Duration::from_secs(2), 789) else {
            panic!("report deadline was not reached")
        };
        let Some(report) = reports[0].as_any().downcast_ref::<SenderReport>() else {
            panic!("first report was not SR")
        };
        assert_eq!(
            (
                report.ssrc,
                report.packet_count,
                report.octet_count,
                report.ntp_time,
                report.rtp_time
            ),
            (240, 1, 123, 456, 789)
        );
    }
}
