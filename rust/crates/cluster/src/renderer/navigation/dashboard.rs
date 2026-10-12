use super::common::{frame, guidance_text, media_frames, minutes, number, MediaFrames};
use crate::config::{BLUE_SOFT, DESIGN_HEIGHT, RED, WHITE};
use crate::models::{ClusterUiState, NaviLiveState};
use crate::renderer::display::{display_speed, format_navi_distance, speed_unit};
use crate::renderer::tokens::{
    NAVI_MAP_BACKGROUND, NAVI_MODE_LEFT_W, NAVI_MODE_MAP_W, NAVI_MODE_MAP_X, NAVI_MODE_RIGHT_W,
    NAVI_MODE_RIGHT_X,
};
use crate::renderer::{Anchor, ClusterUiRenderer, MediaFit, Point, Rect, Result};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns graphics, font, or media resource failures while drawing this source surface.
    pub fn draw_navi_dashboard(&mut self, state: &ClusterUiState) -> Result<()> {
        let theme = self.current_theme();
        let dashboard = state.navi_dashboard.as_ref();
        let navi = state.navi_live.as_ref();
        let height = number(DESIGN_HEIGHT);
        self.gfx.rectangle(
            Rect::new(0.0, 0.0, NAVI_MODE_LEFT_W, height),
            theme.panel_bg,
        )?;
        self.gfx.rectangle(
            Rect::new(NAVI_MODE_RIGHT_X, 0.0, NAVI_MODE_RIGHT_W, height),
            theme.panel_bg,
        )?;
        self.gfx.line(
            Point::new(NAVI_MODE_LEFT_W, 0.0),
            Point::new(NAVI_MODE_LEFT_W, height),
            2.0,
            theme.faint,
        )?;
        self.gfx.line(
            Point::new(NAVI_MODE_RIGHT_X, 0.0),
            Point::new(NAVI_MODE_RIGHT_X, height),
            2.0,
            theme.faint,
        )?;
        let media = media_frames(dashboard);
        let map = Rect::new(NAVI_MODE_MAP_X, 0.0, NAVI_MODE_MAP_W, height);
        self.gfx.rectangle(map, NAVI_MAP_BACKGROUND)?;
        if !self.draw_navi_media(frame(&media, "render:map_main"), map, MediaFit::default())? {
            self.draw_navi_route_fallback(navi, map)?;
        }
        if let Some(dashboard) = dashboard.filter(|v| v.map_stream_stalled) {
            self.draw_navi_map_stalled(map, dashboard.map_frame_age_ms)?;
        }
        self.draw_navi_map_media(&media)?;
        self.draw_navi_left_band(state, navi, &media)?;
        self.draw_navi_right_band(navi, dashboard, &media)
    }

    fn draw_navi_left_band(
        &mut self,
        state: &ClusterUiState,
        navi: Option<&NaviLiveState>,
        media: &MediaFrames<'_>,
    ) -> Result<()> {
        let theme = self.current_theme();
        let full = self.draw_navi_media(
            frame(media, "image:tbt_current_full"),
            Rect::new(10.0, 8.0, 500.0, 88.0),
            MediaFit::default(),
        )?;
        let compact = self.draw_navi_media(
            frame(media, "image:tbt_current_compact"),
            Rect::new(10.0, 104.0, 330.0, 145.0),
            MediaFit::default(),
        )?;
        let next = self.draw_navi_media(
            frame(media, "image:tbt_next"),
            Rect::new(350.0, 104.0, 160.0, 72.0),
            MediaFit::default(),
        )?;
        if let Some(current) = navi
            .and_then(|v| v.current.as_ref())
            .filter(|_| !(full || compact))
        {
            self.draw_navi_turn_icon(current.turn_type, 72.0, 78.0, 92.0)?;
            self.draw_text(
                &format_navi_distance(number(current.distance_m), self.is_metric),
                136.0,
                30.0,
                42.0,
                theme.text,
                Anchor::Left,
            )?;
            let text = self.ellipsize_text(guidance_text(current), 26.0, 350.0)?;
            self.draw_text(&text, 136.0, 82.0, 26.0, theme.text, Anchor::Left)?;
        }
        if let Some(next) = navi.and_then(|v| v.next.as_ref()).filter(|_| !next) {
            self.draw_navi_turn_icon(next.turn_type, 386.0, 143.0, 54.0)?;
            self.draw_text(
                &format_navi_distance(number(next.distance_m), self.is_metric),
                422.0,
                119.0,
                20.0,
                theme.text,
                Anchor::Left,
            )?;
        }
        let speed = display_speed(state.speed_kph, self.is_metric);
        let road_limit = state.speed_limit_kph.or_else(|| {
            navi.and_then(|v| v.speed.as_ref())
                .and_then(|v| v.road_limit_kph)
        });
        let road_name = navi
            .and_then(|v| v.vehicle.as_ref())
            .map_or("", |v| v.road_name.as_str());
        self.draw_text(
            &format!("{speed:.0}"),
            32.0,
            284.0,
            82.0,
            theme.text,
            Anchor::Left,
        )?;
        self.draw_text(
            speed_unit(self.is_metric),
            143.0,
            332.0,
            18.0,
            theme.muted,
            Anchor::Left,
        )?;
        if let Some(limit) = road_limit {
            self.gfx.circle(Point::new(218.0, 325.0), 36.0, WHITE)?;
            self.draw_navi_circle_stroke(Point::new(218.0, 325.0), 36.0, 6.0, RED)?;
            self.draw_text(
                &format!(
                    "{:.0}",
                    display_speed(number(limit), self.is_metric).round_ties_even()
                ),
                218.0,
                308.0,
                27.0,
                [20, 24, 28],
                Anchor::Center,
            )?;
        }
        if !road_name.is_empty() {
            let text = self.ellipsize_text(road_name, 22.0, 480.0)?;
            self.draw_text(&text, 20.0, 389.0, 22.0, theme.text, Anchor::Left)?;
        }
        if let Some(route) = navi.and_then(|v| v.route.as_ref()) {
            let mut text = format_navi_distance(number(route.remaining_distance_m), self.is_metric);
            if route.remaining_time_s > 0 {
                super::common::append_format(
                    &mut text,
                    format_args!("  {:.0} min", minutes(route.remaining_time_s)),
                );
            }
            self.draw_text(&text, 20.0, 431.0, 24.0, BLUE_SOFT, Anchor::Left)?;
        }
        Ok(())
    }

    fn draw_navi_map_media(&mut self, media: &MediaFrames<'_>) -> Result<()> {
        let expanded = frame(media, "image:crossroad_expanded").filter(|v| v.present);
        let minimized = frame(media, "image:crossroad_minimized").filter(|v| v.present);
        if expanded.is_some() {
            self.draw_navi_media(
                expanded,
                Rect::new(NAVI_MODE_MAP_X + 55.0, 20.0, 770.0, 430.0),
                MediaFit::default(),
            )?;
        } else if minimized.is_some() {
            self.draw_navi_media(
                minimized,
                Rect::new(
                    NAVI_MODE_MAP_X + NAVI_MODE_MAP_W - 260.0,
                    48.0,
                    250.0,
                    410.0,
                ),
                MediaFit::default(),
            )?;
        }
        let center = NAVI_MODE_MAP_X + NAVI_MODE_MAP_W * 0.5;
        for (key, rect) in [
            (
                "image:lane_bottom",
                Rect::new(
                    NAVI_MODE_MAP_X + 80.0,
                    298.0,
                    NAVI_MODE_MAP_W - 160.0,
                    174.0,
                ),
            ),
            (
                "image:traffic_signal",
                Rect::new(NAVI_MODE_MAP_X + 16.0, 18.0, 230.0, 98.0),
            ),
            (
                "image:center_tbt_icon",
                Rect::new(center - 112.0, 104.0, 92.0, 92.0),
            ),
            (
                "image:center_tbt_text",
                Rect::new(center - 12.0, 108.0, 220.0, 78.0),
            ),
            (
                "image:center_tbt_fee",
                Rect::new(center - 90.0, 188.0, 180.0, 58.0),
            ),
        ] {
            self.draw_navi_media(frame(media, key), rect, MediaFit::default())?;
        }
        Ok(())
    }
}
