use super::policy::{primary_signal_seconds, signal_active, Signal};
use crate::models::NaviTrafficLightInfo;
use crate::renderer::tokens::{
    NAVI_TRAFFIC_BG_DARK, NAVI_TRAFFIC_BG_LIGHT, NAVI_TRAFFIC_BG_OUTLINE, NAVI_TRAFFIC_GREEN,
    NAVI_TRAFFIC_OFF_ARROW, NAVI_TRAFFIC_OFF_DARK, NAVI_TRAFFIC_OFF_LIGHT, NAVI_TRAFFIC_PANEL_H,
    NAVI_TRAFFIC_PANEL_PAD_X, NAVI_TRAFFIC_RED, NAVI_TRAFFIC_SIGNAL_GAP, NAVI_TRAFFIC_SIGNAL_SIZE,
    NAVI_TRAFFIC_TEXT_GAP,
};
use crate::renderer::{Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result, StrokeStyle};
use num_traits::ToPrimitive;

impl ClusterUiRenderer {
    /// # Errors
    /// Returns graphics, font, or media resource failures while drawing this source surface.
    pub fn draw_navi_traffic_light_panel(
        &mut self,
        traffic: &NaviTrafficLightInfo,
        panel_right: f64,
        y: f64,
        now: f64,
    ) -> Result<()> {
        let theme = self.current_theme();
        let h = NAVI_TRAFFIC_PANEL_H;
        let bg = if theme.is_dark {
            NAVI_TRAFFIC_BG_DARK
        } else {
            NAVI_TRAFFIC_BG_LIGHT
        };
        let off = if theme.is_dark {
            NAVI_TRAFFIC_OFF_DARK
        } else {
            NAVI_TRAFFIC_OFF_LIGHT
        };
        let remaining = countdown(traffic, now);
        let [red, left, uturn, straight, right] = remaining;
        let red_on = signal_active(Some(traffic), Signal::Red, red);
        let left_on = signal_active(Some(traffic), Signal::Left, left);
        let uturn_on = signal_active(Some(traffic), Signal::Uturn, uturn);
        let straight_on = signal_active(Some(traffic), Signal::Straight, straight);
        let right_on = signal_active(Some(traffic), Signal::Right, right);
        let use_uturn = (uturn_on || uturn.is_some()) && !(left_on || left.is_some());
        let (primary, primary_red) = primary_signal_seconds(Some(traffic), remaining);
        let remain_text = primary.map_or_else(|| "--".into(), |seconds| seconds.to_string());
        let remain_color = if primary_red {
            NAVI_TRAFFIC_RED
        } else {
            NAVI_TRAFFIC_GREEN
        };
        let remain_size = 54.0_f64;
        let (remain_width, _) = self.measure_text(
            &remain_text,
            remain_size,
            Some((remain_size * 0.02).max(1.0)),
        )?;
        let show_right = right_on || right.is_some();
        let slots = if show_right { 4.0 } else { 3.0 };
        let span = slots * NAVI_TRAFFIC_SIGNAL_SIZE + (slots - 1.0) * NAVI_TRAFFIC_SIGNAL_GAP;
        let w = (span + NAVI_TRAFFIC_TEXT_GAP + remain_width + NAVI_TRAFFIC_PANEL_PAD_X * 2.0)
            .max(270.0);
        let x = panel_right - w;
        let outline = if theme.is_dark {
            theme.faint
        } else {
            NAVI_TRAFFIC_BG_OUTLINE
        };
        self.rounded_rect(
            Rect::new(x + 5.0, y + 7.0, w, h),
            20.0,
            [0, 0, 0, 92],
            None,
            0.0,
        )?;
        self.rounded_rect(Rect::new(x, y, w, h), 20.0, bg, Some(outline.rgba()), 3.0)?;
        let slot_y = y + h * 0.5;
        let slot_x = x + NAVI_TRAFFIC_PANEL_PAD_X + NAVI_TRAFFIC_SIGNAL_SIZE * 0.5;
        let step = NAVI_TRAFFIC_SIGNAL_SIZE + NAVI_TRAFFIC_SIGNAL_GAP;
        self.draw_navi_round_signal(Point::new(slot_x, slot_y), red_on, NAVI_TRAFFIC_RED, off)?;
        self.draw_navi_turn_signal_slot(
            Point::new(slot_x + step, slot_y),
            if use_uturn { uturn_on } else { left_on },
            use_uturn,
            false,
            off,
        )?;
        self.draw_navi_round_signal(
            Point::new(slot_x + step * 2.0, slot_y),
            straight_on,
            NAVI_TRAFFIC_GREEN,
            off,
        )?;
        if show_right {
            self.draw_navi_turn_signal_slot(
                Point::new(slot_x + step * 3.0, slot_y),
                right_on,
                false,
                true,
                off,
            )?;
        }
        let remain_x =
            slot_x + NAVI_TRAFFIC_SIGNAL_SIZE * 0.5 + step * (slots - 1.0) + NAVI_TRAFFIC_TEXT_GAP;
        self.draw_text_with_stroke(
            &remain_text,
            remain_x,
            slot_y,
            remain_size,
            remain_color,
            StrokeStyle {
                color: [0, 0, 0, 255],
                width: 4,
                anchor: Anchor::Left,
                cache: false,
            },
        )
    }

    fn draw_navi_round_signal(
        &mut self,
        center: Point,
        active: bool,
        on: [u8; 3],
        off: [u8; 3],
    ) -> Result<()> {
        let radius = NAVI_TRAFFIC_SIGNAL_SIZE * if active { 12.5 / 26.0 } else { 0.5 };
        self.gfx
            .circle(center, radius, if active { on } else { off })?;
        if active {
            self.draw_navi_circle_stroke(
                center,
                radius,
                (NAVI_TRAFFIC_SIGNAL_SIZE / 26.0).max(1.6),
                [0, 0, 0],
            )?;
        }
        Ok(())
    }

    fn draw_navi_turn_signal_slot(
        &mut self,
        center: Point,
        active: bool,
        uturn: bool,
        right: bool,
        off: [u8; 3],
    ) -> Result<()> {
        let bg = if self.current_theme().is_dark {
            [0, 0, 0]
        } else {
            [34, 34, 34]
        };
        self.gfx.circle(
            center,
            NAVI_TRAFFIC_SIGNAL_SIZE * 0.5,
            if active { bg } else { off },
        )?;
        let color = if active {
            NAVI_TRAFFIC_GREEN
        } else {
            NAVI_TRAFFIC_OFF_ARROW
        };
        if uturn {
            self.draw_navi_uturn_arrow(center, color)
        } else {
            self.draw_navi_horizontal_arrow(center, color, right)
        }
    }
}

fn countdown(traffic: &NaviTrafficLightInfo, now: f64) -> [Option<i64>; 5] {
    let elapsed = traffic.meta.as_ref().map_or(0, |meta| {
        (now - meta.received_mono_s)
            .max(0.0)
            .to_i64()
            .unwrap_or(i64::MAX)
    });
    [
        traffic.red_s,
        traffic.left_s,
        traffic.uturn_s,
        traffic.straight_s,
        traffic.right_s,
    ]
    .map(|value| value.map(|seconds| seconds.saturating_sub(elapsed).max(0)))
}
