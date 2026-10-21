#![allow(unsafe_code)]
use crate::{config::Codec, native::Mapping, Error};
use openpilot_cereal::log_capnp::{encode_index, event};
use openpilot_msgq::{Publisher, VisionMetadata};

pub struct VideoPublisher {
    transport: Publisher,
    service: &'static str,
    dimensions: (u32, u32),
    kind: encode_index::Type,
    count: i32,
}

#[derive(Clone, Copy)]
pub struct VideoPacket<'a> {
    pub segment: i32,
    pub index: u32,
    pub extra: &'a VisionMetadata,
    pub flags: u32,
    pub header: &'a [u8],
}

enum Payload<'a> {
    Slice(&'a [u8]),
    Mapping(&'a Mapping, usize),
}

impl Payload<'_> {
    fn length(&self) -> usize {
        match self {
            Self::Slice(bytes) => bytes.len(),
            Self::Mapping(_, length) => *length,
        }
    }

    fn copy_into(self, destination: &mut [u8]) -> Result<(), Error> {
        match self {
            Self::Slice(bytes) => destination.copy_from_slice(bytes),
            Self::Mapping(mapping, _) => mapping.copy_into(destination)?,
        }
        Ok(())
    }
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
        self.publish_payload(
            VideoPacket {
                segment,
                index,
                extra,
                flags,
                header,
            },
            Payload::Slice(data),
        )
    }

    pub fn publish_mapping(
        &mut self,
        packet: VideoPacket<'_>,
        mapping: &Mapping,
        length: usize,
    ) -> Result<(), Error> {
        if length > mapping.length() {
            return Err(Error::Contract("DMA copy exceeds mapping"));
        }
        self.publish_payload(packet, Payload::Mapping(mapping, length))
    }

    fn publish_payload(
        &mut self,
        packet: VideoPacket<'_>,
        payload: Payload<'_>,
    ) -> Result<(), Error> {
        let bytes = self.build_payload(packet, payload, nanos)?;
        self.transport.send(&bytes)?;
        Ok(())
    }

    fn build_payload(
        &mut self,
        packet: VideoPacket<'_>,
        payload: Payload<'_>,
        mut clock: impl FnMut(rustix::time::ClockId) -> Result<u64, Error>,
    ) -> Result<Vec<u8>, Error> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(clock(rustix::time::ClockId::Boottime)?);
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
        video.set_unix_timestamp_nanos(clock(rustix::time::ClockId::Realtime)?);
        let mut idx = video.reborrow().init_idx();
        idx.set_frame_id(packet.extra.frame_id);
        idx.set_timestamp_sof(packet.extra.timestamp_sof);
        idx.set_timestamp_eof(packet.extra.timestamp_eof);
        idx.set_type(self.kind);
        idx.set_encode_id(self.count as u32);
        self.count = self.count.wrapping_add(1);
        idx.set_segment_num(packet.segment);
        idx.set_segment_id(packet.index);
        idx.set_flags(packet.flags);
        let length = u32::try_from(payload.length())?;
        idx.set_len(length);
        payload.copy_into(video.reborrow().init_data(length))?;
        video.set_width(self.dimensions.0);
        video.set_height(self.dimensions.1);
        if packet.flags & 8 != 0 {
            video.set_header(packet.header);
        }
        Ok(capnp::serialize::write_message_to_words(&message))
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

