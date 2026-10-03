use crate::geometry::Rect;

#[derive(Clone, Copy)]
pub enum PlaneFormat {
    Luma,
    Chroma,
}
#[derive(Clone, Copy)]
pub enum Style {
    Large,
    Compact { engaged: bool, driver: bool },
}
pub struct CameraDraw {
    pub luma: u32,
    pub chroma: Option<u32>,
    pub source: Rect,
    pub destination: Rect,
    pub style: Style,
}
