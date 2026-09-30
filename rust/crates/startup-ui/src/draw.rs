use crate::{
    geometry::{Point, Rect},
    text::{Font, Measure},
    Error,
};
pub const WHITE: u32 = u32::from_le_bytes([255, 255, 255, 255]);
pub const BLACK: u32 = u32::from_le_bytes([0, 0, 0, 255]);
pub const LIGHTGRAY: u32 = u32::from_le_bytes([200, 200, 200, 255]);
pub const DARKGRAY: u32 = u32::from_le_bytes([55, 55, 55, 255]);
pub const IP: u32 = u32::from_le_bytes([230, 230, 230, 235]);
pub const BUTTON_TEXT: u32 = u32::from_le_bytes([228, 228, 228, 255]);
pub struct TextDraw<'a> {
    pub font: Font,
    pub text: &'a str,
    pub position: Point,
    pub size: f32,
    pub spacing: f32,
    pub color: u32,
}
pub struct ImageDraw {
    pub id: u32,
    pub source: Rect,
    pub destination: Rect,
    pub origin: Point,
    pub rotation: f32,
    pub tint: u32,
}
pub trait Draw: Measure {
    fn font_scale(&self) -> f64 {
        1.0
    }
    fn circle(&mut self, _center: Point, _radius: f32, _color: u32) -> Result<(), Error> {
        Err(Error::Contract("circle drawing unavailable"))
    }
    fn gradient(&mut self, _rect: Rect, _colors: [u32; 4]) -> Result<(), Error> {
        Err(Error::Contract("gradient drawing unavailable"))
    }
    fn line(&mut self, _start: Point, _end: Point, _thick: f32, _color: u32) -> Result<(), Error> {
        Err(Error::Contract("line drawing unavailable"))
    }
    fn image(&mut self, _image: ImageDraw) -> Result<(), Error> {
        Err(Error::Contract("image drawing unavailable"))
    }
    fn emoji(
        &mut self,
        _text: &str,
        _position: Point,
        _size: f32,
        _tint: u32,
    ) -> Result<(), Error> {
        Err(Error::Contract("emoji drawing unavailable"))
    }

    fn text(&mut self, text: TextDraw<'_>) -> Result<(), Error>;
    fn rounded(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error>;
    fn border(&mut self, rect: Rect, roundness: f32, color: u32) -> Result<(), Error>;
    fn texture(
        &mut self,
        track: bool,
        rect: Rect,
        origin: Point,
        rotation: f32,
    ) -> Result<(), Error>;
    fn scissor(&mut self, rect: Option<Rect>) -> Result<(), Error>;
}
