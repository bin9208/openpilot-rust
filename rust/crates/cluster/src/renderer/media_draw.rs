use super::{ClusterUiRenderer, Error, Point, Rect, Result, TextureView};
use crate::models::NaviMediaFrame;

impl ClusterUiRenderer {
    pub(super) fn draw_cached_media(
        &mut self,
        frame: &NaviMediaFrame,
        texture: TextureView,
        source: Rect,
        destination: Rect,
    ) -> Result<()> {
        let cached = self
            .media
            .get(&frame.key)
            .ok_or(Error::Contract("media cache missing"))?;
        let hardware = cached.mime == "video/nv12-dmabuf";
        let yuv = cached.mime == "image/yuv420p";
        if !hardware && !yuv {
            return self.gfx.texture_pro(
                texture,
                source,
                destination,
                Point::default(),
                0.0,
                [255, 255, 255],
            );
        }
        if hardware
            && cached
                .hardware_token
                .is_none_or(|token| !self.media_support.images.contains_key(&token))
        {
            return Ok(());
        }
        if yuv && (cached.u_texture.is_none() || cached.v_texture.is_none()) {
            return Ok(());
        }
        self.ensure_media_shader(hardware)?;
        let cached = self
            .media
            .get(&frame.key)
            .ok_or(Error::Contract("media cache missing"))?;
        let shader = if hardware {
            self.media_support.external_shader.as_ref()
        } else {
            self.media_support.yuv_shader.as_ref()
        }
        .ok_or(Error::Contract("media shader missing"))?;
        let native = self
            .gfx
            .native
            .as_mut()
            .ok_or(Error::Contract("cluster window not open"))?;
        native.begin_shader(shader)?;
        let draw = (|| -> Result<()> {
            if hardware {
                let image = cached
                    .hardware_token
                    .and_then(|token| self.media_support.images.get(&token))
                    .ok_or(Error::Contract("media EGL image missing"))?;
                native.flush_camera();
                image.bind(native.native_texture(texture.id)?);
            } else {
                native.shader_texture(
                    shader,
                    "texture1",
                    cached
                        .u_texture
                        .as_ref()
                        .ok_or(Error::Contract("media U texture missing"))?,
                )?;
                native.shader_texture(
                    shader,
                    "texture2",
                    cached
                        .v_texture
                        .as_ref()
                        .ok_or(Error::Contract("media V texture missing"))?,
                )?;
                native.shader_vec2(shader, "uvScaleU", cached.uv_scale_u)?;
                native.shader_vec2(shader, "uvScaleV", cached.uv_scale_v)?;
            }
            native.tinted_texture(
                texture.id,
                super::graphics::rect(source)?,
                super::graphics::rect(destination)?,
                super::graphics::point(Point::default())?,
                0.0,
                u32::MAX,
            )?;
            Ok(())
        })();
        native.end_shader();
        draw
    }
    fn ensure_media_shader(&mut self, hardware: bool) -> Result<()> {
        use super::tokens::{
            NAVI_EXTERNAL_FRAGMENT_SHADER, NAVI_EXTERNAL_VERTEX_SHADER,
            NAVI_YUV420_FRAGMENT_SHADER, NV12_PACK_VERTEX_SHADER,
        };
        if hardware {
            if self.media_support.external_shader.is_none() {
                self.media_support.external_shader = Some(
                    self.gfx
                        .backend()?
                        .load_shader(NAVI_EXTERNAL_VERTEX_SHADER, NAVI_EXTERNAL_FRAGMENT_SHADER)?,
                );
            }
        } else if self.media_support.yuv_shader.is_none() {
            self.media_support.yuv_shader = Some(
                self.gfx
                    .backend()?
                    .load_shader(NV12_PACK_VERTEX_SHADER, NAVI_YUV420_FRAGMENT_SHADER)?,
            );
        }
        Ok(())
    }
}
