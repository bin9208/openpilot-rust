use crate::models::{NaviDashboardState, NaviGuidanceInfo, NaviMediaFrame};
use num_traits::AsPrimitive;
use std::collections::BTreeMap;

pub(super) type MediaFrames<'a> = BTreeMap<&'a str, &'a NaviMediaFrame>;

pub(super) fn media_frames(dashboard: Option<&NaviDashboardState>) -> MediaFrames<'_> {
    dashboard
        .into_iter()
        .flat_map(|v| &v.media)
        .map(|frame| (frame.key.as_str(), frame))
        .collect()
}

pub(super) fn frame<'a>(media: &MediaFrames<'a>, key: &str) -> Option<&'a NaviMediaFrame> {
    media.get(key).copied()
}

pub(super) fn guidance_text(guidance: &NaviGuidanceInfo) -> &str {
    if !guidance.main_text.is_empty() {
        &guidance.main_text
    } else if !guidance.road_name.is_empty() {
        &guidance.road_name
    } else {
        &guidance.near_direction
    }
}

pub(super) fn number(value: i64) -> f64 {
    value.as_()
}
pub(super) fn minutes(seconds: i64) -> f64 {
    (number(seconds) / 60.0).round_ties_even().max(1.0)
}

pub(super) fn append_format(text: &mut String, args: std::fmt::Arguments<'_>) {
    std::fmt::write(text, args).expect("String formatting writes are infallible");
}

pub(super) fn native_rect(
    rect: crate::renderer::Rect,
) -> crate::renderer::Result<crate::renderer::Rect> {
    let value = crate::renderer::graphics::rect(rect)?;
    Ok(crate::renderer::Rect::new(
        f64::from(value.x),
        f64::from(value.y),
        f64::from(value.width),
        f64::from(value.height),
    ))
}
