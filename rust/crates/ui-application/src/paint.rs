use openpilot_startup_ui::renderer::TextureOptions;
use openpilot_ui_framework::{
    assets::Texture,
    canvas::Canvas,
    draw::{Draw, ImageDraw},
    geometry::{Point, Rect},
    label::gui_label,
    text::Font,
    text_layout::{self, Horizontal, TextStyle, Vertical},
    Error,
};
pub const fn color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    u32::from_le_bytes([r, g, b, a])
}
pub fn texture(canvas: &mut Canvas, path: &str, size: (i32, i32)) -> Result<Texture, Error> {
    canvas.texture(
        path,
        TextureOptions {
            width: Some(size.0),
            height: Some(size.1),
            ..Default::default()
        },
    )
}
pub struct Label<'a> {
    pub text: &'a str,
    pub font: Font,
    pub size: f64,
    pub color: u32,
    pub horizontal: Horizontal,
    pub vertical: Vertical,
    pub elide: bool,
}
impl<'a> Label<'a> {
    pub fn new(text: &'a str, size: f64) -> Self {
        Self {
            text,
            size,
            font: Font::Normal,
            color: color(255, 255, 255, 229),
            horizontal: Horizontal::Left,
            vertical: Vertical::Middle,
            elide: true,
        }
    }
}
pub fn label(draw: &mut dyn Draw, rect: Rect, value: Label<'_>) -> Result<(), Error> {
    gui_label(
        draw,
        rect,
        value.text,
        TextStyle {
            font: value.font,
            size: value.size,
            spacing: 0.0,
            color: value.color,
        },
        (value.horizontal, value.vertical),
        value.elide,
    )
}
pub struct Text<'a> {
    pub value: &'a str,
    pub font: Font,
    pub size: f64,
    pub color: u32,
    pub spacing: f64,
}
pub fn text(draw: &mut dyn Draw, position: Point, value: Text<'_>) -> Result<(), Error> {
    text_layout::draw_text(
        draw,
        value.font,
        value.value,
        position,
        value.size,
        value.spacing,
        value.color,
    )
}
pub struct Image {
    pub texture: Texture,
    pub rect: Rect,
    pub tint: u32,
    pub origin: Point,
    pub rotation: f32,
}
pub fn image(draw: &mut dyn Draw, image: Image) -> Result<(), Error> {
    draw.image(ImageDraw {
        id: image.texture.id,
        source: Rect {
            x: 0.0,
            y: 0.0,
            width: image.texture.width,
            height: image.texture.height,
        },
        destination: image.rect,
        origin: image.origin,
        rotation: image.rotation,
        tint: image.tint,
    })
}
