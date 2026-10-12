use crate::config::{AMBER, GREEN, RED, WHITE};
use crate::models::{GitBranchStatus, GitBranchStatusState};
use crate::renderer::tokens::{
    CLUSTER_CORE_USAGE_MARGIN, CLUSTER_CORE_USAGE_MAX_TEXT_W, FPS_STATUS_DOT_RADIUS,
    FPS_STATUS_DOT_TEXT_GAP, FPS_STATUS_MAX_TEXT_W, GIT_STATUS_BOTTOM_MARGIN,
    GIT_STATUS_DOT_RADIUS, GIT_STATUS_DOT_TEXT_GAP, GIT_STATUS_MARGIN, GIT_STATUS_MAX_TEXT_W,
};
use crate::renderer::{Anchor, ClusterUiRenderer, Point, Result, StrokeStyle};

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font fitting, texture, and drawing resource errors.
    pub fn draw_git_status(
        &mut self,
        status: Option<&GitBranchStatus>,
        network: Option<&str>,
        fps: Option<f64>,
    ) -> Result<()> {
        if status.is_none() && network.is_none() {
            return Ok(());
        }
        let theme = self.current_theme();
        let height = self.measure_text("0", 20.0, Some(1.0))?.1;
        let row_height = height.max(GIT_STATUS_DOT_RADIUS * 2.0);
        let center_y = 480.0 - GIT_STATUS_BOTTOM_MARGIN - row_height * 0.5;
        let x = GIT_STATUS_MARGIN + GIT_STATUS_DOT_RADIUS * 2.0 + GIT_STATUS_DOT_TEXT_GAP;
        let style = StrokeStyle {
            color: [5, 9, 12, 255],
            width: 2,
            anchor: Anchor::Left,
            cache: true,
        };
        if let Some(status) = status {
            let color = match status.state {
                GitBranchStatusState::Ok => GREEN,
                GitBranchStatusState::Pull => AMBER,
                GitBranchStatusState::Missing => RED,
                GitBranchStatusState::Unknown => theme.muted,
            };
            let text = if status.detail.is_empty() {
                status.branch.clone()
            } else {
                format!("{} ({})", status.branch, status.detail)
            };
            let text = self.ellipsize_text(&text, 20.0, GIT_STATUS_MAX_TEXT_W)?;
            self.gfx.circle(
                Point::new(GIT_STATUS_MARGIN + GIT_STATUS_DOT_RADIUS, center_y),
                GIT_STATUS_DOT_RADIUS,
                color,
            )?;
            self.draw_text_with_stroke(&text, x, center_y, 20.0, WHITE, style)?;
        }
        if let Some(network) = network.filter(|network| !network.is_empty()) {
            let text = self.ellipsize_text(network, 20.0, GIT_STATUS_MAX_TEXT_W)?;
            let y = center_y - row_height - 4.0;
            self.draw_text_with_stroke(&text, x, y, 20.0, WHITE, style)?;
            let width = self.measure_text(&text, 20.0, Some(1.0))?.0;
            self.draw_actual_fps(fps, x + width + 18.0, y)?;
        }
        Ok(())
    }

    fn draw_actual_fps(&mut self, fps: Option<f64>, x: f64, y: f64) -> Result<()> {
        let Some(fps) = fps.filter(|fps| fps.is_finite()) else {
            return Ok(());
        };
        let text = self.ellipsize_text(&format!("FPS {fps:.1} Hz"), 20.0, FPS_STATUS_MAX_TEXT_W)?;
        self.gfx.circle(
            Point::new(x + FPS_STATUS_DOT_RADIUS, y),
            FPS_STATUS_DOT_RADIUS,
            GREEN,
        )?;
        self.draw_text_with_stroke(
            &text,
            x + FPS_STATUS_DOT_RADIUS * 2.0 + FPS_STATUS_DOT_TEXT_GAP,
            y,
            20.0,
            WHITE,
            StrokeStyle {
                color: [5, 9, 12, 255],
                width: 2,
                anchor: Anchor::Left,
                cache: false,
            },
        )
    }

    /// # Errors
    /// Returns font fitting and native drawing resource errors.
    pub fn draw_cluster_core_usage(&mut self, text: Option<&str>, right: f64) -> Result<()> {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return Ok(());
        };
        let text = self.ellipsize_text(text, 18.0, CLUSTER_CORE_USAGE_MAX_TEXT_W)?;
        let height = self.measure_text(&text, 18.0, Some(1.0))?.1;
        self.draw_text(
            &text,
            right - CLUSTER_CORE_USAGE_MARGIN,
            480.0 - CLUSTER_CORE_USAGE_MARGIN - height * 0.5,
            18.0,
            self.current_theme().muted,
            Anchor::Right,
        )
    }
}
