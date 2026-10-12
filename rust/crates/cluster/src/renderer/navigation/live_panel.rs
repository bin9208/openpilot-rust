use super::common::{frame, guidance_text, media_frames, minutes, number};
use crate::config::GREEN;
use crate::models::{ClusterUiState, NaviLaneInfo, NaviLiveState};
use crate::renderer::display::{display_speed, format_navi_distance};
use crate::renderer::tokens::{
    NAVI_LIVE_CONTENT_W, NAVI_LIVE_FOOTER_Y, NAVI_LIVE_ICON_SIZE, NAVI_LIVE_ICON_Y,
    NAVI_LIVE_LANE_Y, NAVI_LIVE_NEXT_Y, NAVI_LIVE_PANEL_H, NAVI_LIVE_PANEL_W, NAVI_LIVE_PANEL_X,
    NAVI_LIVE_PANEL_Y, NAVI_MAP_BACKGROUND,
};
use crate::renderer::{
    Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result, SystemPanelOptions,
};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns graphics, font, or media resource failures while drawing this source surface.
    pub fn draw_navi_live_panel(&mut self, state: &ClusterUiState, wall_time: f64) -> Result<()> {
        let navi = state.navi_live.as_ref();
        let dashboard = state.navi_dashboard.as_ref();
        let theme = self.current_theme();
        let panel = Rect::new(
            self.information_panel_x(NAVI_LIVE_PANEL_X),
            NAVI_LIVE_PANEL_Y,
            NAVI_LIVE_PANEL_W,
            NAVI_LIVE_PANEL_H,
        );
        if dashboard.is_some_and(|v| !v.connected) {
            return self.draw_system_stats_panel(
                state,
                SystemPanelOptions {
                    panel_x: Some(panel.x),
                    panel_y: panel.y,
                    panel_w: panel.width,
                    panel_h: Some(panel.height),
                    status_text: Some("NAVI DISCONNECTED"),
                    ..Default::default()
                },
            );
        }
        let map = Rect::new(
            panel.x + 3.0,
            panel.y + 3.0,
            panel.width - 6.0,
            panel.height - 6.0,
        );
        if let Some(dashboard) = dashboard.filter(|v| v.map_stream_stalled) {
            self.rounded_rect(
                panel,
                8.0,
                NAVI_MAP_BACKGROUND,
                Some(theme.faint.rgba()),
                2.0,
            )?;
            self.draw_navi_map_stalled(map, dashboard.map_frame_age_ms)?;
            return self.rounded_rect(panel, 8.0, [0, 0, 0, 0], Some(theme.faint.rgba()), 2.0);
        }
        let media = media_frames(dashboard);
        let map_frame = frame(&media, "render:map_main");
        let map_present = map_frame.is_some_and(|v| v.present);
        let background = if map_present {
            NAVI_MAP_BACKGROUND
        } else {
            theme.route_panel_bg.rgba()
        };
        self.rounded_rect(panel, 8.0, background, Some(theme.faint.rgba()), 2.0)?;
        if map_present {
            self.gfx.rectangle(map, NAVI_MAP_BACKGROUND)?;
            self.draw_navi_clipped_media(map_frame, map)?;
            self.rounded_rect(panel, 8.0, [0, 0, 0, 0], Some(theme.faint.rgba()), 2.0)?;
            self.draw_navi_crossroad_box(frame(&media, "image:crossroad_expanded"), panel)?;
            return self.draw_navi_live_map_overlays(navi, &media, panel, wall_time);
        }
        let Some(navi) = navi else {
            return Ok(());
        };
        self.draw_navi_live_text(navi, panel)?;
        if let Some(lane) = navi
            .lane_current
            .as_ref()
            .or_else(|| navi.lane_ahead.first())
            .filter(|v| v.visible)
        {
            self.draw_navi_lane_strip(lane)?;
        }
        self.draw_navi_live_footer(navi, panel)
    }

    fn draw_navi_live_text(&mut self, navi: &NaviLiveState, panel: Rect) -> Result<()> {
        let theme = self.current_theme();
        let x = panel.x;
        let y = panel.y;
        if let Some(current) = &navi.current {
            self.draw_navi_turn_icon(
                current.turn_type,
                x + 72.0,
                NAVI_LIVE_ICON_Y,
                NAVI_LIVE_ICON_SIZE,
            )?;
            self.draw_text(
                &format_navi_distance(number(current.distance_m), self.is_metric),
                x + 136.0,
                y + 26.0,
                38.0,
                theme.text,
                Anchor::Left,
            )?;
            let main = guidance_text(current);
            let text = self.ellipsize_text(main, 25.0, NAVI_LIVE_CONTENT_W)?;
            self.draw_text(&text, x + 136.0, y + 75.0, 25.0, theme.text, Anchor::Left)?;
            let detail = if !current.road_name.is_empty() && current.road_name != main {
                &current.road_name
            } else {
                &current.near_direction
            };
            if !detail.is_empty() {
                let text = self.ellipsize_text(detail, 18.0, NAVI_LIVE_CONTENT_W)?;
                self.draw_text(&text, x + 136.0, y + 112.0, 18.0, theme.muted, Anchor::Left)?;
            }
        } else if let Some(vehicle) = navi.vehicle.as_ref().filter(|v| !v.road_name.is_empty()) {
            let text = self.ellipsize_text(&vehicle.road_name, 28.0, panel.width - 48.0)?;
            self.draw_text(&text, x + 24.0, y + 34.0, 28.0, theme.text, Anchor::Left)?;
        }
        if let Some(next) = &navi.next {
            self.gfx.line(
                Point::new(x + 20.0, NAVI_LIVE_NEXT_Y - 12.0),
                Point::new(x + panel.width - 20.0, NAVI_LIVE_NEXT_Y - 12.0),
                1.5,
                theme.faint,
            )?;
            self.draw_navi_turn_icon(next.turn_type, x + 46.0, NAVI_LIVE_NEXT_Y + 22.0, 44.0)?;
            self.draw_text(
                &format_navi_distance(number(next.distance_m), self.is_metric),
                x + 82.0,
                NAVI_LIVE_NEXT_Y - 1.0,
                24.0,
                theme.text,
                Anchor::Left,
            )?;
            let text = self.ellipsize_text(guidance_text(next), 18.0, panel.width - 180.0)?;
            self.draw_text(
                &text,
                x + 180.0,
                NAVI_LIVE_NEXT_Y + 5.0,
                18.0,
                theme.muted,
                Anchor::Left,
            )?;
        }
        Ok(())
    }

    fn draw_navi_live_footer(&mut self, navi: &NaviLiveState, panel: Rect) -> Result<()> {
        let mut parts = Vec::new();
        if let Some(route) = navi.route.as_ref().filter(|v| v.remaining_distance_m > 0) {
            let mut text = format_navi_distance(number(route.remaining_distance_m), self.is_metric);
            if route.remaining_time_s > 0 {
                super::common::append_format(
                    &mut text,
                    format_args!(" / {:.0} min", minutes(route.remaining_time_s)),
                );
            }
            parts.push(text);
        }
        if let Some(speed) = &navi.speed {
            for (label, kind, limit, distance) in [
                (
                    "SDI",
                    speed.sdi_type,
                    speed.sdi_speed_limit_kph,
                    speed.sdi_distance_m,
                ),
                (
                    "SDI2",
                    speed.secondary_sdi_type,
                    speed.secondary_sdi_speed_limit_kph,
                    speed.secondary_sdi_distance_m,
                ),
            ] {
                if kind.is_none() {
                    continue;
                }
                let mut text = label.to_owned();
                if let Some(limit) = limit.filter(|v| *v != 0) {
                    super::common::append_format(
                        &mut text,
                        format_args!(" {:.0}", display_speed(number(limit), self.is_metric)),
                    );
                }
                if let Some(distance) = distance {
                    super::common::append_format(
                        &mut text,
                        format_args!(
                            " / {}",
                            format_navi_distance(number(distance), self.is_metric)
                        ),
                    );
                }
                parts.push(text);
            }
        }
        if let Some(crossroad) = navi.crossroad.as_ref().filter(|v| v.visible) {
            parts.push(format!(
                "JCT {}",
                format_navi_distance(number(crossroad.distance_m), self.is_metric)
            ));
        }
        if navi.status.as_ref().is_some_and(|v| v.off_route) {
            parts.push(self.text("off_route").to_owned());
        }
        if !parts.is_empty() {
            let text = self.ellipsize_text(&parts.join("   "), 18.0, panel.width - 40.0)?;
            let theme = self.current_theme();
            self.draw_text(
                &text,
                panel.x + 20.0,
                NAVI_LIVE_FOOTER_Y,
                18.0,
                theme.muted,
                Anchor::Left,
            )?;
        }
        Ok(())
    }

    fn draw_navi_lane_strip(&mut self, lane: &NaviLaneInfo) -> Result<()> {
        let theme = self.current_theme();
        let count = lane
            .count
            .max(i64::try_from(lane.available.len()).unwrap_or(i64::MAX))
            .max(i64::try_from(lane.turn_info.len()).unwrap_or(i64::MAX))
            .min(8);
        if count <= 0 {
            return Ok(());
        }
        let mut x = self.information_panel_x(NAVI_LIVE_PANEL_X) + 22.0;
        let available_w = NAVI_LIVE_PANEL_W - 44.0;
        let gap = 6.0;
        let cell_w = ((available_w - gap * number(count - 1)) / number(count)).min(46.0);
        let total_w = cell_w * number(count) + gap * number(count - 1);
        x += (available_w - total_w) * 0.5;
        let y = NAVI_LIVE_LANE_Y;
        let current = if (1..=count).contains(&lane.current_lane) {
            lane.current_lane - 1
        } else {
            lane.current_lane
        };
        for index in 0..count {
            let available = usize::try_from(index)
                .ok()
                .and_then(|index| lane.available.get(index))
                .is_some_and(|v| *v > 0);
            let active = index == current;
            let fill = if available {
                [35, 92, 58, 230]
            } else {
                theme.panel_bg.rgba()
            };
            let outline = if active {
                GREEN
            } else if available {
                [102, 194, 132]
            } else {
                theme.faint
            };
            self.rounded_rect(
                Rect::new(x, y, cell_w, 46.0),
                4.0,
                fill,
                Some(outline.rgba()),
                if active { 3.0 } else { 1.5 },
            )?;
            let center = x + cell_w * 0.5;
            let arrow = if available { GREEN } else { theme.muted };
            self.draw_navi_round_line(
                Point::new(center, y + 33.0),
                Point::new(center, y + 14.0),
                3.0,
                arrow,
            )?;
            self.gfx.triangle(
                Point::new(center, y + 8.0),
                Point::new(center - 5.0, y + 16.0),
                Point::new(center + 5.0, y + 16.0),
                arrow,
            )?;
            x += cell_w + gap;
        }
        Ok(())
    }
}
