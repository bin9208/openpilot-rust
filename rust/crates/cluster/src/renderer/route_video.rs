use super::{ClusterUiRenderer, Error, Result, TextureView};
use crate::models::RouteOverlay;
use openpilot_startup_ui::{
    draw::PixelBuffer,
    renderer::{DynamicTexture, TextureFilter},
};

#[derive(Default)]
pub(super) struct RouteVideoCache {
    texture: Option<DynamicTexture>,
    frame_id: Option<String>,
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns invalid dimensions or native texture allocation/update errors.
    pub fn route_video_texture_for_overlay(
        &mut self,
        overlay: &RouteOverlay,
    ) -> Result<Option<TextureView>> {
        let width = i32::try_from(overlay.video_width)
            .map_err(|_| Error::Contract("route video width invalid"))?;
        let height = i32::try_from(overlay.video_height)
            .map_err(|_| Error::Contract("route video height invalid"))?;
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|size| size.checked_mul(4))
            .ok_or(Error::Contract("route video dimensions invalid"))?;
        if width <= 0 || height <= 0 {
            return Err(Error::Contract("route video dimensions must be positive"));
        }
        let resize = self
            .route_video
            .texture
            .as_ref()
            .is_none_or(|texture| texture.width != width || texture.height != height);
        if resize {
            self.route_video.texture = None;
            let stage = self.profile_start();
            let mut black = vec![0; expected];
            for pixel in black.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            let texture = self.gfx.backend()?.dynamic_pixels(width, height, &black)?;
            self.profile_add("route_video.alloc_texture", stage);
            let stage = self.profile_start();
            self.gfx
                .backend()?
                .dynamic_texture_filter(&texture, TextureFilter::Bilinear)?;
            self.profile_add("route_video.filter_texture", stage);
            self.route_video.texture = Some(texture);
            self.route_video.frame_id = None;
        }
        if overlay.video_frame_id != self.route_video.frame_id {
            if let Some(bytes) = overlay
                .video_rgba
                .as_deref()
                .filter(|bytes| bytes.len() == expected)
            {
                let stage = self.profile_start();
                let pixels = bytes.to_vec();
                self.profile_add("route_video.copy_pixels", stage);
                let stage = self.profile_start();
                let texture = self
                    .route_video
                    .texture
                    .as_ref()
                    .ok_or(Error::Contract("route video texture missing"))?;
                self.gfx
                    .native
                    .as_mut()
                    .ok_or(Error::Contract("cluster window not open"))?
                    .update_dynamic_pixels(
                        texture,
                        PixelBuffer {
                            dimensions: (width, height),
                            rgba: &pixels,
                        },
                    )?;
                self.profile_add("route_video.update_texture", stage);
                self.route_video
                    .frame_id
                    .clone_from(&overlay.video_frame_id);
            }
        }
        Ok(self.route_video.texture.as_ref().map(Self::texture_view))
    }
}
