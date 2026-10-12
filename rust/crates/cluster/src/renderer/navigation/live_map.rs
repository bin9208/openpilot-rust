use super::common::{frame, minutes, native_rect, number, MediaFrames};
use crate::config::{DESIGN_WIDTH, WHITE};
use crate::models::{NaviLiveState, NaviMediaFrame};
use crate::renderer::display::format_navi_distance;
use crate::renderer::tokens::{NAVI_LIVE_GUIDANCE_MEDIA_SCALE, NAVI_LIVE_PANEL_W};
use crate::renderer::{
    Anchor, ClusterUiRenderer, Error, IntoColor, MediaFit, Rect, Result, StrokeStyle,
};
use num_traits::ToPrimitive;

impl ClusterUiRenderer {
    pub(super) fn draw_navi_clipped_media(
        &mut self,
        frame: Option<&NaviMediaFrame>,
        rect: Rect,
    ) -> Result<()> {
        let clip = Rect::new(
            rect.x.round_ties_even(),
            rect.y.round_ties_even(),
            rect.width.round_ties_even(),
            rect.height.round_ties_even(),
        );
        self.gfx.begin_scissor(clip)?;
        let draw = self.draw_navi_media(
            frame,
            rect,
            MediaFit {
                cover: true,
                ..Default::default()
            },
        );
        let end = self.gfx.end_scissor();
        draw?;
        end
    }

    pub(super) fn draw_navi_crossroad_box(
        &mut self,
        frame: Option<&NaviMediaFrame>,
        panel: Rect,
    ) -> Result<()> {
        let Some(frame) = frame.filter(|v| v.present && v.width > 0 && v.height > 0) else {
            return Ok(());
        };
        let theme = self.current_theme();
        let height = panel.height * 0.70;
        let aspect = number(frame.width) / number(frame.height).max(1.0);
        let swapped = self.panel_swap_active();
        let mut x = if swapped {
            panel.x + NAVI_LIVE_PANEL_W
        } else {
            panel.x
        };
        let available = if swapped {
            number(DESIGN_WIDTH) - x - 4.0
        } else {
            panel.x - 830.0
        };
        let width = (height * aspect).clamp(320.0, available.max(320.0));
        if !swapped {
            x -= width;
        }
        let rect = Rect::new(x, panel.y, width, height);
        self.rounded_rect(
            rect,
            8.0,
            theme.route_panel_bg,
            Some(theme.faint.rgba()),
            2.0,
        )?;
        self.draw_navi_clipped_media(
            Some(frame),
            Rect::new(x + 4.0, panel.y + 4.0, width - 8.0, height - 8.0),
        )?;
        self.rounded_rect(rect, 8.0, [0, 0, 0, 0], Some(theme.faint.rgba()), 2.0)
    }

