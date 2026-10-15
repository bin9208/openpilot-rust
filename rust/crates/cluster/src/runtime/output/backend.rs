use super::super::{Error, Result};
use super::{
    factory::{self, Spec},
    retained::{self, Remote, Transport},
    settings, Geometry,
};
use crate::{
    h264,
    jpeg_async::{
        self,
        codec::{Codec as JpegCodec, Config as JpegConfig},
        DisplayOutput,
    },
    runtime_args::{Codec, H264Backend, OutputRate, RuntimeArgs},
    runtime_preferences::Snapshot,
    usb::ParamsRuntime,
};
use std::io;

pub(super) struct ImageOutput {
    pub display: crate::usb::Display<ParamsRuntime>,
    pub codec: JpegCodec,
}
pub(super) enum Backend {
    Window,
    Image(Box<ImageOutput>),
    Async(jpeg_async::Pipeline),
    Software(Box<h264::Software>),
    #[cfg(feature = "native-media")]
    Hardware(Box<super::native::NativeOutput>),
}

struct Setup<C, F> {
    create: Option<F>,
    resources: Option<C>,
    geometry: Option<Geometry>,
}
impl<C, F: FnOnce(Geometry) -> Result<C>> Setup<C, F> {
    fn prepare(
        &mut self,
        args: &RuntimeArgs,
        dimensions: Option<[u32; 2]>,
        native: bool,
    ) -> Result<Geometry> {
        let geometry = Geometry::resolve(args, dimensions, native)?;
        if let Some(create) = self.create.take() {
            self.resources = Some(create(geometry)?);
        }
        self.geometry = Some(geometry);
        Ok(geometry)
    }
}

pub(super) fn open<C>(
    args: &RuntimeArgs,
    preferences: &Snapshot,
    rate: OutputRate,
    create: impl FnOnce(Geometry) -> Result<C>,
) -> Result<(Backend, Geometry, C, Option<Transport>)> {
    let mut setup = Setup {
        create: Some(create),
        resources: None,
        geometry: None,
    };
    let mut transport = None;
    let output = if args.usb_output() {
        let spec = Spec::new(args, preferences, rate)?;
        // The S3 expires idle TCP clients after two seconds. Finish graphics
        // creation before the asynchronous sender discovers/connects it.
        if spec.wifi() {
            setup.prepare(args, Some([1920, 462]), false)?;
        }
        match args.usb_codec {
            Codec::H264 => {
                if args.usb_h264_backend == H264Backend::Auto {
                    transport = Some(Transport::open(spec.clone()).map_err(factory::classify)?);
                }
                h264(
                    args,
                    rate,
                    &spec,
                    &mut setup,
                    transport.as_ref().map(Transport::remote),
                )?
            }
            Codec::Jpeg if args.transport.usb_async => {
                let codec = JpegConfig::from_environment(
                    args.usb_jpeg_quality,
                    args.usb_jpeg_encoder,
                    args.diagnostics.profile_render,
                )?;
                let output = jpeg_async::Pipeline::start(move || {
                    Ok(DisplayOutput::new(spec.open()?, JpegCodec::new(codec)))
                })
                .map_err(|error| Error::Io(io::Error::other(error)))?;
                setup.prepare(args, output.dimensions(), false)?;
                Backend::Async(output)
            }
            Codec::Jpeg | Codec::Png => {
                let display = spec.open()?;
                let codec = JpegCodec::new(JpegConfig::from_environment(
                    args.usb_jpeg_quality,
                    args.usb_jpeg_encoder,
                    args.diagnostics.profile_render,
                )?);
                setup.prepare(args, Some(display.dimensions()), false)?;
                Backend::Image(Box::new(ImageOutput { display, codec }))
            }
        }
    } else {
        setup.prepare(args, None, false)?;
        Backend::Window
    };
    Ok((
        output,
        setup
            .geometry
            .ok_or_else(|| Error::Invalid("cluster output geometry missing".into()))?,
        setup
            .resources
            .ok_or_else(|| Error::Invalid("cluster output setup missing".into()))?,
        transport,
    ))
}

