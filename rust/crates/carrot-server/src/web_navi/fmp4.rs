use super::fmp4_io;
use ffmpeg_next::{self as av, Dictionary};
use std::collections::VecDeque;

pub struct Initialization {
    pub payload: Vec<u8>,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}
pub struct Segment {
    pub payload: Vec<u8>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub duration_ms: i64,
    pub keyframe: bool,
}
#[derive(Default)]
pub struct Output {
    pub initialization: Option<Initialization>,
    pub segments: Vec<Segment>,
}
pub struct Sample<'a> {
    pub payload: &'a [u8],
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub keyframe: bool,
}
pub(super) struct Pending {
    pub(super) sequence: u64,
    pub(super) timestamp_ms: u64,
    pub(super) duration_ms: i64,
    pub(super) keyframe: bool,
}
pub struct Muxer {
    pub(super) config: Vec<u8>,
    pub(super) dimensions: (u32, u32),
    session: String,
    output: Option<fmp4_io::Output>,
    pub(super) pending: VecDeque<Pending>,
    first: Option<u64>,
    last: Option<u64>,
    last_pts: i64,
    pub(super) initialized: bool,
    pub(super) fragment: Vec<u8>,
}
impl Default for Muxer {
    fn default() -> Self {
        Self {
            config: Vec::new(),
            dimensions: (0, 0),
            session: String::new(),
            output: None,
            pending: VecDeque::new(),
            first: None,
            last: None,
            last_pts: -1,
            initialized: false,
            fragment: Vec::new(),
        }
    }
}
impl Muxer {
    pub fn configure(&mut self, config: &[u8], dimensions: (u32, u32), session: &str) -> bool {
        let dimensions = (
            if dimensions.0 == 0 { 960 } else { dimensions.0 }.clamp(1, 65535),
            if dimensions.1 == 0 { 540 } else { dimensions.1 }.clamp(1, 65535),
        );
        if self.config == config && self.dimensions == dimensions && self.session == session {
            return false;
        }
        self.close();
        self.config = config.to_vec();
        self.dimensions = dimensions;
        self.session = session.into();
        true
    }
    fn start(&mut self, frame: &[u8]) -> Result<(), String> {
        if self.config.is_empty() {
            return Err("RuntimeError: H.264 configuration is unavailable".into());
        }
        av::init().map_err(|error| error.to_string())?;
        let bytes = [self.config.as_slice(), frame].concat();
        let parameters = fmp4_io::probe(bytes).map_err(|error| error.to_string())?;
        let mut output = fmp4_io::Output::new().map_err(|error| error.to_string())?;
        let mut stream = output
            .format
            .add_stream(av::encoder::find(av::codec::Id::None))
            .map_err(|error| error.to_string())?;
        stream.set_parameters(parameters);
        stream.set_time_base((1, 90_000));
        let mut options = Dictionary::new();
        options.set(
            "movflags",
            "frag_every_frame+empty_moov+default_base_moof+omit_tfhd_offset",
        );
        options.set("flush_packets", "1");
        output.header(options).map_err(|error| error.to_string())?;
        self.output = Some(output);
        Ok(())
    }
    fn timing(&mut self, timestamp: u64) -> Result<(i64, i64, i64), String> {
        let mut pts = match self.first {
            None => {
                self.first = Some(timestamp);
                0
            }
            Some(first) if timestamp > first => {
                i64::try_from((u128::from(timestamp - first) * 90_000) / 1000)
                    .map_err(|error| error.to_string())?
            }
            Some(_) => self
                .last_pts
                .checked_add(18_000)
                .ok_or("timestamp overflow")?,
        };
        if pts <= self.last_pts {
            pts = self.last_pts.checked_add(1).ok_or("timestamp overflow")?;
        }
        let duration = self
            .last
            .filter(|last| timestamp > *last)
            .map_or(18_000, |last| {
                i64::try_from((u128::from(timestamp - last) * 90_000) / 1000)
                    .unwrap_or(i64::MAX)
                    .clamp(900, 22_500)
            });
        self.last = Some(timestamp);
        self.last_pts = pts;
        Ok((pts, duration, (duration / 90).max(1)))
    }
    pub fn push(&mut self, sample: Sample<'_>) -> Result<Output, String> {
        if sample.payload.is_empty() {
            return Ok(Output::default());
        }
        if self.output.is_none() {
            if !sample.keyframe {
                return Ok(Output::default());
            }
            self.start(sample.payload)?;
        }
        let (pts, duration, duration_ms) = self.timing(sample.timestamp_ms)?;
        let mut packet = av::Packet::copy(sample.payload);
        packet.set_stream(0);
        packet.set_pts(Some(pts));
        packet.set_dts(Some(pts));
        packet.set_duration(duration);
        if sample.keyframe {
            packet.set_flags(av::packet::Flags::KEY);
        }
        self.pending.push_back(Pending {
            sequence: sample.sequence,
            timestamp_ms: sample.timestamp_ms,
            duration_ms,
            keyframe: sample.keyframe,
        });
        let Some(output) = &mut self.output else {
            return Err("muxer unavailable".into());
        };
        let result = packet
            .write_interleaved(&mut output.format)
            .and_then(|_| output.drain());
        match result {
            Ok(bytes) => self.consume(&bytes),
            Err(error) => {
                self.close();
                Err(error.to_string())
            }
        }
    }
    pub fn close(&mut self) {
        if let Some(mut output) = self.output.take() {
            let _ = output.format.write_trailer();
        }
        self.pending.clear();
        self.first = None;
        self.last = None;
        self.last_pts = -1;
        self.initialized = false;
        self.fragment.clear();
    }
    pub fn clear(&mut self) {
        self.close();
        self.config.clear();
        self.dimensions = (0, 0);
        self.session.clear();
    }
}
impl Drop for Muxer {
    fn drop(&mut self) {
        self.close();
    }
}
