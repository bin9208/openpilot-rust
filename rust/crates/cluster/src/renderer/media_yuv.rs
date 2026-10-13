use super::{media::CachedMedia, ClusterUiRenderer, Result};
use crate::models::NaviMediaFrame;
use openpilot_startup_ui::{camera::PlaneFormat, renderer::TextureFilter};

pub(super) struct PlaneLayout {
    pub size: (i32, i32),
    pub shapes: [(i32, i32); 3],
}
impl PlaneLayout {
    fn from_frame(frame: &NaviMediaFrame) -> Option<Self> {
        let size = (
            i32::try_from(frame.width).ok()?,
            i32::try_from(frame.height).ok()?,
        );
        if size.0 <= 0 || size.1 <= 0 || size.0 % 2 != 0 || size.1 % 2 != 0 {
            return None;
        }
        let strides = frame.plane_strides?;
        let strides = [
            i32::try_from(strides[0]).ok()?,
            i32::try_from(strides[1]).ok()?,
            i32::try_from(strides[2]).ok()?,
        ];
        if strides[0] < size.0 || strides[1] < size.0 / 2 || strides[2] < size.0 / 2 {
            return None;
        }
        let shapes = [
            (strides[0], size.1),
            (strides[1], size.1 / 2),
            (strides[2], size.1 / 2),
        ];
        let data = frame.plane_data.as_ref()?;
        for (plane, (width, height)) in data.iter().zip(shapes) {
            let expected = usize::try_from(width)
                .ok()?
                .checked_mul(usize::try_from(height).ok()?)?;
            if plane.len() < expected {
                return None;
            }
        }
        Some(Self { size, shapes })
    }
    fn reusable(&self, cached: &CachedMedia, frame: &NaviMediaFrame) -> bool {
        cached.mime == frame.mime
            && cached.size == self.size
            && (cached.texture.width, cached.texture.height) == self.shapes[0]
            && cached
                .u_texture
                .as_ref()
                .is_some_and(|texture| texture.width == self.shapes[1].0)
            && cached
                .v_texture
                .as_ref()
                .is_some_and(|texture| texture.width == self.shapes[2].0)
    }
}
impl ClusterUiRenderer {
    pub(super) fn upload_yuv(
        &mut self,
        frame: &NaviMediaFrame,
        cached: Option<CachedMedia>,
    ) -> Option<CachedMedia> {
        let Some(layout) = PlaneLayout::from_frame(frame) else {
            return cached;
        };
        let reuse = cached
            .as_ref()
            .is_some_and(|value| layout.reusable(value, frame));
        let mut old = cached;
        let mut replacement = if reuse {
            old.take()
        } else {
            self.allocate_yuv(frame, &layout).ok()
        };
        let Some(value) = replacement.as_mut() else {
            return old;
        };
        let start = self.profile_start();
        let upload = (|| -> Result<()> {
            let planes = frame
                .plane_data
                .as_ref()
                .ok_or(super::Error::Contract("YUV planes missing"))?;
            let textures = [
                &value.texture,
                value
                    .u_texture
                    .as_ref()
                    .ok_or(super::Error::Contract("U texture missing"))?,
                value
                    .v_texture
                    .as_ref()
                    .ok_or(super::Error::Contract("V texture missing"))?,
            ];
            for (texture, plane) in textures.into_iter().zip(planes) {
                self.gfx
                    .backend()?
                    .update_camera_plane(texture.id(), plane)?;
            }
            Ok(())
        })();
        if upload.is_err() {
            return if reuse { replacement } else { old };
        }
        self.profile_add("navi_media.upload_yuv420p", start);
        value.sequence = frame.sequence;
        replacement
    }
    fn allocate_yuv(
        &mut self,
        frame: &NaviMediaFrame,
        layout: &PlaneLayout,
    ) -> Result<CachedMedia> {
        let native = self.gfx.backend()?;
        let texture = native.camera_plane(layout.shapes[0], PlaneFormat::Luma)?;
        native.dynamic_texture_filter(&texture, TextureFilter::Bilinear)?;
        let u_texture = native.camera_plane(layout.shapes[1], PlaneFormat::Luma)?;
        native.dynamic_texture_filter(&u_texture, TextureFilter::Point)?;
        let v_texture = native.camera_plane(layout.shapes[2], PlaneFormat::Luma)?;
        native.dynamic_texture_filter(&v_texture, TextureFilter::Point)?;
        Ok(CachedMedia {
            sequence: frame.sequence,
            mime: frame.mime.clone(),
            size: layout.size,
            texture,
            u_texture: Some(u_texture),
            v_texture: Some(v_texture),
            uv_scale_u: [
                super::graphics::scalar(
                    f64::from(layout.shapes[0].0) / (2.0 * f64::from(layout.shapes[1].0)),
                )?,
                1.0,
            ],
            uv_scale_v: [
                super::graphics::scalar(
                    f64::from(layout.shapes[0].0) / (2.0 * f64::from(layout.shapes[2].0)),
                )?,
                1.0,
            ],
            hardware_token: None,
        })
    }
}
