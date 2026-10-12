//! Source design-space arithmetic stays double precision until the graphics call.

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    #[must_use]
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        self.x <= point.x
            && point.x <= self.x + self.width
            && self.y <= point.y
            && point.y <= self.y + self.height
    }
}

pub trait IntoColor {
    fn rgba(self) -> [u8; 4];
}
impl IntoColor for [u8; 3] {
    fn rgba(self) -> [u8; 4] {
        [self[0], self[1], self[2], 255]
    }
}
impl IntoColor for [u8; 4] {
    fn rgba(self) -> [u8; 4] {
        self
    }
}
#[must_use]
pub fn with_alpha(color: impl IntoColor, alpha: u8) -> [u8; 4] {
    let mut result = color.rgba();
    result[3] = alpha;
    result
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct MediaFit {
    pub cover: bool,
    pub align_x: f64,
    pub align_y: f64,
}
impl Default for MediaFit {
    fn default() -> Self {
        Self {
            cover: false,
            align_x: 0.5,
            align_y: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextureView {
    pub id: u32,
    pub width: i32,
    pub height: i32,
}
