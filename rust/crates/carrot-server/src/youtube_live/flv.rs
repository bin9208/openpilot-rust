use super::{
    aac::{Audio, RATE, SAMPLES},
    h264,
};
use crate::Error;
use std::io::Write;

pub struct Muxer<W: Write> {
    output: W,
    audio: Audio,
    fps: u32,
    packet_index: u64,
    audio_pts: i64,
    last_video_ms: i64,
    closed: bool,
}
impl<W: Write> Muxer<W> {
    pub fn new(mut output: W, header: &[u8], fps: u32) -> Result<Self, Error> {
        if header.is_empty() {
            return Err(Error::Source("H.264 codec header is required".into()));
        }
        let video_config = h264::configuration(header)?;
        let audio = Audio::new()?;
        output.write_all(b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00")?;
        let mut muxer = Self {
            output,
            audio,
            fps: fps.max(1),
            packet_index: 0,
            audio_pts: 0,
            last_video_ms: 0,
            closed: false,
        };
        let mut video = vec![0x17, 0, 0, 0, 0];
        video.extend(video_config);
        muxer.tag(9, 0, &video)?;
        let mut audio = vec![0xaf, 0];
        audio.extend_from_slice(&muxer.audio.configuration);
        muxer.tag(8, 0, &audio)?;
        Ok(muxer)
    }
    pub fn mux(
        &mut self,
        payload: &[u8],
        keyframe: bool,
        timestamp_ms: Option<i64>,
    ) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Source("FLV muxer is closed".into()));
        }
        if payload.is_empty() {
            return Ok(());
        }
        let access = h264::normalize(payload)?;
        let idr = access.is_idr();
        if self.packet_index == 0 && !idr {
            return Err(Error::Source(
                "first H.264 access unit is not an IDR frame".into(),
            ));
        }
        if self.packet_index > 0 && keyframe && !idr {
            return Err(Error::Source(
                "H.264 frame marked as keyframe has no IDR NAL".into(),
            ));
        }
        let mut video_ms = timestamp_ms
            .unwrap_or_else(|| {
                i64::try_from(u128::from(self.packet_index) * 1000 / u128::from(self.fps))
                    .unwrap_or(i64::MAX)
            })
            .max(0);
        if self.packet_index > 0 && video_ms <= self.last_video_ms {
            video_ms = self.last_video_ms.saturating_add(1);
        }
        self.last_video_ms = video_ms;
        self.silence(
            i64::try_from(i128::from(video_ms) * i128::from(RATE) / 1000)
                .map_err(|_| Error::Source("audio timestamp overflow".into()))?,
        )?;
        let mut data = vec![if idr { 0x17 } else { 0x27 }, 1, 0, 0, 0];
        data.extend(access.avcc);
        self.tag(9, video_ms, &data)?;
        self.packet_index = self.packet_index.saturating_add(1);
        Ok(())
    }
    fn silence(&mut self, target: i64) -> Result<(), Error> {
        while self.audio_pts <= target {
            let packets = self.audio.silence(self.audio_pts)?;
            for packet in packets {
                self.audio_tag(packet)?;
            }
            self.audio_pts = self
                .audio_pts
                .saturating_add(i64::try_from(SAMPLES).unwrap_or(0));
        }
        Ok(())
    }
    fn audio_tag(&mut self, packet: super::aac::Packet) -> Result<(), Error> {
        let mut audio = vec![0xaf, 1];
        audio.extend(packet.payload);
        self.tag(8, packet.timestamp_ms, &audio)
    }
    fn tag(&mut self, kind: u8, timestamp: i64, payload: &[u8]) -> Result<(), Error> {
        let timestamp = u32::try_from(i128::from(timestamp.max(0)) & i128::from(u32::MAX))
            .map_err(|_| Error::Source("invalid timestamp".into()))?;
        let size = u32::try_from(payload.len())
            .map_err(|_| Error::Source("int too big to convert".into()))?;
        if size > 0xff_ffff {
            return Err(Error::Source("int too big to convert".into()));
        }
        let mut tag = vec![kind];
        tag.extend_from_slice(&size.to_be_bytes()[1..]);
        tag.extend_from_slice(&timestamp.to_be_bytes()[1..]);
        tag.push(timestamp.to_be_bytes()[0]);
        tag.extend([0, 0, 0]);
        tag.extend(payload);
        tag.extend_from_slice(&size.saturating_add(11).to_be_bytes());
        self.output.write_all(&tag)?;
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), Error> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let result = self
            .silence(
                i64::try_from(i128::from(self.last_video_ms) * i128::from(RATE) / 1000)
                    .map_err(|_| Error::Source("audio timestamp overflow".into()))?,
            )
            .and_then(|()| self.audio.finish())
            .and_then(|packets| {
                for packet in packets {
                    self.audio_tag(packet)?;
                }
                Ok(())
            });
        self.output.flush()?;
        result
    }
    pub fn output(&self) -> &W {
        &self.output
    }
}