    pub(super) fn draw_navi_live_map_overlays(
        &mut self,
        navi: Option<&NaviLiveState>,
        media: &MediaFrames<'_>,
        panel: Rect,
        wall_time: f64,
    ) -> Result<()> {
        let x = panel.x;
        let y = panel.y;
        let w = panel.width;
        let h = panel.height;
        let scale = NAVI_LIVE_GUIDANCE_MEDIA_SCALE;
        let current = frame(media, "image:tbt_current_compact")
            .filter(|v| v.present)
            .or_else(|| frame(media, "image:tbt_current_full"));
        let current_rect =
            native_rect(Rect::new(x + 12.0, y + 12.0, 310.0 * scale, 116.0 * scale))?;
        let current_size = self.navi_media_fitted_size(current, current_rect, false)?;
        let left_top = MediaFit {
            align_x: 0.0,
            align_y: 0.0,
            ..Default::default()
        };
        self.draw_navi_media(current, current_rect, left_top)?;
        let next_y = current_rect.y + (current_size.map_or(0.0, |v| v.1) - 24.0 * scale).max(0.0);
        self.draw_navi_media(
            frame(media, "image:tbt_next"),
            native_rect(Rect::new(x + 12.0, next_y, 190.0 * scale, 68.0 * scale))?,
            left_top,
        )?;
        let center_x = x + w * 0.5;
        let icon = Rect::new(center_x - 55.0, y + 76.0, 110.0, 110.0);
        self.draw_navi_media(
            frame(media, "image:center_tbt_icon"),
            icon,
            MediaFit::default(),
        )?;
        self.draw_navi_media(
            frame(media, "image:center_tbt_text"),
            Rect::new(center_x - 110.0, icon.y + icon.height + 2.0, 220.0, 78.0),
            MediaFit::default(),
        )?;
        self.draw_navi_media(
            frame(media, "image:traffic_signal"),
            Rect::new(x + w - 242.0, y + 12.0, 230.0, 98.0),
            MediaFit {
                align_x: 1.0,
                align_y: 0.0,
                ..Default::default()
            },
        )?;
        let safety = [
            "image:safety_primary",
            "image:safety_secondary",
            "image:safety_section",
        ]
        .into_iter()
        .find_map(|key| frame(media, key).filter(|v| v.present));
        self.draw_navi_media(
            safety,
            native_rect(Rect::new(x + 10.0, y + h * 0.62 - 50.0, 110.0, 100.0))?,
            MediaFit {
                align_x: 0.0,
                ..Default::default()
            },
        )?;
        let lane_w = 226.0 * scale;
        let lane_h = 67.0 * scale;
        let lane_rect = native_rect(Rect::new(
            x + (w - lane_w) * 0.5,
            y + h - lane_h - 6.0,
            lane_w,
            lane_h,
        ))?;
        let lane = frame(media, "image:lane_bottom");
        let lane_size = self.navi_media_fitted_size(lane, lane_rect, false)?;
        self.draw_navi_media(lane, lane_rect, MediaFit::default())?;
        let (left, right) =
            lane_size.map_or((lane_rect.x, lane_rect.x + lane_rect.width), |size| {
                let left = lane_rect.x + (lane_rect.width - size.0) * 0.5;
                (left, left + size.0)
            });
        self.draw_navi_map_route_labels(navi, panel, (left, right), wall_time)
    }

    fn draw_navi_map_route_labels(
        &mut self,
        navi: Option<&NaviLiveState>,
        panel: Rect,
        lane_edges: (f64, f64),
        wall_time: f64,
    ) -> Result<()> {
        let (lane_left, lane_right) = lane_edges;
        let stroke = StrokeStyle {
            color: [10, 13, 16, 255],
            width: 2,
            anchor: Anchor::Left,
            cache: false,
        };
        if let Some(route) = navi.and_then(|v| v.route.as_ref()) {
            let left = panel.x + 14.0;
            let width = lane_left - left - 14.0;
            let distance = format_navi_distance(number(route.remaining_distance_m), self.is_metric);
            let arrival_seconds = (wall_time + number(route.remaining_time_s.max(0)))
                .floor()
                .to_i64()
                .ok_or(Error::Contract("navigation arrival time out of range"))?;
            let arrival = chrono::DateTime::from_timestamp(arrival_seconds, 0)
                .ok_or(Error::Contract("navigation arrival time out of range"))?
                .with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string();
            let text = self.ellipsize_text(&distance, 25.0, width)?;
            self.draw_text_with_stroke(
                &text,
                left,
                panel.y + panel.height - 48.0,
                25.0,
                WHITE,
                stroke,
            )?;
            let text = self.ellipsize_text(
                &format!("{arrival}  {:.0} min", minutes(route.remaining_time_s)),
                22.0,
                width,
            )?;
            self.draw_text_with_stroke(
                &text,
                left,
                panel.y + panel.height - 19.0,
                22.0,
                WHITE,
                stroke,
            )?;
        }
        let name = navi
            .and_then(|v| v.vehicle.as_ref())
            .map_or("", |v| v.road_name.as_str());
        let right = panel.x + panel.width - 14.0;
        let width = right - lane_right - 14.0;
        if !name.is_empty() && width >= 70.0 {
            let text = self.ellipsize_text(name, 20.0, width)?;
            self.draw_text_with_stroke(
                &text,
                right,
                panel.y + panel.height - 25.0,
                20.0,
                WHITE,
                StrokeStyle {
                    anchor: Anchor::Right,
                    cache: true,
                    ..stroke
                },
            )?;
        }
        Ok(())
    }
}
