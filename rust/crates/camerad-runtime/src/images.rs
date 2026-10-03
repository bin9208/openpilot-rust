use openpilot_camera_kernel::{Device, ImportedBuffers, Master};
use openpilot_msgq::{RawVisionImage, VisionImage, VisionLayout, VisionMetadata, VisionServer};
use std::os::fd::AsFd;

use crate::{CameraConfig, CameraError, IspPort, OutputMode};

pub(crate) struct FrameImages<'device> {
    request: &'device Device,
    imports: Vec<ImportedBuffers>,
    raw: Vec<RawVisionImage>,
    yuv: Vec<VisionImage>,
    layout: VisionLayout,
    raw_length: usize,
}

impl<'device> FrameImages<'device> {
    pub(crate) fn new(
        master: &'device Master,
        server: &VisionServer,
        config: CameraConfig,
        isp: &IspPort<'_, '_>,
        kind: openpilot_camerad::sensor::SensorKind,
    ) -> Result<Self, CameraError> {
        let sensor = kind.config();
        let raw_length = sensor
            .frame_height
            .checked_add(sensor.extra_height)
            .and_then(|height| height.checked_mul(sensor.frame_stride))
            .ok_or(CameraError::Size)? as usize;
        let (width, height) = isp.dimensions();
        let nv12 = isp.layout().map_err(crate::IspPortError::from)?;
        let layout = VisionLayout {
            width: width as usize,
            height: height as usize,
            stride: nv12.stride as usize,
            uv_offset: (nv12.stride * nv12.y_height) as usize,
            len: nv12.size as usize,
        };
        let mut output = Self {
            request: &master.request,
            imports: Vec::with_capacity(config.depth),
            raw: Vec::with_capacity(config.depth),
            yuv: Vec::new(),
            layout,
            raw_length,
        };
        if config.mode != OutputMode::Ife {
            for _ in 0..config.depth {
                output.raw.push(RawVisionImage::new(raw_length)?);
            }
        }
        // The source creates the full 18-buffer IPC stream even for raw output.
        output.yuv = server.create_stream(config.stream, 18, layout)?;
        for slot in 0..config.depth {
            output.imports.push(master.request.import_images(
                master.mmu,
                isp.bps_handle().is_some_and(|handle| handle.0 > 0),
                output.raw.get(slot).map(AsFd::as_fd),
                (config.mode != OutputMode::Raw).then(|| output.yuv[slot].as_fd()),
            )?);
        }
        Ok(output)
    }

    pub(crate) fn handles(&self, slot: usize) -> Result<(i32, i32), CameraError> {
        let image = self.imports.get(slot).ok_or(CameraError::Slot(slot))?;
        Ok((image.raw as i32, image.yuv as i32))
    }

    pub(crate) fn layout(&self) -> VisionLayout {
        self.layout
    }

    pub(crate) fn publish(&self, slot: usize, metadata: VisionMetadata) -> Result<(), CameraError> {
        Ok(self
            .yuv
            .get(slot)
            .ok_or(CameraError::Slot(slot))?
            .publish(metadata)?)
    }

    pub(crate) fn copy_yuv(&self, slot: usize, destination: &mut [u8]) -> Result<(), CameraError> {
        Ok(self
            .yuv
            .get(slot)
            .ok_or(CameraError::Slot(slot))?
            .copy_into(destination)?)
    }

    pub(crate) fn copy_raw(&self, slot: usize) -> Result<Option<Vec<u8>>, CameraError> {
        if self.raw.is_empty() {
            return Ok(None);
        }
        let image = self.raw.get(slot).ok_or(CameraError::Slot(slot))?;
        let mut bytes = vec![0; self.raw_length];
        image.copy_into(&mut bytes)?;
        Ok(Some(bytes))
    }

    pub(crate) fn release(&mut self) {
        for image in self.imports.drain(..) {
            for handle in [
                image.raw,
                if image.yuv == image.raw { 0 } else { image.yuv },
            ] {
                if handle != 0 {
                    let result = self.request.release_buffer(handle);
                    if result.code != 0 {
                        camera_log!(Error, "release image: {}", result.code);
                    }
                }
            }
        }
    }
}

impl Drop for FrameImages<'_> {
    fn drop(&mut self) {
        self.release();
    }
}
