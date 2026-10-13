use super::{media::CachedMedia, ClusterUiRenderer, Error, Result};
use crate::{media::HardwareFrame, models::NaviMediaFrame};
use openpilot_startup_ui::{
    egl::{Context, FrameLayout},
    renderer::TextureFilter,
};
use std::{os::fd::AsFd, rc::Rc};

impl ClusterUiRenderer {
    pub(super) fn upload_hardware(
        &mut self,
        frame: &NaviMediaFrame,
        cached: Option<CachedMedia>,
    ) -> Result<Option<CachedMedia>> {
        let Some(buffer) = frame.hardware_buffer.as_ref() else {
            return Ok(cached);
        };
        let Some(size) = i32::try_from(frame.width)
            .ok()
            .zip(i32::try_from(frame.height).ok())
        else {
            return Ok(cached);
        };
        let layout = FrameLayout {
            width: size.0,
            height: size.1,
            stride: buffer.layout.stride,
            uv_offset: buffer.layout.uv_offset,
        };
        if size.0 <= 0
            || size.1 <= 0
            || layout.stride < size.0
            || layout
                .stride
                .checked_mul(size.1)
                .is_none_or(|length| layout.uv_offset < length)
        {
            return Ok(cached);
        }
        let start = self.profile_start();
        if let Err(error) = self.import_media_image(buffer, layout) {
            report_failure(buffer, &error);
            return Ok(cached);
        }
        let reuse = cached.as_ref().is_some_and(|value| {
            value.mime == frame.mime
                && value.size == size
                && value.u_texture.is_none()
                && value.v_texture.is_none()
        });
        let mut old = cached;
        let mut replacement = if reuse {
            old.take()
        } else {
            let create = (|| -> Result<CachedMedia> {
                let texture = self.gfx.backend()?.dynamic_pixels(1, 1, &[0, 0, 0, 255])?;
                self.gfx
                    .backend()?
                    .dynamic_texture_filter(&texture, TextureFilter::Bilinear)?;
                Ok(CachedMedia::image(frame, texture))
            })();
            match create {
                Ok(value) => Some(value),
                Err(error) => {
                    report_failure(buffer, &error);
                    return Ok(old);
                }
            }
        };
        let value = replacement
            .as_mut()
            .ok_or(Error::Contract("hardware texture missing"))?;
        let bind = (|| -> Result<()> {
            let native = self.gfx.backend()?;
            native.dynamic_logical_size(&mut value.texture, size)?;
            let image = self
                .media_support
                .images
                .get(&buffer.token())
                .ok_or(Error::Contract("hardware EGL image missing"))?;
            native.flush_camera();
            image.bind(native.native_texture(value.texture.id())?);
            Ok(())
        })();
        if let Err(error) = bind {
            report_failure(buffer, &error);
            return Ok(if reuse { replacement } else { old });
        }
        value.sequence = frame.sequence;
        value.size = size;
        value.hardware_token = Some(buffer.token());
        self.evict_media_images(buffer.token());
        self.profile_add("navi_media.bind_nv12_dmabuf", start);
        Ok(replacement)
    }
    fn import_media_image(&mut self, buffer: &HardwareFrame, layout: FrameLayout) -> Result<()> {
        let token = buffer.token();
        if self.media_support.egl.is_none() {
            self.media_support.egl =
                Some(Rc::new(Context::new().map_err(|_| {
                    Error::Contract("EGL DMA-BUF import is unavailable")
                })?));
        }
        self.media_support
            .images
            .retain(|(generation, _), _| *generation == token.0);
        self.media_support
            .image_order
            .retain(|value| value.0 == token.0);
        if !self.media_support.images.contains_key(&token) {
            let context = self
                .media_support
                .egl
                .as_ref()
                .ok_or(Error::Contract("media EGL context missing"))?;
            let image = context
                .create_owned(layout, buffer.as_fd())
                .map_err(|_| Error::Contract("EGL rejected the decoded NV12 DMA-BUF"))?;
            self.media_support.images.insert(token, image);
        }
        self.media_support
            .image_order
            .retain(|value| *value != token);
        self.media_support.image_order.push_back(token);
        Ok(())
    }
    fn evict_media_images(&mut self, current: (u64, u32)) {
        let mut cursor = 0;
        while self.media_support.images.len() > super::tokens::NAVI_EGL_IMAGE_CACHE_LIMIT
            && cursor < self.media_support.image_order.len()
        {
            let token = self.media_support.image_order[cursor];
            if token == current
                || self
                    .media
                    .values()
                    .any(|cached| cached.hardware_token == Some(token))
            {
                cursor += 1;
            } else {
                self.media_support.image_order.remove(cursor);
                self.media_support.images.remove(&token);
            }
        }
    }
}
fn report_failure(buffer: &HardwareFrame, error: &Error) {
    let message = error.to_string();
    buffer.mark_egl_import_failed(&message);
    println!(
        "Navi hardware H264 EGL import failed token={:?}; requesting PyAV fallback: {message}",
        buffer.token()
    );
}
