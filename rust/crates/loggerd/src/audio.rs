use crate::diagnostics;
use crate::Error;
use ffmpeg_next::{self as av, format::context::Output};
use openpilot_logging::{log_site, record::Level};

pub struct Audio {
    encoder: av::codec::encoder::audio::Encoder,
    buffer: Vec<f32>,
    pts: i64,
    stream: usize,
}

impl Audio {
    pub fn new(output: &mut Output, sample_rate: u32) -> Result<Self, Error> {
        if sample_rate == 0 {
            return Err(Error::Invalid("zero audio sample rate"));
        }
        let codec = av::encoder::find(av::codec::Id::AAC).ok_or(av::Error::EncoderNotFound)?;
        let mut encoder = av::codec::Context::new_with_codec(codec)
            .encoder()
            .audio()?;
        encoder.set_format(av::format::Sample::F32(av::format::sample::Type::Planar));
        encoder.set_rate(i32::try_from(sample_rate)?);
        encoder.set_channel_layout(av::ChannelLayout::MONO);
        encoder.set_bit_rate(32_000);
        encoder.set_flags(av::codec::Flags::GLOBAL_HEADER);
        encoder.set_time_base((1, i32::try_from(sample_rate)?));
        let encoder = encoder.open_as(codec)?;
        av::log::set_level(av::log::Level::Warning);
        let mut stream = output.add_stream(codec)?;
        stream.set_parameters(&encoder);
        let stream = stream.index();
        Ok(Self {
            encoder,
            buffer: Vec::new(),
            pts: 0,
            stream,
        })
    }

    pub fn push(&mut self, bytes: &[u8], timestamp_us: u64, sample_rate: u32) -> Result<(), Error> {
        if self.pts == 0 {
            self.pts = i64::try_from(
                timestamp_us
                    .checked_mul(u64::from(self.encoder.rate()))
                    .ok_or(Error::Invalid("audio timestamp overflow"))?
                    / 1_000_000,
            )?;
        }
        let sample_count = bytes.len() / 2;
        let maximum = usize::try_from(sample_rate)?
            .checked_mul(10)
            .ok_or(Error::Invalid("audio queue overflow"))?;
        let combined = self
            .buffer
            .len()
            .checked_add(sample_count)
            .ok_or(Error::Invalid("audio queue overflow"))?;
        let discard = combined.saturating_sub(maximum);
        if discard > self.buffer.len() {
            return Err(Error::Invalid("audio packet exceeds ten seconds"));
        }
        if discard > 0 {
            self.buffer.drain(..discard);
            self.pts = self
                .pts
                .checked_add(i64::try_from(discard)?)
                .ok_or(Error::Invalid("audio timestamp overflow"))?;
            diagnostics::emit(
                log_site!(),
                Level::Error,
                format!("Audio buffer overflow, dropping {discard} oldest samples"),
            );
        }
        self.buffer.extend(
            bytes.chunks_exact(2).map(|sample| {
                f32::from(i16::from_le_bytes([sample[0], sample[1]])) * (1.0 / 32768.0)
            }),
        );
        Ok(())
    }

    pub fn drain(&mut self, output: &mut Output) -> Result<(), Error> {
        let count = usize::try_from(self.encoder.frame_size())?;
        if count == 0 {
            return Err(Error::Invalid("AAC encoder returned zero frame size"));
        }
        while self.buffer.len() >= count {
            self.encode_frame(output, count)?;
        }
        Ok(())
    }

    fn encode_frame(&mut self, output: &mut Output, count: usize) -> Result<(), Error> {
        let mut frame =
            av::frame::Audio::new(self.encoder.format(), count, av::ChannelLayout::MONO);
        frame.set_rate(self.encoder.rate());
        frame.set_pts(Some(self.pts));
        frame.plane_mut::<f32>(0)[..count].copy_from_slice(&self.buffer[..count]);
        self.buffer.drain(..count);
        if let Err(error) = self.encoder.send_frame(&frame) {
            diagnostics::emit(
                log_site!(),
                Level::Warning,
                format!(
                    "AUDIO: Failed to send audio frame to encoder: {}",
                    i32::from(error)
                ),
            );
        } else {
            self.receive(output);
        }
        self.pts = self
            .pts
            .checked_add(i64::try_from(count)?)
            .ok_or(Error::Invalid("audio timestamp overflow"))?;
        Ok(())
    }