fn h264<C, F: FnOnce(Geometry) -> Result<C>>(
    args: &RuntimeArgs,
    rate: OutputRate,
    spec: &Spec,
    setup: &mut Setup<C, F>,
    remote: Option<Remote>,
) -> Result<Backend> {
    if args.usb_h264_backend != H264Backend::Ffmpeg {
        match hardware(args, rate, spec, setup, remote.clone()) {
            Ok(output) => return Ok(output),
            Err(Error::H264Initialization(error)) if args.usb_h264_backend == H264Backend::Auto => {
                eprintln!(
                    "Warning: native H264 encoder unavailable, falling back to ffmpeg: {error}"
                );
            }
            Err(error) => return Err(error),
        }
    }
    let config = settings::software(args, rate)?;
    let spec = spec.clone();
    let mut started = None;
    let mut output = h264::Software::start_with(
        config,
        retained::factory(spec, remote),
        |config, dimensions| {
            let geometry = setup
                .prepare(args, dimensions, false)
                .map_err(io::Error::other)?;
            settings::configure(config, geometry).map_err(io::Error::other)?;
            started = Some(std::time::Instant::now());
            Ok(())
        },
    )
    .map_err(factory::classify)?;
    let monitor = h264::observability::Monitor::new(
        "ffmpeg",
        args.usb_h264_diagnose_interval,
        args.diagnostics.profile_render,
        args.diagnostics.usb_h264_debug,
        crate::live::monotonic()?,
    );
    monitor.session(
        output.config(),
        setup
            .geometry
            .ok_or_else(|| Error::Invalid("H264 session geometry missing".into()))?
            .frame,
        match args.usb_h264_backend {
            H264Backend::Auto => "auto",
            H264Backend::Native => "native",
            H264Backend::Ffmpeg => "ffmpeg",
        },
        output.chunk_size(),
    )?;
    if let Some(started) = started {
        monitor.elapsed("usb_h264.start", started)?;
    }
    output.set_monitor(monitor)?;
    Ok(Backend::Software(Box::new(output)))
}

#[cfg(feature = "native-media")]
fn hardware<C, F: FnOnce(Geometry) -> Result<C>>(
    args: &RuntimeArgs,
    rate: OutputRate,
    spec: &Spec,
    setup: &mut Setup<C, F>,
    remote: Option<Remote>,
) -> Result<Backend> {
    let config = settings::software(args, rate)?;
    let spec = spec.clone();
    let mut started = None;
    let mut output = h264::Hardware::start_with(
        h264::vidc::Config::default(),
        config,
        retained::factory(spec, remote),
        |encoder, config, dimensions| {
            let geometry = setup
                .prepare(args, dimensions, true)
                .map_err(io::Error::other)?;
            settings::configure(config, geometry).map_err(io::Error::other)?;
            *encoder = settings::hardware(args, config).map_err(|error| {
                io::Error::other(Error::H264Initialization(io::Error::other(error)))
            })?;
            [encoder.width, encoder.height] = geometry.encoder;
            started = Some(std::time::Instant::now());
            Ok(())
        },
    )
    .map_err(factory::classify)?;
    let monitor = h264::observability::Monitor::new(
        "native",
        args.usb_h264_diagnose_interval,
        args.diagnostics.profile_render,
        args.diagnostics.usb_h264_debug,
        crate::live::monotonic()?,
    );
    monitor.session(
        output.config(),
        setup
            .geometry
            .ok_or_else(|| Error::Invalid("H264 session geometry missing".into()))?
            .frame,
        match args.usb_h264_backend {
            H264Backend::Auto => "auto",
            H264Backend::Native => "native",
            H264Backend::Ffmpeg => "ffmpeg",
        },
        output.chunk_size(),
    )?;
    if let Some(started) = started {
        monitor.elapsed("usb_h264.start", started)?;
    }
    output.set_monitor(monitor)?;
    Ok(Backend::Hardware(Box::new(
        super::native::NativeOutput::new(output),
    )))
}
#[cfg(not(feature = "native-media"))]
fn hardware<C, F: FnOnce(Geometry) -> Result<C>>(
    _args: &RuntimeArgs,
    _rate: OutputRate,
    _spec: &Spec,
    _setup: &mut Setup<C, F>,
    _remote: Option<Remote>,
) -> Result<Backend> {
    Err(Error::H264Initialization(io::Error::other(
        "native H264 runtime requires the native-media feature",
    )))
}
