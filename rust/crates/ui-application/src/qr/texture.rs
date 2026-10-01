use super::{encode, Correction};
use openpilot_startup_ui::{
    draw::{Draw, ImageDraw, PixelBuffer, TextureResource, WHITE},
    geometry::{Point, Rect},
    Error,
};
pub struct Texture {
    data: Option<String>,
    texture: Option<Box<dyn TextureResource>>,
    correction: Correction,
}
impl Texture {
    pub fn new(correction: Correction) -> Self {
        Self {
            data: None,
            texture: None,
            correction,
        }
    }
    pub fn available(&self) -> bool {
        self.texture.is_some()
    }
    pub fn set_data(&mut self, data: Option<&str>, draw: &mut dyn Draw) -> Result<bool, Error> {
        let data = data.unwrap_or("");
        if self.data.as_deref() == Some(data) {
            return Ok(false);
        }
        self.texture = None;
        self.data = Some(data.into());
        if data.is_empty() {
            return Ok(true);
        }
        match encode(data, self.correction) {
            Ok(matrix) => {
                let (size, bytes) = matrix.rgba();
                let size =
                    i32::try_from(size).map_err(|_| Error::Contract("QR image size overflow"))?;
                match draw.upload_pixels(PixelBuffer {
                    dimensions: (size, size),
                    rgba: &bytes,
                }) {
                    Ok(texture) => self.texture = Some(texture),
                    Err(error) => openpilot_startup_ui::logging::emit(
                        openpilot_logging::record::Level::Error,
                        format!("QR code generation failed: {error}"),
                    ),
                }
            }
            Err(error) => {
                openpilot_startup_ui::logging::emit(
                    openpilot_logging::record::Level::Error,
                    format!("QR code generation failed: {error}"),
                );
            }
        }
        Ok(true)
    }
    pub fn draw(&self, draw: &mut dyn Draw, rect: Rect) -> Result<bool, Error> {
        let Some(texture) = &self.texture else {
            return Ok(false);
        };
        use num_traits::ToPrimitive;
        draw.image(ImageDraw {
            id: texture.id(),
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: texture
                    .dimensions()
                    .0
                    .to_f32()
                    .ok_or(Error::Contract("QR width overflow"))?,
                height: texture
                    .dimensions()
                    .1
                    .to_f32()
                    .ok_or(Error::Contract("QR height overflow"))?,
            },
            destination: rect,
            origin: Point::default(),
            rotation: 0.0,
            tint: WHITE,
        })?;
        Ok(true)
    }
    pub fn destroy(&mut self) {
        self.data = None;
        self.texture = None;
    }
}