    fn receive(&mut self, output: &mut Output) {
        let mut packet = av::Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            if let Some(stream) = output.stream(self.stream) {
                packet.rescale_ts(self.encoder.time_base(), stream.time_base());
                packet.set_stream(self.stream);
                if let Err(error) = packet.write_interleaved(output) {
                    diagnostics::emit(
                        log_site!(),
                        Level::Warning,
                        format!("AUDIO: Write frame failed - error: {}", i32::from(error)),
                    );
                }
            }
        }
    }

    pub fn finish(&mut self, output: &mut Output) -> Result<(), Error> {
        self.drain(output)?;
        let frame_size = usize::try_from(self.encoder.frame_size())?;
        if !self.buffer.is_empty() && self.buffer.len() < frame_size {
            self.buffer.resize(frame_size, 0.0);
            self.encode_frame(output, frame_size)?;
        }
        if let Err(error) = self.encoder.send_eof() {
            diagnostics::emit(
                log_site!(),
                Level::Warning,
                format!(
                    "AUDIO: Failed to send audio frame to encoder: {}",
                    i32::from(error)
                ),
            );
        } else {
            self.receive(output);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use av::Rescale;
    use std::path::Path;

    fn decoded_samples(path: &Path) -> Result<(usize, f32), av::Error> {
        let mut input = av::format::input(path)?;
        let stream = input
            .streams()
            .best(av::media::Type::Audio)
            .ok_or(av::Error::StreamNotFound)?;
        let index = stream.index();
        let time_base = stream.time_base();
        let context = av::codec::Context::from_parameters(stream.parameters())?;
        let mut decoder = context.decoder().audio()?;
        decoder.set_packet_time_base(time_base);
        let mut count = 0;
        let mut peak = 0.0_f32;
        let mut drain = |decoder: &mut av::decoder::Audio| -> Result<(), av::Error> {
            let mut frame = av::frame::Audio::empty();
            loop {
                match decoder.receive_frame(&mut frame) {
                    Ok(()) => {
                        // Do not let AAC's priming frame mask a lost input tail.
                        let pts = frame.pts().ok_or(av::Error::InvalidData)?;
                        if pts.rescale(time_base, (1, 16000)) < 80_000 {
                            continue;
                        }
                        count += frame.samples();
                        for sample in &frame.plane::<f32>(0)[..frame.samples()] {
                            peak = peak.max(sample.abs());
                        }
                    }
                    Err(av::Error::Eof) => return Ok(()),
                    Err(av::Error::Other { errno }) if errno == libc::EAGAIN => return Ok(()),
                    Err(error) => return Err(error),
                }
            }
        };
        for (stream, packet) in input.packets() {
            if stream.index() == index {
                decoder.send_packet(&packet)?;
                drain(&mut decoder)?;
            }
        }
        decoder.send_eof()?;
        drain(&mut decoder)?;
        Ok((count, peak))
    }

    fn finish_buffered_audio(samples: usize) {
        av::init().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("buffered-audio.mka");
        let mut output = av::format::output(&path).unwrap();
        let mut audio = Audio::new(&mut output, 16000).unwrap();
        assert_eq!(audio.encoder.frame_size(), 1024, "AAC fixture frame size");
        let pcm: Vec<u8> = (0..samples)
            .flat_map(|index| {
                let value: i16 = if index % 32 < 16 { 4096 } else { -4096 };
                value.to_le_bytes()
            })
            .collect();
        // This is the real logger's initial ordering: audio arrives before the
        // first video header. No later audio call drains it before shutdown.
        audio.push(&pcm, 5_000_000, 16000).unwrap();
        output.write_header().unwrap();
        audio.finish(&mut output).unwrap();
        output.write_trailer().unwrap();
        drop(output);
        let decoded = decoded_samples(&path);
        assert!(
            decoded.is_ok(),
            "{samples} buffered samples produced no decodable audio: {decoded:?}"
        );
        let (count, peak) = decoded.unwrap();
        assert!(
            count >= samples,
            "shutdown lost audio: supplied {samples}, decoded {count}"
        );
        assert!(
            peak > 0.01,
            "shutdown replaced the buffered signal with silence"
        );
    }

    #[test]
    fn shutdown_encodes_buffer_smaller_than_one_aac_frame() {
        finish_buffered_audio(1023);
    }

    #[test]
    fn shutdown_encodes_exactly_one_buffered_aac_frame() {
        finish_buffered_audio(1024);
    }

    #[test]
    fn shutdown_encodes_one_buffered_aac_frame_and_tail() {
        finish_buffered_audio(1025);
    }

    #[test]
    fn shutdown_encodes_multiple_buffered_aac_frames_and_tail() {
        finish_buffered_audio(3585);
    }
}
