use super::{Error, Result};
use num_traits::{AsPrimitive, ToPrimitive};
pub const OPTION_LABELS: [(&str, &str); 5] = [
    ("front_radar_only", "front-radar-only"),
    ("route_loop", "route-loop"),
    ("pause_on_cutin", "pause-on-cutin"),
    ("show_route_overlay", "route-overlay"),
    ("road_camera", "road-camera"),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Rect {
    #[must_use]
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
    #[must_use]
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.width > 0
            && self.height > 0
            && x >= self.x
            && y >= self.y
            && i64::from(x) < i64::from(self.x) + i64::from(self.width)
            && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Layout {
    pub open: Rect,
    pub previous: Rect,
    pub pause: Rect,
    pub next: Rect,
    pub seek: Rect,
    pub video: Rect,
    pub debug: Rect,
    pub options: Vec<(&'static str, Rect)>,
}
impl Layout {
    /// # Errors
    /// Returns an error for dimensions outside the source integer rectangle range.
    pub fn new(width: i32, height: i32) -> Result<Self> {
        let debug_width = 380.min(
            (f64::from(width) * 0.35)
                .trunc()
                .max(300.)
                .to_i32()
                .ok_or(Error::Contract("tools debug width invalid"))?,
        );
        let video_width = (i64::from(width) - i64::from(debug_width) - 60).max(160);
        let video_height = (i64::from(height) - 139 - 120).max(120);
        let video = Rect::new(24, 120, integer(video_width)?, integer(video_height)?);
        let right = 24 + video_width;
        Ok(Self {
            open: Rect::new(24, 72, 116, 36),
            previous: Rect::new(150, 72, 92, 36),
            pause: Rect::new(252, 72, 92, 36),
            next: Rect::new(354, 72, 92, 36),
            seek: Rect::new(
                24,
                integer(i64::from(height) - 42)?,
                integer((i64::from(width) - 48).max(120))?,
                16,
            ),
            video,
            debug: Rect::new(
                integer(right + 18)?,
                120,
                integer(i64::from(width) - right - 42)?,
                video.height,
            ),
            options: option_rects(width, height)?,
        })
    }
}
fn integer(value: i64) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::Contract("tools rectangle out of range"))
}
/// # Errors
/// Returns an error for dimensions outside the source integer rectangle range.
pub fn option_rects(width: i32, height: i32) -> Result<Vec<(&'static str, Rect)>> {
    let item_width = ((i64::from(width) - 48).max(1) / 3).max(180);
    OPTION_LABELS
        .iter()
        .enumerate()
        .map(|(index, (name, _))| {
            let index =
                i64::try_from(index).map_err(|_| Error::Contract("tools option index invalid"))?;
            Ok((
                *name,
                Rect::new(
                    integer(24 + (index % 3) * item_width)?,
                    integer(i64::from(height) - 123 + (index / 3) * 25)?,
                    integer(item_width - 8)?,
                    24,
                ),
            ))
        })
        .collect()
}
#[must_use]
pub fn seek_value(mouse_x: i32, rect: Rect, duration: f64) -> f64 {
    let delta: f64 = (i64::from(mouse_x) - i64::from(rect.x)).as_();
    let ratio = (delta / f64::from(rect.width.max(1))).clamp(0., 1.);
    duration.max(0.) * ratio
}
/// # Errors
/// Rejects nonfinite times and values outside the supported integer duration range.
pub fn format_time(seconds: f64) -> Result<String> {
    if !seconds.is_finite() {
        return Err(Error::Contract("tools time is not finite"));
    }
    let total = seconds
        .trunc()
        .max(0.)
        .to_i128()
        .ok_or(Error::Contract("tools time is out of range"))?;
    Ok(format!("{}:{:02}", total / 60, total % 60))
}
