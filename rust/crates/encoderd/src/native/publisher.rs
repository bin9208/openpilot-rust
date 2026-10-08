#![allow(unsafe_code)]
use crate::{config::Codec, Error};
use openpilot_cereal::log_capnp::{encode_index, event};
use openpilot_msgq::{Publisher, VisionMetadata};

pub struct VideoPublisher {
    transport: Publisher,
    service: &'static str,
    dimensions: (u32, u32),
    kind: encode_index::Type,
    count: i32,
}

// SAFETY: the native Publisher owns only a mmap queue and FD lock, with no
// thread-local state. This encoder-private wrapper transfers exclusive ownership
// into the dequeue worker and back after join; it never permits concurrent use.
unsafe impl Send for VideoPublisher {}

impl VideoPublisher {
    pub fn new(service: &'static str, dimensions: (i32, i32), kind: Codec) -> Result<Self, Error> {
        Ok(Self {
            transport: transport(service)?,
            service,
            dimensions: (u32::try_from(dimensions.0)?, u32::try_from(dimensions.1)?),
            kind: match kind {
                Codec::FullHevc => encode_index::Type::FullHEVC,
                Codec::BigBoxLossless => encode_index::Type::BigBoxLossless,
                Codec::QcameraH264 => encode_index::Type::QcameraH264,
            },
            count: 0,
        })
    }
    pub fn publish(
        &mut self,
        segment: i32,
        index: u32,
        extra: &VisionMetadata,
        flags: u32,
        header: &[u8],
        data: &[u8],
    ) -> Result<(), Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(nanos(rustix::time::ClockId::Boottime)?);
        event.set_valid(true);
        let mut video = match self.service {
            "roadEncodeData" => event.init_road_encode_data(),
            "driverEncodeData" => event.init_driver_encode_data(),
            "wideRoadEncodeData" => event.init_wide_road_encode_data(),
            "qRoadEncodeData" => event.init_q_road_encode_data(),
            "livestreamRoadEncodeData" => event.init_livestream_road_encode_data(),
            "livestreamDriverEncodeData" => event.init_livestream_driver_encode_data(),
            "livestreamWideRoadEncodeData" => event.init_livestream_wide_road_encode_data(),
            "youtubeRoadEncodeData" => event.init_youtube_road_encode_data(),
            _ => return Err(Error::Contract("unknown source encoder service")),
        };
        video.set_unix_timestamp_nanos(nanos(rustix::time::ClockId::Realtime)?);
        let mut idx = video.reborrow().init_idx();
        idx.set_frame_id(extra.frame_id);
        idx.set_timestamp_sof(extra.timestamp_sof);
        idx.set_timestamp_eof(extra.timestamp_eof);
        idx.set_type(self.kind);
        idx.set_encode_id(self.count as u32);
        self.count = self.count.wrapping_add(1);
        idx.set_segment_num(segment);
        idx.set_segment_id(index);
        idx.set_flags(flags);
        idx.set_len(u32::try_from(data.len())?);
        video.set_data(data);
        video.set_width(self.dimensions.0);
        video.set_height(self.dimensions.1);
        if flags & 8 != 0 {
            video.set_header(header);
        }
        self.transport
            .send(&capnp::serialize::write_message_to_words(&message))?;
        Ok(())
    }
}

pub fn transport(service: &str) -> Result<Publisher, Error> {
    let info = openpilot_messaging::services::SERVICES
        .iter()
        .find(|info| info.name == service)
        .ok_or(Error::Contract(
            "encoder service absent from source catalog",
        ))?;
    Ok(Publisher::for_runtime(service, info.queue_size)?)
}

pub fn thumbnail(
    publisher: &mut Publisher,
    extra: &VisionMetadata,
    bytes: &[u8],
) -> Result<(), Error> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(nanos(rustix::time::ClockId::Boottime)?);
    event.set_valid(true);
    let mut thumbnail = event.init_thumbnail();
    thumbnail.set_frame_id(extra.frame_id);
    thumbnail.set_timestamp_eof(extra.timestamp_eof);
    thumbnail.set_thumbnail(bytes);
    publisher.send(&capnp::serialize::write_message_to_words(&message))?;
    Ok(())
}

pub fn nanos(clock: rustix::time::ClockId) -> Result<u64, Error> {
    let time = rustix::time::clock_gettime(clock);
    Ok(u64::try_from(time.tv_sec)?
        .wrapping_mul(1_000_000_000)
        .wrapping_add(u64::try_from(time.tv_nsec)?))
}
