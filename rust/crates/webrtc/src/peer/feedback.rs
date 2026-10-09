use rtc::{
    interceptor::{Attribute, Interceptor, Packet, StreamInfo, TaggedPacket},
    sansio::Protocol,
};
use std::{collections::VecDeque, time::Instant};

#[derive(Default)]
pub(super) struct Feedback {
    reads: VecDeque<TaggedPacket>,
    writes: VecDeque<TaggedPacket>,
}

impl Protocol<TaggedPacket, TaggedPacket, ()> for Feedback {
    type Rout = TaggedPacket;
    type Wout = TaggedPacket;
    type Eout = ();
    type Error = rtc::shared::error::Error;
    type Time = Instant;

    fn handle_read(&mut self, mut packet: TaggedPacket) -> Result<(), Self::Error> {
        if matches!(packet.message.packet, Packet::Rtcp(_)) {
            packet.message.add(Attribute::DeliverToApplication);
        }
        self.reads.push_back(packet);
        Ok(())
    }

    fn poll_read(&mut self) -> Option<Self::Rout> {
        self.reads.pop_front()
    }

    fn handle_write(&mut self, packet: TaggedPacket) -> Result<(), Self::Error> {
        self.writes.push_back(packet);
        Ok(())
    }

    fn poll_write(&mut self) -> Option<Self::Wout> {
        self.writes.pop_front()
    }
}

impl Interceptor for Feedback {
    fn bind_local_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_local_stream(&mut self, _info: &StreamInfo) {}
    fn bind_remote_stream(&mut self, _info: &StreamInfo) {}
    fn unbind_remote_stream(&mut self, _info: &StreamInfo) {}
}
