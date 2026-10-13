mod debug;
mod health;
mod summary;

use crate::renderer::{IntoColor, Rect};

#[derive(Clone, Copy)]
struct CardStyle {
    background: [u8; 4],
    outline: [u8; 4],
    text: [u8; 4],
    muted: [u8; 4],
    target: [u8; 4],
}
impl CardStyle {
    fn from_theme(theme: &crate::config::ClusterTheme) -> Self {
        Self {
            background: theme.route_panel_bg.rgba(),
            outline: theme.faint.rgba(),
            text: theme.text.rgba(),
            muted: theme.muted.rgba(),
            target: theme.panel_bg.rgba(),
        }
    }
}

#[derive(Clone, Copy)]
struct Metric<'a> {
    bounds: Rect,
    label: &'a str,
    value: &'a str,
    unit: &'a str,
}
