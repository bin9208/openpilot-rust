use crate::{
    h264::{Hardware, InputStage},
    models::ClusterUiState,
    renderer::{
        nv12::{
            dmabuf::DmaTargetPool, readback::DirectReadback, Nv12Layout, Nv12Packer, Nv12Request,
        },
        ClusterUiRenderer, FrameClock,
    },
};
use std::{error::Error as StdError, io};

#[derive(Debug, thiserror::Error)]
enum BoundaryError {
    #[error(transparent)]
    Read(#[from] crate::renderer::nv12::readback::Error),
    #[error(transparent)]
    Dma(#[from] crate::renderer::nv12::dmabuf::Error),
    #[error("{0}")]
    Contract(&'static str),
}
fn read_error(error: crate::renderer::nv12::readback::Error) -> io::Error {
    io::Error::other(BoundaryError::Read(error))
}
fn dma_error(error: crate::renderer::nv12::dmabuf::Error) -> io::Error {
    io::Error::other(BoundaryError::Dma(error))
}
fn boundary(error: &(dyn StdError + 'static)) -> bool {
    if error.is::<BoundaryError>() {
        return true;
    }
    if let Some(inner) = error
        .downcast_ref::<io::Error>()
        .and_then(io::Error::get_ref)
    {
        if boundary(inner) {
            return true;
        }
    }
    error.source().is_some_and(boundary)
}

pub(super) struct NativeOutput {
    pub pipeline: Hardware,
    packer: Nv12Packer,
    readback: Option<DirectReadback>,
    dma: Option<DmaTargetPool>,
    initialized: bool,
    buffer: Vec<u8>,
}
impl NativeOutput {
    pub fn new(pipeline: Hardware) -> Self {
        Self {
            pipeline,
            packer: Nv12Packer::default(),
            readback: None,
            dma: None,
            initialized: false,
            buffer: Vec::new(),
        }
    }
    fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.readback = match DirectReadback::create_tici() {
            Ok(readback) => readback,
            Err(error) => {
                eprintln!("TICI GLES direct NV12 readback unavailable: {error}");
                None
            }
        };
        self.dma = match DmaTargetPool::create_tici() {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("Encoder DMA-BUF render targets unavailable: {error}");
                None
            }
        };
    }
    pub fn render(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
        flip_x: bool,
    ) -> io::Result<()> {
        self.pipeline.check_error()?;
        self.initialize();
        renderer
            .prepare_trip_report_cache(state, clock.monotonic)
            .map_err(io::Error::other)?;
        if self.try_dma(renderer, state, clock, flip_x)? {
            return Ok(());
        }
        if self.try_async(renderer, state, clock, flip_x)? {
            return Ok(());
        }
        if self.try_direct(renderer, state, clock, flip_x)? {
            return Ok(());
        }
        let full = self.pipeline.input_layout();
        let active = full.uv_offset
            + usize::try_from(full.stride).map_err(io::Error::other)?
                * usize::try_from(full.uv_scanlines).map_err(io::Error::other)?;
        let layout = Nv12Layout {
            byte_count: active,
            ..full
        };
        let started = std::time::Instant::now();
        let bytes = renderer
            .render_to_nv12_buffer(
                &mut self.packer,
                Nv12Request {
                    layout,
                    buffer: std::mem::take(&mut self.buffer),
                    flip_x,
                },
                |renderer| renderer.render_at(state, clock, None),
            )
            .map_err(io::Error::other)?;
        let monitor = self.pipeline.monitor();
        if let Some(monitor) = &monitor {
            monitor.elapsed("main.usb.render_nv12_total", started)?;
        }
        let started = std::time::Instant::now();
        let result = self.pipeline.submit_nv12(&bytes, true);
        if let Some(monitor) = monitor {
            monitor.elapsed("main.usb_h264.submit_nv12", started)?;
        }
        self.buffer = bytes;
        result
    }
    fn try_dma(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
        flip_x: bool,
    ) -> io::Result<bool> {
        let Some(pool) = &mut self.dma else {
            return Ok(false);
        };
        let packer = &mut self.packer;
        let result = self
            .pipeline
            .render_input_profiled(InputStage::Dma, |_, fd, layout| {
                {
                    let view = pool
                        .target_for(
                            fd,
                            usize::try_from(layout.stride).map_err(io::Error::other)?,
                            layout.byte_count,
                        )
                        .map_err(dma_error)?;
                    renderer
                        .render_to_nv12_external(packer, layout, flip_x, view.info(), |renderer| {
                            renderer.render_at(state, clock, None)
                        })
                        .map_err(io::Error::other)?;
                }
                pool.wait_for_gpu().map_err(dma_error)
            });
        match result {
            Ok(()) => Ok(true),
            Err(error) if boundary(&error) => {
                eprintln!(
                    "H264 DMA-BUF output unavailable; using asynchronous PBO readback: {error}"
                );
                if let Some(mut pool) = self.dma.take() {
                    pool.close().map_err(io::Error::other)?;
                }
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    fn try_async(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
        flip_x: bool,
    ) -> io::Result<bool> {
        let Some(readback) = self
            .readback
            .as_mut()
            .filter(|readback| readback.async_supported())
        else {
            return Ok(false);
        };
        let result = (|| -> io::Result<()> {
            if readback
                .async_ready(clock.perf_counter)
                .map_err(read_error)?
            {
                self.pipeline
                    .render_input_profiled(InputStage::Pbo, |bytes, _, _| {
                        if readback
                            .copy_ready(bytes, clock.perf_counter)
                            .map_err(read_error)?
                        {
                            Ok(())
                        } else {
                            Err(io::Error::other(BoundaryError::Contract(
                                "ready NV12 PBO was unavailable during copy",
                            )))
                        }
                    })?;
            }
            if readback.async_can_enqueue() {
                let started = std::time::Instant::now();
                renderer
                    .render_to_nv12_target(
                        &mut self.packer,
                        self.pipeline.input_layout(),
                        flip_x,
                        |renderer| renderer.render_at(state, clock, None),
                        |info| {
                            readback
                                .enqueue_rgba(info, clock.perf_counter)
                                .map_err(read_error)?;
                            Ok(())
                        },
                    )
                    .map_err(io::Error::other)?;
                if let Some(monitor) = self.pipeline.monitor() {
                    monitor.elapsed("main.usb.render_nv12_pbo_total", started)?;
                }
            } else if let Some(monitor) = self.pipeline.monitor() {
                monitor.samples([("main.usb_h264.pbo_ring_full_drop".into(), 0.0)])?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(true),
            Err(error) if boundary(&error) => {
                eprintln!(
                    "H264 asynchronous PBO readback unavailable; using synchronous direct ION: {error}"
                );
                readback.disable_async().map_err(read_error)?;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    fn try_direct(
        &mut self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
        flip_x: bool,
    ) -> io::Result<bool> {
        let Some(readback) = &mut self.readback else {
            return Ok(false);
        };
        let packer = &mut self.packer;
        let result = self
            .pipeline
            .render_input_profiled(InputStage::Direct, |bytes, _, layout| {
                renderer
                    .render_to_nv12_target(
                        packer,
                        layout,
                        flip_x,
                        |renderer| renderer.render_at(state, clock, None),
                        |info| {
                            readback.read_rgba(info, bytes).map_err(read_error)?;
                            Ok(())
                        },
                    )
                    .map_err(io::Error::other)
            });
        match result {
            Ok(()) => Ok(true),
            Err(error) if boundary(&error) => {
                eprintln!(
                    "H264 direct ION readback unavailable; using staged NV12 fallback: {error}"
                );
                if let Some(mut readback) = self.readback.take() {
                    readback.close().map_err(read_error)?;
                }
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    pub fn close(&mut self) -> io::Result<()> {
        let dma = self
            .dma
            .take()
            .map_or(Ok(()), |mut pool| pool.close().map_err(io::Error::other));
        let encoder = self.pipeline.close();
        let readback = self
            .readback
            .take()
            .map_or(Ok(()), |mut readback| readback.close().map_err(read_error));
        self.packer.close();
        dma.and(encoder).and(readback)
    }
}
