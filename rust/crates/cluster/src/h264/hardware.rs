mod profile;
mod start;
use super::{
    native_sender::{AccessUnit, Sender, Snapshot},
    sps::PatchCache,
    vidc, Config, Control,
};
use crate::renderer::nv12::Nv12Layout;
use num_traits::ToPrimitive;
pub use profile::InputStage;
use std::{
    fs::File,
    io::{self, Write},
    os::fd::BorrowedFd,
};

struct Packets {
    sender: Sender,
    cache: PatchCache,
    config: Config,
    dump: Option<File>,
    writes: u64,
    failure: Option<String>,
    monitor: Option<super::observability::Monitor>,
}
impl Packets {
    fn accept(&mut self, packet: &vidc::Packet) {
        if packet.data.is_empty() {
            return;
        }
        let result = (|| {
            let total = std::time::Instant::now();
            let started = total;
            let copied = packet.data.clone();
            if let Some(monitor) = &self.monitor {
                monitor.elapsed("usb_h264.native.callback_copy", started)?;
                monitor.packet(packet, &copied, false)?;
            }
            let started = std::time::Instant::now();
            let bytes = self.cache.prepare(
                copied,
                i64::from(self.config.width),
                i64::from(self.config.height),
                i64::from(self.config.fps),
                packet.codec_config || packet.keyframe,
            );
            if let Some(monitor) = &self.monitor {
                monitor.elapsed("usb_h264.native.callback_prepare", started)?;
                monitor.packet(packet, &bytes, true)?;
            }
            if let Some(dump) = &mut self.dump {
                dump.write_all(&bytes)?;
                self.writes = self.writes.wrapping_add(1);
                if self
                    .writes
                    .is_multiple_of(u64::from(self.config.fps.max(1)))
                {
                    dump.flush()?;
                }
            }
            let started = std::time::Instant::now();
            let chunks: Vec<Vec<u8>> = bytes
                .chunks(self.sender.chunk_size.max(1))
                .map(<[u8]>::to_vec)
                .collect();
            if let Some(monitor) = &self.monitor {
                monitor.elapsed("usb_h264.native.callback_packetize", started)?;
                monitor.unit(
                    &bytes,
                    &chunks.iter().map(Vec::len).collect::<Vec<_>>(),
                    packet.keyframe,
                )?;
            }
            let started = std::time::Instant::now();
            self.sender.enqueue(AccessUnit {
                chunks,
                codec_config: packet.codec_config,
                keyframe: packet.keyframe,
            })?;
            if let Some(monitor) = &self.monitor {
                monitor.elapsed("usb_h264.native.callback_queue", started)?;
                monitor.elapsed("usb_h264.native.callback_total", total)?;
            }
            Ok::<(), io::Error>(())
        })();
        if let Err(error) = result {
            if self.failure.is_none() {
                self.failure = Some(error.to_string());
            }
        }
    }
}
pub struct Hardware {
    encoder: vidc::Encoder,
    packets: Packets,
    frame: u64,
    closed: bool,
}
impl Hardware {
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.packets.config
    }
    #[must_use]
    pub const fn chunk_size(&self) -> usize {
        self.packets.sender.chunk_size
    }

    /// # Errors
    /// Returns sender-state failure while attaching measured callback and queue observations.
    pub fn set_monitor(&mut self, monitor: super::observability::Monitor) -> io::Result<()> {
        self.packets.sender.set_monitor(monitor.clone())?;
        self.packets.monitor = Some(monitor);
        Ok(())
    }
    /// # Errors
    /// Returns observation-state failure while draining measured stages.
    pub fn profile_samples(&self) -> io::Result<Vec<(String, f64)>> {
        self.packets
            .monitor
            .as_ref()
            .map_or(Ok(Vec::new()), super::observability::Monitor::drain)
    }
    #[must_use]
    pub fn monitor(&self) -> Option<super::observability::Monitor> {
        self.packets.monitor.clone()
    }
    fn timing_samples(&self) -> io::Result<()> {
        if let Some(monitor) = &self.packets.monitor {
            let timings = self.encoder.last_timings();
            monitor.samples(
                [
                    ("usb_h264.native.pre_poll", timings.pre_poll_us),
                    ("usb_h264.native.wait_input", timings.wait_input_us),
                    ("usb_h264.native.convert", timings.convert_us),
                    ("usb_h264.native.sync", timings.sync_us),
                    ("usb_h264.native.queue", timings.queue_us),
                    ("usb_h264.native.post_poll", timings.post_poll_us),
                    ("usb_h264.native.total_inner", timings.total_us),
                ]
                .into_iter()
                .filter(|(_, us)| *us > 0)
                .map(|(name, us)| (name.into(), us.to_f64().unwrap_or(f64::MAX) / 1000.0)),
            )?;
        }
        Ok(())
    }
    #[must_use]
    pub fn input_layout(&self) -> Nv12Layout {
        self.encoder.input_layout()
    }
    /// # Errors
    /// Preserves a callback/dump failure or the existing USB sender's failure.
    pub fn check_error(&self) -> io::Result<()> {
        if let Some(error) = &self.packets.failure {
            return Err(io::Error::other(error.clone()));
        }
        if self.closed {
            return Err(io::Error::other("native H264 pipeline is closed"));
        }
        self.packets.sender.check_error()
    }
    /// # Errors
    /// Preserves output control failures on the owned sender thread.
    pub fn control(&mut self, control: Control) -> io::Result<()> {
        self.packets.sender.control(control)
    }
    /// # Errors
    /// Preserves shared-state failure while returning actual sender counters.
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        self.packets.sender.snapshot()
    }
    fn timestamp(&self) -> io::Result<u64> {
        let timestamp =
            u128::from(self.frame) * 1_000_000 / u128::from(self.packets.config.fps.max(1));
        u64::try_from(timestamp & u128::from(u64::MAX)).map_err(io::Error::other)
    }
    /// # Errors
    /// Preserves packed input, native queue, callback, or previously recorded transfer failures.
    pub fn submit_nv12(&mut self, bytes: &[u8], active: bool) -> io::Result<()> {
        self.check_error()?;
        let timestamp = self.timestamp()?;
        let packets = &mut self.packets;
        let mut callback = |packet: &vidc::Packet| {
            packets.accept(packet);
            Ok(())
        };
        if active {
            self.encoder
                .encode_nv12_active(bytes, timestamp, &mut callback)
        } else {
            self.encoder.encode_nv12(bytes, timestamp, &mut callback)
        }
        .map_err(io::Error::other)?;
        self.timing_samples()?;
        self.frame = self.frame.wrapping_add(1);
        Ok(())
    }
    /// # Errors
    /// Preserves render/queue failures, returning an acquired slot if rendering fails.
    pub fn render_input(
        &mut self,
        dmabuf_written: bool,
        draw: impl FnOnce(&mut [u8], BorrowedFd<'_>, Nv12Layout) -> io::Result<()>,
    ) -> io::Result<()> {
        self.render_input_stages(dmabuf_written, None, draw)
    }
    fn render_input_stages(
        &mut self,
        dmabuf_written: bool,
        stages: Option<[&str; 3]>,
        draw: impl FnOnce(&mut [u8], BorrowedFd<'_>, Nv12Layout) -> io::Result<()>,
    ) -> io::Result<()> {
        self.check_error()?;
        let timestamp = self.timestamp()?;
        let layout = self.encoder.input_layout();
        let packets = &mut self.packets;
        let mut callback = |packet: &vidc::Packet| {
            packets.accept(packet);
            Ok(())
        };
        let started = std::time::Instant::now();
        let index = self
            .encoder
            .acquire(&mut callback)
            .map_err(io::Error::other)?;
        self.profile_stage(stages.map(|names| names[0]), started)?;
        let started = std::time::Instant::now();
        let rendered = self
            .encoder
            .with_input(index, |bytes, fd| draw(bytes, fd, layout))
            .map_err(io::Error::other)?;
        if let Err(error) = rendered {
            self.encoder.cancel(index).map_err(io::Error::other)?;
            return Err(error);
        }
        self.profile_stage(stages.map(|names| names[1]), started)?;
        let started = std::time::Instant::now();
        let packets = &mut self.packets;
        let mut callback = |packet: &vidc::Packet| {
            packets.accept(packet);
            Ok(())
        };
        self.encoder
            .submit(index, timestamp, dmabuf_written, &mut callback)
            .map_err(io::Error::other)?;
        self.profile_stage(stages.map(|names| names[2]), started)?;
        self.timing_samples()?;
        self.frame = self.frame.wrapping_add(1);
        Ok(())
    }
    /// # Errors
    /// Preserves sender drain and dump flush failures after native buffers have closed.
    pub fn close(&mut self) -> io::Result<()> {
        if self.closed {
            return Ok(());
        }
        let packets = &mut self.packets;
        let _ = self.encoder.drain(250, &mut |packet| {
            packets.accept(packet);
            Ok(())
        });
        self.encoder.close();
        self.packets.sender.close()?;
        if let Some(dump) = &mut self.packets.dump {
            dump.flush()?;
        }
        self.packets.dump = None;
        self.closed = true;
        if let Some(monitor) = &self.packets.monitor {
            monitor.finish()?;
        }
        Ok(())
    }
}
impl Drop for Hardware {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
