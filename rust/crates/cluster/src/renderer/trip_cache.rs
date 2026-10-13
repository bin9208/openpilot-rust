use super::{
    graphics::scalar,
    tokens::{
        TRIP_REPORT_CACHE_REFRESH_SECONDS, TRIP_REPORT_PANEL_H, TRIP_REPORT_PANEL_W,
        TRIP_REPORT_PANEL_X, TRIP_REPORT_PANEL_Y,
    },
    ClusterUiRenderer, Point, Rect, Result,
};
use crate::{config::ClusterTheme, models::ClusterUiState};
use num_traits::ToPrimitive;
use openpilot_startup_ui::renderer::{RenderTarget, TargetPaint, TextureFilter};

#[derive(Clone, PartialEq)]
struct Key {
    theme: ClusterTheme,
    language: String,
    metric: bool,
    layout: i64,
    width: i32,
    height: i32,
}
#[derive(Default)]
pub(super) struct TripCache {
    target: Option<RenderTarget>,
    key: Option<Key>,
    valid: bool,
    visible: bool,
    next_refresh: f64,
}
impl TripCache {
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.next_refresh = 0.0;
    }
    pub fn close(&mut self) {
        *self = Self::default();
    }
}
impl ClusterUiRenderer {
    /// # Errors
    /// Returns target allocation or trip-panel drawing errors.
    pub fn prepare_trip_report_cache(&mut self, state: &ClusterUiState, now: f64) -> Result<()> {
        if self.effective_screen_mode(state) != 5 {
            self.trip_cache.visible = false;
            return Ok(());
        }
        let key = Key {
            theme: self.current_theme(),
            language: self.language.clone(),
            metric: self.is_metric,
            layout: self.panel_layout,
            width: self.options.width,
            height: self.options.height,
        };
        let refresh = !self.trip_cache.visible
            || !self.trip_cache.valid
            || self.trip_cache.target.is_none()
            || self.trip_cache.key.as_ref() != Some(&key)
            || now >= self.trip_cache.next_refresh;
        if refresh {
            let start_timer = self.profile_start();
            self.refresh_trip_report_cache(state)?;
            self.trip_cache.key = Some(key);
            self.trip_cache.valid = true;
            self.trip_cache.next_refresh = now + TRIP_REPORT_CACHE_REFRESH_SECONDS;
            self.profile_add("trip_report_cache.refresh", start_timer);
        }
        self.trip_cache.visible = true;
        Ok(())
    }
    fn refresh_trip_report_cache(&mut self, state: &ClusterUiState) -> Result<()> {
        let target = if let Some(target) = self.trip_cache.target.take() {
            target
        } else {
            let width = TRIP_REPORT_PANEL_W
                .round_ties_even()
                .to_i32()
                .ok_or(super::Error::Contract("trip target width invalid"))?;
            let height = TRIP_REPORT_PANEL_H
                .round_ties_even()
                .to_i32()
                .ok_or(super::Error::Contract("trip target height invalid"))?;
            let target = self.gfx.backend()?.render_target((width, height))?;
            self.gfx
                .backend()?
                .target_filter(&target, TextureFilter::Bilinear)?;
            target
        };
        let panel_x = self.information_panel_x(TRIP_REPORT_PANEL_X);
        self.gfx.backend()?.begin_target(&target)?;
        let mut pushed = false;
        let draw = (|| -> Result<()> {
            self.gfx.clear([0, 0, 0, 0])?;
            self.gfx.push_matrix()?;
            pushed = true;
            self.gfx.translate(-panel_x, -TRIP_REPORT_PANEL_Y, 0.0)?;
            self.draw_trip_report_panel_contents(state)
        })();
        let pop = if pushed {
            self.gfx.pop_matrix()
        } else {
            Ok(())
        };
        self.gfx.backend()?.end_target();
        self.trip_cache.target = Some(target);
        draw?;
        pop
    }
    /// # Errors
    /// Returns cached-target or direct trip-panel drawing errors.
    pub fn draw_trip_report_panel(&mut self, state: &ClusterUiState) -> Result<()> {
        if !self.trip_cache.valid || self.trip_cache.target.is_none() {
            return self.draw_trip_report_panel_contents(state);
        }
        let x = self.information_panel_x(TRIP_REPORT_PANEL_X);
        let target = self
            .trip_cache
            .target
            .as_ref()
            .ok_or(super::Error::Contract("trip target missing"))?;
        let backend = self
            .gfx
            .native
            .as_mut()
            .ok_or(super::Error::Contract("cluster window not open"))?;
        Ok(backend.target_texture(
            target,
            TargetPaint {
                source: openpilot_startup_ui::geometry::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: scalar(f64::from(target.width))?,
                    height: scalar(-f64::from(target.height))?,
                },
                destination: super::graphics::rect(Rect::new(
                    x,
                    TRIP_REPORT_PANEL_Y,
                    TRIP_REPORT_PANEL_W,
                    TRIP_REPORT_PANEL_H,
                ))?,
                origin: super::graphics::point(Point::default())?,
                rotation: 0.0,
                tint: u32::MAX,
            },
        )?)
    }
}
