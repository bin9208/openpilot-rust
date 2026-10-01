use openpilot_ui_framework::{
    draw::{Draw, TextDraw},
    geometry::{Point, Rect},
    text::{Font, Measure},
    Error,
};
pub struct NoDraw;
impl Measure for NoDraw {
    fn measure(&self, _: Font, _: &str, _: f32, _: f32) -> Point {
        Point::default()
    }
}
impl Draw for NoDraw {
    fn text(&mut self, _: TextDraw<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn rounded(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn border(&mut self, _: Rect, _: f32, _: u32) -> Result<(), Error> {
        Ok(())
    }
    fn texture(&mut self, _: bool, _: Rect, _: Point, _: f32) -> Result<(), Error> {
        Ok(())
    }
    fn scissor(&mut self, _: Option<Rect>) -> Result<(), Error> {
        Ok(())
    }
    fn gradient(&mut self, _: Rect, _: [u32; 4]) -> Result<(), Error> {
        Ok(())
    }
}
