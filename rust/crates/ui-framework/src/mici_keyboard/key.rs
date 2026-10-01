use crate::{
    animation::Bounce,
    assets::Texture,
    draw::{Draw, ImageDraw},
    geometry::{Point, Rect},
    text::Font,
    text_layout::{self as text, float},
    Error,
};
use num_traits::ToPrimitive;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Character,
    Small,
    Icon { bottom: bool },
}
pub struct Key {
    pub value: String,
    pub kind: Kind,
    pub rect: Rect,
    pub original: Point,
    pub x: Bounce,
    pub y: Bounce,
    pub size: Bounce,
    pub alpha: Bounce,
    pub icon: Option<Texture>,
    initialized: bool,
}
impl Key {
    pub fn new(value: &str, kind: Kind, fps: f64) -> Self {
        Self {
            value: value.into(),
            kind,
            rect: Rect::default(),
            original: Point::default(),
            x: Bounce::new(0.0, 0.065, fps, 2.0),
            y: Bounce::new(0.0, 0.065, fps, 2.0),
            size: Bounce::new(
                if kind == Kind::Small { 24.0 } else { 42.0 },
                0.065,
                fps,
                2.0,
            ),
            alpha: Bounce::new(1.0, 0.04875, fps, 2.0),
            icon: None,
            initialized: false,
        }
    }
    pub fn position(&mut self, position: (f64, f64), base_y: f64, smooth: bool) {
        let (x, global_y) = position;
        let y = global_y - base_y;
        if !self.initialized {
            self.x.position.x = x;
            self.y.position.x = y;
            self.original = Point {
                x: float(x),
                y: float(y + 10.0),
            };
            self.initialized = true;
        }
        if !smooth {
            self.x.position.x = x;
            self.y.position.x = y;
        }
        self.rect.x = float(self.x.update(x));
        self.rect.y = float(base_y + self.y.update(y));
    }
    pub fn set_font_size(&mut self, size: f64) {
        self.size.update(if self.kind == Kind::Small {
            size * (24.0 / 42.0)
        } else {
            size
        });
    }
    pub fn paint(&self, draw: &mut dyn Draw) -> Result<(), Error> {
        let alpha = (255.0 * self.alpha.position.x)
            .min(255.0)
            .to_u8()
            .ok_or(Error::Contract("key alpha out of range"))?;
        let tint = u32::from_le_bytes([255, 255, 255, alpha]);
        match self.kind {
            Kind::Character | Kind::Small => {
                let font = if self.kind == Kind::Small {
                    Font::Bold
                } else {
                    Font::SemiBold
                };
                let size = self.size.position.x.round_ties_even();
                let bounds = text::measure(draw, font, &self.value, size, 0.0);
                text::draw_text(
                    draw,
                    font,
                    &self.value,
                    Point {
                        x: self.rect.x + self.rect.width / 2.0 - bounds.x / 2.0,
                        y: self.rect.y + self.rect.height / 2.0 - bounds.y / 2.0,
                    },
                    size,
                    0.0,
                    tint,
                )?;
            }
            Kind::Icon { bottom } => {
                let icon = self.icon.ok_or(Error::Contract("key icon missing"))?;
                let scale = 1.0 + ((self.size.position.x - 42.0) / 42.0).clamp(0.0, 1.0) * 0.5;
                let width = f64::from(icon.width) * scale;
                let height = f64::from(icon.height) * scale;
                draw.image(ImageDraw {
                    id: icon.id,
                    source: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: icon.width,
                        height: icon.height,
                    },
                    destination: Rect {
                        x: float(
                            f64::from(self.rect.x) + (f64::from(self.rect.width) - width) / 2.0,
                        ),
                        y: if bottom {
                            self.rect.y
                        } else {
                            float(
                                f64::from(self.rect.y)
                                    + (f64::from(self.rect.height) - height) / 2.0,
                            )
                        },
                        width: float(width),
                        height: float(height),
                    },
                    origin: Point::default(),
                    rotation: 0.0,
                    tint,
                })?;
            }
        }
        Ok(())
    }
}
