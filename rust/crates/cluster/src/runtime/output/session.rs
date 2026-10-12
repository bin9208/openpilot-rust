mod draw;
use super::super::{Error, Result};
use super::{backend, pattern, Backend, Geometry};
use crate::{
    h264::Control,
    models::ClusterUiState,
    renderer::{CaptureOptions, ClusterUiRenderer, FrameClock},
    runtime_args::{Codec, Orientation, OutputRate, RuntimeArgs},
    runtime_preferences::Snapshot,
};
use std::io;

pub struct Output {
    pub geometry: Geometry,
    backend: Backend,
    codec: Codec,
    portrait: bool,
    pattern: Option<Vec<u8>>,
    nv12_pattern: bool,
    fps: Option<u32>,
    closed: bool,
    transport: Option<super::retained::Transport>,
    samples: Option<Vec<(String, f64)>>,
}
impl Output {
    /// # Errors
    /// Preserves display, configured input/renderer, codec, encoder and stream startup failures.
    pub fn open_with<C>(
        args: &RuntimeArgs,
        preferences: &Snapshot,
        rate: OutputRate,
        create: impl FnOnce(Geometry) -> Result<C>,
    ) -> Result<(Self, C)> {
        let (backend, geometry, resources, transport) =
            backend::open(args, preferences, rate, create)?;
        let mut output = Self {
            geometry,
            backend,
            codec: args.usb_codec,
            portrait: args.usb_h264_orientation == Orientation::Portrait,
            pattern: None,
            nv12_pattern: args.patterns.usb_h264_test_pattern_nv12,
            fps: if args.usb_output() && args.usb_codec == Codec::H264 {
                Some(
                    u32::try_from(crate::rate::encoder_fps(
                        rate.target_fps,
                        args.usb_h264_fps,
                    )?)
                    .map_err(|error| Error::Invalid(error.to_string()))?,
                )
            } else {
                None
            },
            closed: false,
            transport,
            samples: args.diagnostics.profile_render.then(Vec::new),
        };
        if args.patterns.usb_h264_test_pattern && output.native() {
            return Err(Error::Invalid("--usb-h264-test-pattern is only supported by --usb-h264-backend ffmpeg; use --usb-h264-test-pattern-nv12 for native H264".into()));
        }
        if output.nv12_pattern && !output.native() {
            return Err(Error::Invalid(
                "--usb-h264-test-pattern-nv12 requires the native H264 backend".into(),
            ));
        }
        if args.patterns.usb_h264_test_pattern {
            output.pattern = Some(pattern::rgba(geometry.logical[0], geometry.logical[1])?);
        }
        #[cfg(feature = "native-media")]
        if output.nv12_pattern {
            if let Backend::Hardware(native) = &output.backend {
                output.pattern = Some(pattern::nv12(native.pipeline.input_layout())?);
            }
        }
        Ok((output, resources))
    }
    #[must_use]
    pub const fn fps(&self) -> Option<u32> {
        self.fps
    }
    #[must_use]
    pub fn encoder_pid(&self) -> Option<u32> {
        match &self.backend {
            Backend::Software(pipeline) => pipeline.encoder_pid(),
            _ => None,
        }
    }
    #[must_use]
    pub fn native(&self) -> bool {
        #[cfg(feature = "native-media")]
        if matches!(self.backend, Backend::Hardware(_)) {
            return true;
        }
        false
    }
    /// # Errors
    /// Returns the existing worker or software child failure.
    pub fn check_error(&mut self) -> Result<()> {
        match &mut self.backend {
            Backend::Async(output) => output
                .check_error()
                .map_err(|error| Error::Io(io::Error::other(error))),
            Backend::Software(output) => Ok(output.check_error()?),
            #[cfg(feature = "native-media")]
            Backend::Hardware(output) => Ok(output.pipeline.check_error()?),
            Backend::Window | Backend::Image(_) => Ok(()),
        }
    }
    /// # Errors
    /// Returns transport or owned worker-control dispatch failures.
    pub fn control(&mut self, control: Control) -> Result<()> {
        match &mut self.backend {
            Backend::Window => (),
            Backend::Image(output) => match control {
                Control::Brightness(value) => {
                    output.display.set_brightness(value, false);
                }
                Control::Orientation(value) => {
                    output.display.set_orientation(value, false);
                }
                Control::DisplayFps(value) => {
                    output.display.set_display_fps(value, false);
                }
            },
            Backend::Async(output) => output
                .control(control)
                .map_err(|error| Error::Io(io::Error::other(error)))?,
            Backend::Software(output) => output.control(control)?,
            #[cfg(feature = "native-media")]
            Backend::Hardware(output) => output.pipeline.control(control)?,
        }
        Ok(())
    }
    /// # Errors
    /// Preserves rendering, selected codec, mapped native input and existing USB transport failures.
    pub fn render(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
        mirror: i64,
    ) -> Result<()> {
        draw::render(self, renderer, state, clock, mirror)
    }
    /// # Errors
    /// Returns JPEG worker synchronization failures while draining measured stage samples.
    pub fn profile_samples(&mut self) -> Result<Vec<(String, f64)>> {
        let mut samples = match &mut self.backend {
            Backend::Async(output) => output
                .profile_samples()
                .map_err(|error| Error::Io(io::Error::other(error)))?,
            Backend::Image(output) => {
                let mut samples = output.codec.profile_samples().to_vec();
                samples.extend(
                    output
                        .display
                        .profile_samples()
                        .iter()
                        .map(|&(name, value)| (name.into(), value)),
                );
                output.codec.clear_profile_samples();
                output.display.clear_profile_samples();
                samples
            }
            Backend::Software(pipeline) => pipeline.profile_samples()?,
            #[cfg(feature = "native-media")]
            Backend::Hardware(native) => native.pipeline.profile_samples()?,
            Backend::Window => Vec::new(),
        };
        if let Some(transport) = &self.transport {
            samples.extend(transport.samples()?);
        }
        if let Some(local) = &mut self.samples {
            samples.append(local);
        }
        Ok(samples)
    }
    /// # Errors
    /// Preserves worker, encoder, GPU cleanup and timed JPEG join failures after cleanup attempts.
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        let cleanup = (|| -> Result<()> {
            match &mut self.backend {
                Backend::Window => (),
                Backend::Image(output) => output.display.close(),
                Backend::Async(output) => {
                    if !output
                        .close()
                        .map_err(|error| Error::Io(io::Error::other(error)))?
                    {
                        return Err(Error::Invalid(
                            "JPEG output worker did not stop before its deadline".into(),
                        ));
                    }
                }
                Backend::Software(output) => output.close()?,
                #[cfg(feature = "native-media")]
                Backend::Hardware(output) => output.close()?,
            }
            Ok(())
        })();
        let transport = self
            .transport
            .as_mut()
            .map_or(Ok(()), |transport| transport.close().map_err(Error::from));
        cleanup.and(transport)?;
        self.closed = true;
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("cluster output cleanup: {error}");
        }
    }
}
