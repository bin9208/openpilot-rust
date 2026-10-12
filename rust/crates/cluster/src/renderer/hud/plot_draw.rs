use crate::config::GREEN;
use crate::models::DebugPlotSnapshot;
use crate::renderer::{with_alpha, Anchor, ClusterUiRenderer, IntoColor, Point, Rect, Result};
use crate::scene::math::clamp;
use num_traits::ToPrimitive;

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font fitting and native graphics resource errors.
    pub fn draw_debug_plot(
        &mut self,
        plot: Option<&DebugPlotSnapshot>,
        bounds: Rect,
        now: f64,
    ) -> Result<()> {
        self.hud.plot.update(plot, now);
        let plot = plot.filter(|plot| plot.mode > 0);
        let title = plot.map_or("SHOW PLOT MODE 0", |plot| plot.title.as_str());
        self.draw_debug_plot_panel(title, plot.is_some(), bounds)
    }
    fn draw_debug_plot_panel(&mut self, title: &str, available: bool, bounds: Rect) -> Result<()> {
        let theme = self.current_theme();
        let compact = bounds.width < 700.0;
        let pad = if compact { 18.0 } else { 24.0 };
        let inner = Rect::new(
            bounds.x + pad,
            bounds.y + if compact { 74.0 } else { 70.0 },
            bounds.width - pad * 2.0,
            bounds.height - if compact { 100.0 } else { 96.0 },
        );
        self.rounded_rect(
            bounds,
            18.0,
            theme.route_panel_bg,
            Some(theme.faint.rgba()),
            2.0,
        )?;
        let title_size = if compact { 18.0 } else { 22.0 };
        let title = self.ellipsize_text(
            title,
            title_size,
            bounds.width - pad * 2.0 - if compact { 120.0 } else { 190.0 },
        )?;
        self.draw_text(
            &title,
            bounds.x + pad,
            bounds.y + 30.0,
            title_size,
            theme.text,
            Anchor::Left,
        )?;
        let (minimum, maximum) = self.hud.plot.bounds();
        self.draw_text(
            &format!("min {minimum:.2}  max {maximum:.2}"),
            bounds.x + bounds.width - pad,
            bounds.y + 30.0,
            if compact { 13.0 } else { 17.0 },
            theme.muted,
            Anchor::Right,
        )?;
        self.draw_plot_grid(inner, minimum, maximum)?;
        if !available || self.hud.plot.len() < 2 {
            return self.draw_text(
                "no plot data",
                inner.x + inner.width * 0.5,
                inner.y + inner.height * 0.5,
                22.0,
                theme.muted,
                Anchor::Center,
            );
        }
        for (series, color) in [[255, 220, 0], GREEN, [255, 165, 0]]
            .into_iter()
            .enumerate()
        {
            self.draw_debug_plot_series(series, inner, color)?;
        }
        Ok(())
    }
    fn draw_plot_grid(&mut self, inner: Rect, minimum: f64, maximum: f64) -> Result<()> {
        let theme = self.current_theme();
        self.gfx
            .rectangle(inner, [0, 0, 0, if theme.is_dark { 52 } else { 30 }])?;
        self.gfx.rectangle_lines_ex(inner, 2.0, theme.faint)?;
        let grid = with_alpha(theme.faint, 110);
        let bottom = inner.y + inner.height;
        for index in 1..6 {
            let x = inner.x + inner.width * f64::from(index) / 6.0;
            self.gfx
                .line(Point::new(x, inner.y), Point::new(x, bottom), 1.0, grid)?;
        }
        for index in 1..4 {
            let y = inner.y + inner.height * f64::from(index) / 4.0;
            self.gfx.line(
                Point::new(inner.x, y),
                Point::new(inner.x + inner.width, y),
                1.0,
                grid,
            )?;
        }
        let range = maximum - minimum;
        if minimum < 0.0 && 0.0 < maximum && range > 0.001 {
            let y = bottom - (0.0 - minimum) / range * inner.height;
            self.gfx.line(
                Point::new(inner.x, y),
                Point::new(inner.x + inner.width, y),
                2.0,
                with_alpha(theme.muted, 160),
            )?;
        }
        Ok(())
    }
    fn draw_debug_plot_series(
        &mut self,
        series: usize,
        bounds: Rect,
        color: [u8; 3],
    ) -> Result<()> {
        let (minimum, maximum) = self.hud.plot.bounds();
        let range = (maximum - minimum).max(0.001);
        let count = self.hud.plot.len();
        let dx = bounds.width / count.saturating_sub(1).max(1).to_f64().unwrap_or(1.0);
        let mut previous = None;
        let mut latest = None;
        for offset in 0..count {
            let value = self.hud.plot.sample_at(offset)[series];
            let x = bounds.x + dx * offset.to_f64().unwrap_or(0.0);
            let y = bounds.y + bounds.height - (value - minimum) / range * bounds.height;
            let native = crate::renderer::graphics::point(Point::new(x, y))?;
            let point = Point::new(f64::from(native.x), f64::from(native.y));
            if let Some(previous) = previous {
                self.gfx.line(previous, point, 3.0, color)?;
            }
            previous = Some(point);
            latest = Some((point, value));
        }
        let Some((point, value)) = latest else {
            return Ok(());
        };
        let x = (bounds.x + bounds.width - 4.0).min(point.x + 42.0);
        let y = clamp(
            point.y + if series > 0 { 24.0 } else { 0.0 },
            bounds.y + 12.0,
            bounds.y + bounds.height - 12.0,
        );
        self.draw_text(&format!("{value:.2}"), x, y, 18.0, color, Anchor::Right)
    }
}