#[cfg(test)]
mod direct_packet_tests {
    use super::*;
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
        fs::File,
        hint::black_box,
        io::Write,
        os::fd::{FromRawFd, OwnedFd},
        sync::atomic::{AtomicU64, Ordering},
        time::Instant,
    };

    thread_local! {
        static TRACK: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }
    struct Counter;
    fn record_allocation() {
        let _ = TRACK.try_with(|active| {
            if active.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }
    // SAFETY: System receives the same allocation arguments; TLS only counts
    // allocations on the current test thread while measurement is active.
    unsafe impl GlobalAlloc for Counter {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            record_allocation();
            unsafe { System.realloc(pointer, layout, size) }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Counter = Counter;
    static NEXT_ENDPOINT: AtomicU64 = AtomicU64::new(0);

    fn fixture_mapping(bytes: &[u8]) -> Mapping {
        // SAFETY: memfd_create returns one new descriptor owned by this test.
        let raw = unsafe { libc::memfd_create(c"v4l-direct-packet".as_ptr(), libc::MFD_CLOEXEC) };
        assert!(raw >= 0);
        // SAFETY: raw is a fresh descriptor transferred exactly once into File.
        let mut file = unsafe { File::from_raw_fd(raw) };
        file.set_len(bytes.len() as u64).unwrap();
        file.write_all(bytes).unwrap();
        let fd: OwnedFd = file.into();
        // SAFETY: the retained file is initialized and never truncated.
        unsafe { Mapping::new(fd, bytes.len()) }.unwrap()
    }

    fn publisher() -> VideoPublisher {
        let endpoint = format!(
            "v4l-direct-packet-448-{}-{}",
            std::process::id(),
            NEXT_ENDPOINT.fetch_add(1, Ordering::Relaxed)
        );
        VideoPublisher {
            transport: Publisher::new(&endpoint).unwrap(),
            service: "roadEncodeData",
            dimensions: (1920, 1080),
            kind: encode_index::Type::FullHEVC,
            count: 0,
        }
    }

    fn metadata() -> VisionMetadata {
        VisionMetadata {
            width: 1920,
            height: 1080,
            stride: 1920,
            uv_offset: 1920 * 1080,
            len: 4096,
            frame_id: 419,
            timestamp_sof: 1_000_000_000,
            timestamp_eof: 1_020_000_000,
            valid: true,
            received: true,
            index: 0,
            fd: -1,
        }
    }

    // Independently retain the previous set_data wire construction. This
    // mirrors the original C++ VideoEncoder::publisher_publish field roster.
    fn prior_wire(packet: VideoPacket<'_>, data: &[u8], encode_id: u32) -> Vec<u8> {
        let mut message = capnp::message::Builder::new_default();
        let mut event = message.init_root::<event::Builder<'_>>();
        event.set_log_mono_time(11);
        event.set_valid(true);
        let mut video = event.init_road_encode_data();
        video.set_unix_timestamp_nanos(22);
        let mut idx = video.reborrow().init_idx();
        idx.set_frame_id(packet.extra.frame_id);
        idx.set_timestamp_sof(packet.extra.timestamp_sof);
        idx.set_timestamp_eof(packet.extra.timestamp_eof);
        idx.set_type(encode_index::Type::FullHEVC);
        idx.set_encode_id(encode_id);
        idx.set_segment_num(packet.segment);
        idx.set_segment_id(packet.index);
        idx.set_flags(packet.flags);
        idx.set_len(data.len() as u32);
        video.set_data(data);
        video.set_width(1920);
        video.set_height(1080);
        if packet.flags & 8 != 0 {
            video.set_header(packet.header);
        }
        capnp::serialize::write_message_to_words(&message)
    }

    fn fixed_clocks() -> impl FnMut(rustix::time::ClockId) -> Result<u64, Error> {
        let mut calls = 0;
        move |clock| {
            calls += 1;
            match calls {
                1 => {
                    assert!(matches!(clock, rustix::time::ClockId::Boottime));
                    Ok(11)
                }
                2 => {
                    assert!(matches!(clock, rustix::time::ClockId::Realtime));
                    Ok(22)
                }
                _ => panic!("unexpected encoder clock call"),
            }
        }
    }

    #[test]
    fn direct_dma_builder_matches_prior_complete_wire_for_variable_packets() {
        let source: Vec<u8> = (0..4096)
            .map(|index| (index as u8).wrapping_mul(37))
            .collect();
        let mapping = fixture_mapping(&source);
        let extra = metadata();
        let mut publisher = publisher();
        for (index, length) in [0, 1, 65, 4096, 7, 1024, 0, 33].into_iter().enumerate() {
            let flags = if index == 3 { 8 } else { 0 };
            let packet = VideoPacket {
                segment: 2,
                index: index as u32,
                extra: &extra,
                flags,
                header: b"codec-header",
            };
            let expected = prior_wire(packet, &source[..length], index as u32);
            let actual = publisher
                .build_payload(packet, Payload::Mapping(&mapping, length), fixed_clocks())
                .unwrap();
            assert_eq!(
                actual, expected,
                "complete Cap'n Proto wire at length {length}"
            );
        }
        assert_eq!(publisher.count, 8);
        let packet = VideoPacket {
            segment: 2,
            index: 9,
            extra: &extra,
            flags: 0,
            header: &[],
        };
        assert!(matches!(
            publisher.publish_mapping(packet, &mapping, 4097),
            Err(Error::Contract("DMA copy exceeds mapping"))
        ));
        assert_eq!(
            publisher.count, 8,
            "failed bounds must not consume encode ID"
        );
    }

    #[test]
    fn all_encoder_services_and_codec_types_share_slice_and_dma_wire() {
        let source: Vec<u8> = (0..65).map(|index| index as u8).collect();
        let mapping = fixture_mapping(&source);
        let extra = metadata();
        let packet = VideoPacket {
            segment: 4,
            index: 8,
            extra: &extra,
            flags: 8,
            header: b"codec-header",
        };
        let mut publisher = publisher();
        for service in [
            "roadEncodeData",
            "driverEncodeData",
            "wideRoadEncodeData",
            "qRoadEncodeData",
            "livestreamRoadEncodeData",
            "livestreamDriverEncodeData",
            "livestreamWideRoadEncodeData",
            "youtubeRoadEncodeData",
        ] {
            publisher.service = service;
            for kind in [
                encode_index::Type::FullHEVC,
                encode_index::Type::BigBoxLossless,
                encode_index::Type::QcameraH264,
            ] {
                publisher.kind = kind;
                publisher.count = 0;
                let slice = publisher
                    .build_payload(packet, Payload::Slice(&source), fixed_clocks())
                    .unwrap();
                publisher.count = 0;
                let dma = publisher
                    .build_payload(
                        packet,
                        Payload::Mapping(&mapping, source.len()),
                        fixed_clocks(),
                    )
                    .unwrap();
                assert_eq!(slice, dma, "{service} {kind:?}");
            }
        }
    }

    #[test]
    fn direct_dma_builder_reports_actual_warmed_copy_and_publication_cost() {
        let source: Vec<u8> = (0..4096).map(|index| index as u8).collect();
        let mapping = fixture_mapping(&source);
        let extra = metadata();
        let packet = VideoPacket {
            segment: 1,
            index: 1,
            extra: &extra,
            flags: 0,
            header: &[],
        };
        let mut publisher = publisher();
        let mut scratch = vec![0; source.len()];
        let lengths = [8, 1024, 4096, 7];
        for length in lengths {
            mapping.copy_into(&mut scratch[..length]).unwrap();
            publisher.count = 0;
            let old_wire = publisher
                .build_payload(packet, Payload::Slice(&scratch[..length]), fixed_clocks())
                .unwrap();
            publisher.transport.send(black_box(&old_wire)).unwrap();
            publisher.count = 0;
            let new_wire = publisher
                .build_payload(packet, Payload::Mapping(&mapping, length), fixed_clocks())
                .unwrap();
            publisher.transport.send(black_box(&new_wire)).unwrap();
        }
        let mut old_allocations = 0;
        let mut new_allocations = 0;
        let mut old_elapsed_ns = 0;
        let mut new_elapsed_ns = 0;
        let mut old_accounted_copies = 0;
        let mut new_accounted_copies = 0;
        for _ in 0..20 {
            for length in lengths {
                publisher.count = 0;
                ALLOCATIONS.with(|count| count.set(0));
                let started = Instant::now();
                TRACK.with(|active| active.set(true));
                mapping.copy_into(&mut scratch[..length]).unwrap();
                let old_wire = publisher
                    .build_payload(packet, Payload::Slice(&scratch[..length]), fixed_clocks())
                    .unwrap();
                publisher.transport.send(black_box(&old_wire)).unwrap();
                TRACK.with(|active| active.set(false));
                old_elapsed_ns += started.elapsed().as_nanos();
                old_allocations += ALLOCATIONS.with(Cell::get);
                old_accounted_copies += 2 * length; // DMA to scratch, then slice to message data.

                publisher.count = 0;
                ALLOCATIONS.with(|count| count.set(0));
                let started = Instant::now();
                TRACK.with(|active| active.set(true));
                let new_wire = publisher
                    .build_payload(packet, Payload::Mapping(&mapping, length), fixed_clocks())
                    .unwrap();
                publisher.transport.send(black_box(&new_wire)).unwrap();
                TRACK.with(|active| active.set(false));
                new_elapsed_ns += started.elapsed().as_nanos();
                new_allocations += ALLOCATIONS.with(Cell::get);
                new_accounted_copies += length; // DMA to owned Cap'n Proto data.
            }
        }
        assert_eq!(old_accounted_copies, 2 * new_accounted_copies);
        println!(
            "{{\"scope\":\"selected DMA and Cap'n Proto payload copies; serialization and queue copies remain\",\"old_accounted_copy_bytes\":{old_accounted_copies},\"new_accounted_copy_bytes\":{new_accounted_copies},\"old_allocations\":{old_allocations},\"new_allocations\":{new_allocations},\"old_elapsed_ns_sample\":{old_elapsed_ns},\"new_elapsed_ns_sample\":{new_elapsed_ns}}}"
        );
    }
}
