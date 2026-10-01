use super::*;
pub fn draw_indicator(
    draw: &mut dyn Draw,
    texture: Texture,
    offset: f64,
    content: f64,
    viewport: Rect,
) -> Result<(), Error> {
    let width = 300.0 - 200.0 * ((content - 1000.0) / 2000.0).clamp(0.0, 1.0);
    let maximum = content - f64::from(viewport.width);
    let ratio = if maximum.abs() > 1e-3 {
        -offset / maximum.abs()
    } else {
        0.0
    };
    let x = f64::from(viewport.x) + ratio * (f64::from(viewport.width) - width);
    let y = f64::from(viewport.y).max(0.0) + f64::from(viewport.height)
        - f64::from(texture.height) / 2.0;
    let mut left = x.max(f64::from(viewport.x));
    let right = (x + width).min(f64::from(viewport.x + viewport.width));
    let width = (width / 2.0).max(right - left);
    left = left
        .min(f64::from(viewport.x + viewport.width) - width)
        .max(f64::from(viewport.x));
    draw.image(ImageDraw {
        id: texture.id,
        source: Rect {
            x: 0.0,
            y: 0.0,
            width: texture.width,
            height: texture.height,
        },
        destination: Rect {
            x: float(left),
            y: float(y),
            width: float(width),
            height: texture.height,
        },
        origin: Point::default(),
        rotation: 0.0,
        tint: u32::from_le_bytes([255, 255, 255, 114]),
    })
}
