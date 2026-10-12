use crate::{live::navigation::resolve_navi_speed_limit, models::ClusterUiState};

#[must_use]
pub fn merge_overlay_state(base: &ClusterUiState, overlay: &ClusterUiState) -> ClusterUiState {
    let mut result = base.clone();
    (result.speed_limit_kph, result.speed_limit_source) = resolve_navi_speed_limit(
        base.speed_limit_kph,
        base.speed_limit_source.as_deref(),
        overlay.navi_live.as_ref(),
    );
    result.external_nav_active |= overlay.navi_live.as_ref().is_some_and(|navi| {
        navi.current.is_some()
            || navi
                .status
                .as_ref()
                .is_some_and(|status| status.guidance_active)
    });
    result.navi_live.clone_from(&overlay.navi_live);
    result.navi_dashboard.clone_from(&overlay.navi_dashboard);
    if result
        .center_clock_text
        .as_ref()
        .is_none_or(String::is_empty)
    {
        result
            .center_clock_text
            .clone_from(&overlay.center_clock_text);
    }
    result
}
