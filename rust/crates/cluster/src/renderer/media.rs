use super::{ClusterUiRenderer, MediaFit, Rect, Result, TextureView};
use crate::models::{NaviGuidanceImage, NaviMediaFrame};
use base64::{
    alphabet,
    engine::{general_purpose::GeneralPurpose, DecodePaddingMode, GeneralPurposeConfig},
    Engine,
};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    draw::PixelBuffer,
    renderer::{DecodedImage, DynamicTexture, TextureFilter},
};

pub(super) struct CachedMedia {
    pub sequence: u64,
    pub mime: String,
    pub size: (i32, i32),
    pub texture: DynamicTexture,
    pub u_texture: Option<DynamicTexture>,
    pub v_texture: Option<DynamicTexture>,
    pub uv_scale_u: [f32; 2],
    pub uv_scale_v: [f32; 2],
    pub hardware_token: Option<(u64, u32)>,
}
impl CachedMedia {
    pub fn image(frame: &NaviMediaFrame, texture: DynamicTexture) -> Self {
        Self {
            sequence: frame.sequence,
            mime: frame.mime.clone(),
            size: (texture.width, texture.height),
            texture,
            u_texture: None,
            v_texture: None,
            uv_scale_u: [1.0; 2],
            uv_scale_v: [1.0; 2],
            hardware_token: None,
        }
    }
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns graphics errors while preserving the source missing-image fallback.
    pub fn navi_guidance_texture_for(
        &mut self,
        image: Option<&NaviGuidanceImage>,
    ) -> Result<Option<TextureView>> {
        let encoded = image.map_or("", |value| value.image_base64.as_str());
        if encoded.is_empty() {
            self.guidance = None;
            self.guidance_hash.clear();
            self.navi_guidance_size = None;
            return Ok(None);
        }
        let hash = image
            .filter(|value| !value.image_hash.is_empty())
            .map_or(encoded, |value| value.image_hash.as_str());
        if self.guidance.is_some() && self.guidance_hash == hash {
            return Ok(self.guidance.as_ref().map(Self::texture_view));
        }
        self.guidance = None;
        self.guidance_hash.clear();
        self.navi_guidance_size = None;
        let payload = if encoded.chars().take(64).any(|value| value == ',') {
            encoded.split_once(',').map_or(encoded, |(_, value)| value)
        } else {
            encoded
        };
        if !payload.is_ascii() {
            return Ok(None);
        }
        let clean: Vec<u8> = payload
            .bytes()
            .filter(|value| value.is_ascii_alphanumeric() || matches!(value, b'+' | b'/' | b'='))
            .collect();
        let engine = GeneralPurpose::new(
            &alphabet::STANDARD,
            GeneralPurposeConfig::new()
                .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                .with_decode_allow_trailing_bits(true),
        );
        let Ok(bytes) = engine.decode(clean) else {
            return Ok(None);
        };
        let extension =
            if image.is_some_and(|value| value.image_mime.to_lowercase().contains("jpeg")) {
                ".jpg"
            } else {
                ".png"
            };
        let Ok(decoded) = DecodedImage::encoded(extension, &bytes) else {
            return Ok(None);
        };
        let texture = self.gfx.backend()?.dynamic_filtered_image(
            PixelBuffer {
                dimensions: (decoded.width, decoded.height),
                rgba: &decoded.rgba,
            },
            TextureFilter::Bilinear,
        );
        let Ok(texture) = texture else {
            return Ok(None);
        };
        self.navi_guidance_size = Some((texture.width, texture.height));
        self.guidance_hash = hash.into();
        self.guidance = Some(texture);
        Ok(self.guidance.as_ref().map(Self::texture_view))
    }
    fn navi_media_texture_for(
        &mut self,
        frame: Option<&NaviMediaFrame>,
    ) -> Result<Option<TextureView>> {
        let Some(frame) = frame else {
            return Ok(None);
        };
        let has_payload = frame.hardware_buffer.is_some()
            || frame.data.is_some()
            || frame.mime == "image/yuv420p"
                && frame.plane_data.is_some()
                && frame.plane_strides.is_some();
        if !frame.present || !has_payload {
            self.media.remove(&frame.key);
            return Ok(None);
        }
        if self
            .media
            .get(&frame.key)
            .is_some_and(|value| value.sequence == frame.sequence && value.mime == frame.mime)
        {
            return Ok(self
                .media
                .get(&frame.key)
                .map(|value| Self::texture_view(&value.texture)));
        }
        let cached = self.media.remove(&frame.key);
        let replacement = match frame.mime.as_str() {
            "image/yuv420p" => self.upload_yuv(frame, cached),
            "video/nv12-dmabuf" => self.upload_hardware(frame, cached)?,
            "image/rgba" => self.upload_rgba(frame, cached)?,
            _ => self.upload_encoded(frame, cached)?,
        };
        let view = replacement
            .as_ref()
            .map(|value| Self::texture_view(&value.texture));
        if let Some(value) = replacement {
            self.media.insert(frame.key.clone(), value);
        }
        Ok(view)
    }
    fn upload_rgba(
        &mut self,
        frame: &NaviMediaFrame,
        cached: Option<CachedMedia>,
    ) -> Result<Option<CachedMedia>> {
        let data = frame.data.as_deref().unwrap_or_default();
        let Some(size) =
            frame
                .width
                .to_i32()
                .zip(frame.height.to_i32())
                .filter(|(width, height)| {
                    *width > 0
                        && *height > 0
                        && i64::from(*width) * i64::from(*height) * 4
                            == data.len().to_i64().unwrap_or(-1)
                })
        else {
            return Ok(cached);
        };
        if let Some(mut value) =
            cached.filter(|value| value.mime == frame.mime && value.size == size)
        {
            self.gfx.backend()?.update_dynamic_pixels(
                &value.texture,
                PixelBuffer {
                    dimensions: size,
                    rgba: data,
                },
            )?;
            value.sequence = frame.sequence;
            return Ok(Some(value));
        }
        let texture = self.gfx.backend()?.dynamic_filtered_image(
            PixelBuffer {
                dimensions: size,
                rgba: data,
            },
            TextureFilter::Bilinear,
        );
        Ok(texture
            .ok()
            .map(|texture| CachedMedia::image(frame, texture)))
    }
    fn upload_encoded(
        &mut self,
        frame: &NaviMediaFrame,
        cached: Option<CachedMedia>,
    ) -> Result<Option<CachedMedia>> {
        let Ok(decoded) = DecodedImage::encoded(
            if frame.mime.contains("jpeg") {
                ".jpg"
            } else {
                ".png"
            },
            frame.data.as_deref().unwrap_or_default(),
        ) else {
            return Ok(cached);
        };
        let texture = self.gfx.backend()?.dynamic_filtered_image(
            PixelBuffer {
                dimensions: (decoded.width, decoded.height),
                rgba: &decoded.rgba,
            },
            TextureFilter::Bilinear,
        );
        Ok(if let Ok(texture) = texture {
            Some(CachedMedia::image(frame, texture))
        } else {
            cached
        })
    }
    /// # Errors
    /// Returns graphics errors while preserving the source missing-image fallback.
    pub fn navi_media_fitted_size(
        &mut self,
        frame: Option<&NaviMediaFrame>,
        rect: Rect,
        cover: bool,
    ) -> Result<Option<(f64, f64)>> {
        let rect = native_rect(rect)?;
        self.navi_media_texture_for(frame)?;
        Ok(frame
            .and_then(|frame| self.media.get(&frame.key))
            .filter(|value| value.size.0 > 0 && value.size.1 > 0)
            .map(|value| fitted(value.size, rect, cover)))
    }
    /// # Errors
    /// Returns graphics errors while preserving the source missing-image fallback.
    pub fn draw_navi_media(
        &mut self,
        frame: Option<&NaviMediaFrame>,
        rect: Rect,
        fit: MediaFit,
    ) -> Result<bool> {
        let rect = native_rect(rect)?;
        let Some(texture) = self.navi_media_texture_for(frame)? else {
            return Ok(false);
        };
        let cached = frame
            .and_then(|frame| self.media.get(&frame.key))
            .ok_or(super::Error::Contract("media cache missing after upload"))?;
        if cached.size.0 <= 0 || cached.size.1 <= 0 {
            return Ok(false);
        }
        let (width, height) = fitted(cached.size, rect, fit.cover);
        let source = Rect::new(0.0, 0.0, f64::from(cached.size.0), f64::from(cached.size.1));
        let destination = Rect::new(
            rect.x + (rect.width - width) * fit.align_x.clamp(0.0, 1.0),
            rect.y + (rect.height - height) * fit.align_y.clamp(0.0, 1.0),
            width,
            height,
        );
        self.draw_cached_media(
            frame.ok_or(super::Error::Contract("media frame missing"))?,
            texture,
            source,
            destination,
        )?;
        Ok(true)
    }
}
fn fitted(size: (i32, i32), rect: Rect, cover: bool) -> (f64, f64) {
    let (width, height) = (f64::from(size.0), f64::from(size.1));
    let scale = if cover {
        (rect.width / width).max(rect.height / height)
    } else {
        (rect.width / width).min(rect.height / height)
    };
    (width * scale, height * scale)
}
fn native_rect(rect: Rect) -> Result<Rect> {
    let value = super::graphics::rect(rect)?;
    Ok(Rect::new(
        f64::from(value.x),
        f64::from(value.y),
        f64::from(value.width),
        f64::from(value.height),
    ))
}
