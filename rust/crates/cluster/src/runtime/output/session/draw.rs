use super::{
    Backend, CaptureOptions, ClusterUiRenderer, ClusterUiState, Codec, FrameClock, Output, Result,
};
use crate::runtime::output::backend::ImageOutput;
use std::{io, sync::Arc, time::Instant};
fn elapsed(samples: &mut Option<Vec<(String, f64)>>, name: &str, start: Instant) {
    if let Some(samples) = samples {
        samples.push((name.into(), start.elapsed().as_secs_f64() * 1000.0));
    }
}
pub(super) fn render(
    output: &mut Output,
    renderer: &mut ClusterUiRenderer,
    state: &ClusterUiState,
    clock: FrameClock,
    mirror: i64,
) -> Result<()> {
    #[cfg(not(feature = "native-media"))]
    let _ = mirror;
    output.check_error()?;
    let samples = &mut output.samples;
    match &mut output.backend {
        Backend::Window => (),
        Backend::Image(image) => image_frame(image, renderer, state, clock, output.codec, samples)?,
        Backend::Async(pipeline) => asynchronous(pipeline, renderer, state, clock, samples)?,
        Backend::Software(pipeline) => {
            let started = Instant::now();
            if let Some(pattern) = &output.pattern {
                let [width, height] = output
                    .geometry
                    .logical
                    .map(|value| u32::try_from(value).map_err(io::Error::other));
                pipeline.submit_rgba(pattern, width?, height?)?;
                elapsed(samples, "main.usb_h264.submit_test_pattern", started);
            } else {
                renderer.with_rgba_buffer_at(
                    state,
                    clock,
                    CaptureOptions {
                        portrait_upload: output.portrait,
                        output_width: output.portrait.then_some(output.geometry.encoder[0]),
                        output_height: output.portrait.then_some(output.geometry.encoder[1]),
                    },
                    |_, pixels, (width, height)| {
                        elapsed(samples, "main.usb.render_rgba_total", started);
                        let started = Instant::now();
                        pipeline.submit_rgba(
                            pixels,
                            u32::try_from(width).map_err(io::Error::other)?,
                            u32::try_from(height).map_err(io::Error::other)?,
                        )?;
                        elapsed(samples, "main.usb_h264.submit_rgba", started);
                        Ok(())
                    },
                )?;
            }
        }
        #[cfg(feature = "native-media")]
        Backend::Hardware(native) => {
            if let Some(pattern) = &output.pattern {
                let started = Instant::now();
                native.pipeline.submit_nv12(pattern, false)?;
                elapsed(samples, "main.usb_h264.submit_test_pattern", started);
            } else {
                native.render(renderer, state, clock, mirror & 1 == 0)?;
            }
        }
    }
    Ok(())
}
fn image_frame(
    output: &mut ImageOutput,
    renderer: &mut ClusterUiRenderer,
    state: &ClusterUiState,
    clock: FrameClock,
    codec: Codec,
    samples: &mut Option<Vec<(String, f64)>>,
) -> Result<()> {
    let started = Instant::now();
    if codec == Codec::Png {
        let png = renderer.render_to_png_bytes_at(state, clock, true)?;
        elapsed(samples, "main.usb.render_png_total", started);
        let started = Instant::now();
        output.display.send_png(&png)?;
        elapsed(samples, "main.usb.send_png", started);
    } else {
        let jpeg = renderer.with_rgba_buffer_at(
            state,
            clock,
            CaptureOptions {
                portrait_upload: true,
                ..CaptureOptions::default()
            },
            |_, pixels, (width, height)| {
                elapsed(samples, "main.usb.render_rgba_total", started);
                let started = Instant::now();
                let jpeg = output.codec.encode_pixels(
                    pixels,
                    u32::try_from(width).map_err(io::Error::other)?,
                    u32::try_from(height).map_err(io::Error::other)?,
                )?;
                elapsed(samples, "main.usb.encode_jpeg", started);
                Ok(jpeg)
            },
        )?;
        let started = Instant::now();
        output.display.send_jpeg(&jpeg)?;
        elapsed(samples, "main.usb.send_jpeg", started);
    }
    Ok(())
}

fn asynchronous(
    pipeline: &mut crate::jpeg_async::Pipeline,
    renderer: &mut ClusterUiRenderer,
    state: &ClusterUiState,
    clock: FrameClock,
    samples: &mut Option<Vec<(String, f64)>>,
) -> Result<()> {
    let started = Instant::now();
    pipeline.wait_for_capacity(None).map_err(io::Error::other)?;
    elapsed(samples, "main.usb_async.wait_capacity", started);
    let started = Instant::now();
    let frame = renderer.render_to_rgba_bytes_at(
        state,
        clock,
        CaptureOptions {
            portrait_upload: true,
            ..CaptureOptions::default()
        },
    )?;
    elapsed(samples, "main.usb.render_rgba_total", started);
    let started = Instant::now();
    pipeline
        .submit_rgba(crate::jpeg_async::Frame {
            rgba: Arc::from(frame.rgba),
            width: u32::try_from(frame.width).map_err(io::Error::other)?,
            height: u32::try_from(frame.height).map_err(io::Error::other)?,
        })
        .map_err(io::Error::other)?;
    elapsed(samples, "main.usb_async.submit_rgba", started);
    Ok(())
}
