use crate::{
    draw::{Draw, ImageDraw},
    geometry::{Point, Rect},
    Error,
};
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    pub id: u32,
    pub width: f32,
    pub height: f32,
}
impl Texture {
    pub fn draw(
        self,
        draw: &mut dyn Draw,
        position: Point,
        scale: f32,
        tint: u32,
    ) -> Result<(), Error> {
        draw.image(ImageDraw {
            id: self.id,
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: self.width,
                height: self.height,
            },
            destination: Rect {
                x: position.x,
                y: position.y,
                width: self.width * scale,
                height: self.height * scale,
            },
            origin: Point::default(),
            rotation: 0.0,
            tint,
        })
    }
}
