use crate::{
    media::{Packet, VideoSpec, VideoWriter},
    writer::Logger,
    Error,
};
use openpilot_cereal::log_capnp::{encode_data, event};
use openpilot_params::Params;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Road,
    Wide,
    Driver,
    Qroad,
}

impl Stream {
    pub fn from_service(name: &str) -> Option<Self> {
        match name {
            "roadEncodeData" => Some(Self::Road),
            "wideRoadEncodeData" => Some(Self::Wide),
            "driverEncodeData" => Some(Self::Driver),
            "qRoadEncodeData" => Some(Self::Qroad),
            _ => None,
        }
    }

    fn data(self, event: event::Reader<'_>) -> Result<encode_data::Reader<'_>, Error> {
        match (self, event.which()?) {
            (Self::Road, event::RoadEncodeData(data))
            | (Self::Wide, event::WideRoadEncodeData(data))
            | (Self::Driver, event::DriverEncodeData(data))
            | (Self::Qroad, event::QRoadEncodeData(data)) => Ok(data?),
            _ => Err(Error::Invalid("encoder event does not match service")),
        }
    }

    fn filename(self) -> &'static str {
        match self {
            Self::Road => "fcamera.hevc",
            Self::Wide => "ecamera.hevc",
            Self::Driver => "dcamera.hevc",
            Self::Qroad => "qcamera.ts",
        }
    }
}

pub struct Encoder {
    stream: Stream,
    pub writer: Option<VideoWriter>,
    offset: Option<i64>,
    current_segment: i32,
    queue: Vec<Vec<u8>>,
    recording: bool,
    marked_ready: bool,
    pub audio_initialized: bool,
    pub include_audio: bool,
    record: bool,
}

impl Encoder {
    pub fn new(stream: Stream, params: &Params) -> Result<Self, Error> {
        let road = params.get("RecordRoadCam")?.unwrap_or_default();
        let road = String::from_utf8_lossy(&road);
        let road = if road.is_empty() {
            0
        } else {
            let value = road.trim_start_matches(|character: char| character.is_ascii_whitespace());
            let sign = usize::from(value.starts_with(['+', '-']));
            let end = sign + value[sign..].bytes().take_while(u8::is_ascii_digit).count();
            value[..end]
                .parse::<i32>()
                .map_err(|_| Error::Invalid("invalid RecordRoadCam integer"))?
        };
        let record = match stream {
            Stream::Road => road > 0,
            Stream::Wide => road > 1,
            Stream::Driver => params.get_bool("RecordFront")?,
            Stream::Qroad => true,
        };
        Ok(Self {
            stream,
            writer: None,
            offset: None,
            current_segment: -1,
            queue: Vec::new(),
            recording: false,
            marked_ready: false,
            audio_initialized: false,
            include_audio: stream == Stream::Qroad && params.get_bool("RecordAudio")?,
            record,
        })
    }

    pub fn handle(&mut self, logger: &mut Logger, bytes: Vec<u8>) -> Result<bool, Error> {
        let message = capnp::serialize::read_message_from_flat_slice(
            &mut bytes.as_slice(),
            Default::default(),
        )?;
        let event = message.get_root::<event::Reader>()?;
        let data = self.stream.data(event)?;
        let index = data.get_idx()?;
        let offset = *self
            .offset
            .get_or_insert(i64::from(index.get_segment_num()));
        let part = i64::from(index.get_segment_num()) - offset;
        match part.cmp(&i64::from(logger.part)) {
            std::cmp::Ordering::Equal => {
                if self.current_segment != logger.part {
                    if self.record {
                        if let Some(writer) = self.writer.take() {
                            writer.close()?;
                        }
                        self.writer = Some(VideoWriter::open(
                            &logger.path()?.join(self.stream.filename()),
                            VideoSpec {
                                width: data.get_width(),
                                height: data.get_height(),
                                fps: 20,
                                codec: index.get_type()?,
                            },
                        )?);
                        self.recording = false;
                        self.audio_initialized = false;
                    }
                    self.current_segment = logger.part;
                    self.marked_ready = false;
                }
                if self.audio_initialized || !self.include_audio {
                    for queued in std::mem::take(&mut self.queue) {
                        self.write(logger, &queued)?;
                    }
                    self.write(logger, &bytes)?;
                } else {
                    self.enqueue(bytes);
                }
                Ok(false)
            }
            std::cmp::Ordering::Greater => {
                let newly_ready = !self.marked_ready;
                self.marked_ready = true;
                self.enqueue(bytes);
                Ok(newly_ready)
            }
            std::cmp::Ordering::Less => {
                self.offset = Some(-i64::from(logger.part));
                eprintln!(
                    "loggerd: old encoder segment; reset {:?} offset",
                    self.stream
                );
                Ok(false)
            }
        }
    }

    fn enqueue(&mut self, bytes: Vec<u8>) {
        if self.queue.len() > 200 {
            eprintln!(
                "loggerd: encoder queue full for {:?}; dropping packet",
                self.stream
            );
        } else {
            self.queue.push(bytes);
        }
    }

    fn write(&mut self, logger: &mut Logger, bytes: &[u8]) -> Result<(), Error> {
        let message =
            capnp::serialize::read_message_from_flat_slice(&mut &*bytes, Default::default())?;
        let event = message.get_root::<event::Reader>()?;
        let data = self.stream.data(event)?;
        let index = data.get_idx()?;
        let keyframe = index.get_flags() & 8 != 0;
        let timestamp_us = i64::try_from(index.get_timestamp_eof() / 1000)?;
        if !self.recording {
            if !keyframe {
                return Ok(());
            }
            if let Some(writer) = &mut self.writer {
                writer.write(Packet {
                    data: data.get_header()?,
                    timestamp_us,
                    configuration: true,
                    keyframe: false,
                })?;
            }
            self.recording = true;
        }
        if let Some(writer) = &mut self.writer {
            writer.write(Packet {
                data: data.get_data()?,
                timestamp_us,
                configuration: false,
                keyframe,
            })?;
        }
        let mut message = capnp::message::Builder::new_default();
        let mut output = message.init_root::<event::Builder>();
        output.set_valid(event.get_valid());
        output.set_log_mono_time(event.get_log_mono_time());
        match self.stream {
            Stream::Road => output.set_road_encode_idx(index)?,
            Stream::Wide => output.set_wide_road_encode_idx(index)?,
            Stream::Driver => output.set_driver_encode_idx(index)?,
            Stream::Qroad => output.set_q_road_encode_idx(index)?,
        }
        logger.write(&capnp::serialize::write_message_to_words(&message), true)
    }

    pub fn close(&mut self) -> Result<(), Error> {
        if let Some(writer) = self.writer.take() {
            writer.close()?;
        }
        Ok(())
    }
}
