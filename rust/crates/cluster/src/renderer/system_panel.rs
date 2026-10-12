use super::{
    tokens::{NAVI_MAP_BACKGROUND, SYSTEM_PANEL_X},
    Anchor, ClusterUiRenderer, Rect, Result, SystemPanelOptions, SystemStats,
};
use crate::{
    config::{cluster_camera_view_is_road_camera, AMBER, BLUE, RED, WHITE},
    models::ClusterUiState,
};
use num_traits::ToPrimitive;

#[derive(Clone, Copy)]
struct CoreGrid {
    columns: usize,
    header_height: f64,
    row_height: f64,
}

impl ClusterUiRenderer {
    /// # Errors
    /// Returns font or graphics errors while drawing the stats snapshot.
    pub fn draw_system_stats_panel(
        &mut self,
        state: &ClusterUiState,
        options: SystemPanelOptions<'_>,
    ) -> Result<()> {
        let x = options
            .panel_x
            .unwrap_or_else(|| self.information_panel_x(SYSTEM_PANEL_X));
        let (y, width) = (options.panel_y, options.panel_w);
        let theme = self.current_theme();
        let snapshot = options
            .stats
            .cloned()
            .unwrap_or_else(|| self.stats.sample(self.started.elapsed().as_secs_f64()));
        let disconnected = options.status_text.is_some();
        let detailed = disconnected || options.show_runtime_info;
        let count = snapshot.cpu_core_percents.len();
        let columns = if count <= 8 { 2 } else { 4 };
        let rows = count.max(1).div_ceil(columns).max(1);
        let header_height = if detailed { 216.0 } else { 122.0 };
        let height = options.panel_h.unwrap_or_else(|| {
            (480.0 - y - 18.0)
                .min(header_height + number(rows) * if columns == 2 { 30.0 } else { 24.0 } + 18.0)
        });
        let row_height = (if columns == 2 { 30.0_f64 } else { 24.0 })
            .min((height - header_height - 14.0).max(24.0) / number(rows));
        let background = if disconnected {
            NAVI_MAP_BACKGROUND
        } else {
            [
                theme.route_panel_bg[0],
                theme.route_panel_bg[1],
                theme.route_panel_bg[2],
                255,
            ]
        };
        let outline = if disconnected {
            [82, 92, 100, 255]
        } else {
            [theme.faint[0], theme.faint[1], theme.faint[2], 255]
        };
        let text_color = if disconnected { WHITE } else { theme.text };
        let muted = if disconnected {
            [154, 164, 172]
        } else {
            theme.muted
        };
        self.rounded_rect(
            Rect::new(x, y, width, height),
            18.0,
            background,
            Some(outline),
            2.0,
        )?;
        self.draw_text(
            options.title_text,
            x + 24.0,
            y + 28.0,
            18.0,
            muted,
            Anchor::Left,
        )?;
        if let Some(status) = options.status_text.filter(|value| !value.is_empty()) {
            self.draw_text(
                status,
                x + width - 24.0,
                y + 28.0,
                18.0,
                AMBER,
                Anchor::Right,
            )?;
        }
        if detailed {
            self.system_runtime_rows(state, Rect::new(x, y, width, height), [text_color, muted])?;
        }
        self.system_memory_rows(
            &snapshot,
            Rect::new(x, y, width, height),
            detailed,
            [text_color, muted],
        )?;
        self.system_cpu_rows(
            &snapshot,
            Rect::new(x, y, width, height),
            detailed,
            CoreGrid {
                columns,
                header_height,
                row_height,
            },
            muted,
        )
    }
    fn system_cpu_rows(
        &mut self,
        snapshot: &SystemStats,
        bounds: Rect,
        detailed: bool,
        grid: CoreGrid,
        muted: [u8; 3],
    ) -> Result<()> {
        let (x, y, width) = (bounds.x, bounds.y, bounds.width);
        let CoreGrid {
            columns,
            header_height,
            row_height,
        } = grid;
        let count = snapshot.cpu_core_percents.len();
        let cpu_y = y + if detailed { 208.0 } else { 104.0 };
        self.draw_text("CPU CORE %", x + 24.0, cpu_y, 15.0, muted, Anchor::Left)?;
        if count == 0 {
            self.draw_text(
                "unavailable",
                x + width - 24.0,
                cpu_y,
                15.0,
                muted,
                Anchor::Right,
            )?;
            return Ok(());
        }
        let gap = if columns == 2 { 18.0 } else { 10.0 };
        let cell_width = (width - 48.0 - gap * number(columns - 1)) / number(columns);
        for (index, percent) in snapshot.cpu_core_percents.iter().copied().enumerate() {
            let cell_x = x + 24.0 + number(index % columns) * (cell_width + gap);
            let line_y = y + header_height + number(index / columns) * row_height;
            let tint = self.system_metric_color(percent);
            let size = if columns == 2 { 15.0 } else { 12.0 };
            self.draw_text(
                &format!("C{index}"),
                cell_x,
                line_y + 8.0,
                size,
                muted,
                Anchor::Left,
            )?;
            self.draw_text(
                &percent_text(percent),
                cell_x + cell_width,
                line_y + 8.0,
                size,
                tint,
                Anchor::Right,
            )?;
            self.draw_percent_bar(
                Rect::new(cell_x, line_y + 19.0, cell_width, 6.0),
                percent,
                tint,
            )?;
        }
        Ok(())
    }
    fn system_runtime_rows(
        &mut self,
        state: &ClusterUiState,
        bounds: Rect,
        colors: [[u8; 3]; 2],
    ) -> Result<()> {
        let (x, y, width) = (bounds.x, bounds.y, bounds.width);
        let [text_color, muted] = colors;
        let fps = state
            .actual_fps
            .filter(|value| value.is_finite())
            .map_or_else(|| "-- Hz".into(), |value| format!("{value:.1} Hz"));
        let camera = if state.onroad {
            if cluster_camera_view_is_road_camera(state.camera_view_mode) {
                "ROAD CAMERA"
            } else {
                "ONROAD"
            }
        } else {
            "STANDBY"
        };
        let network = state
            .network_address
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("unavailable");
        let display = format!("{} x {}  /  {fps}", self.options.width, self.options.height);
        for (index, (label, value)) in [
            ("NETWORK", network),
            ("DISPLAY", display.as_str()),
            ("CAMERA", camera),
        ]
        .into_iter()
        .enumerate()
        {
            let line_y = y + 66.0 + number(index) * 30.0;
            self.draw_text(label, x + 24.0, line_y, 16.0, muted, Anchor::Left)?;
            self.draw_text(
                value,
                x + width - 24.0,
                line_y,
                17.0,
                text_color,
                Anchor::Right,
            )?;
        }
        Ok(())
    }
    fn system_memory_rows(
        &mut self,
        snapshot: &SystemStats,
        bounds: Rect,
        detailed: bool,
        colors: [[u8; 3]; 2],
    ) -> Result<()> {
        let (x, y, width) = (bounds.x, bounds.y, bounds.width);
        let [text_color, muted] = colors;
        let mem_color = self.system_metric_color(snapshot.memory_used_percent);
        let mem_y = y + if detailed { 164.0 } else { 62.0 };
        self.draw_text("MEM", x + 24.0, mem_y, 17.0, muted, Anchor::Left)?;
        self.draw_text(
            &memory_text(snapshot)?,
            x + 86.0,
            mem_y,
            17.0,
            if snapshot.memory_used_bytes.is_some() {
                text_color
            } else {
                muted
            },
            Anchor::Left,
        )?;
        self.draw_text(
            &percent_text(snapshot.memory_used_percent),
            x + width - 24.0,
            mem_y,
            17.0,
            mem_color,
            Anchor::Right,
        )?;
        self.draw_percent_bar(
            Rect::new(
                x + 24.0,
                y + if detailed { 182.0 } else { 80.0 },
                width - 48.0,
                12.0,
            ),
            snapshot.memory_used_percent,
            mem_color,
        )?;
        Ok(())
    }
    pub(super) fn system_metric_color(&self, percent: Option<f64>) -> [u8; 3] {
        match percent {
            None => self.current_theme().muted,
            Some(value) if value >= 85.0 => RED,
            Some(value) if value >= 60.0 => AMBER,
            Some(_) => BLUE,
        }
    }
    pub(super) fn draw_percent_bar(
        &mut self,
        rect: Rect,
        percent: Option<f64>,
        fill: [u8; 3],
    ) -> Result<()> {
        self.rounded_rect(
            rect,
            rect.height * 0.5,
            self.current_theme().gauge_bg,
            None,
            1.0,
        )?;
        let Some(percent) = percent else {
            return Ok(());
        };
        let ratio = percent.clamp(0.0, 100.0) / 100.0;
        if ratio > 0.0 {
            self.rounded_rect(
                Rect {
                    width: (rect.width * ratio).max(2.0),
                    ..rect
                },
                rect.height * 0.5,
                fill,
                None,
                1.0,
            )?;
        }
        Ok(())
    }
}
pub(super) fn memory_text(snapshot: &SystemStats) -> Result<String> {
    let Some((used, total)) = snapshot
        .memory_used_bytes
        .as_ref()
        .zip(snapshot.memory_total_bytes.as_ref())
    else {
        return Ok("--/-- GB".into());
    };
    Ok(format!(
        "{:.1}/{:.1} GB",
        used.gibibytes()?,
        total.gibibytes()?
    ))
}
pub(super) fn percent_text(percent: Option<f64>) -> String {
    percent.map_or_else(
        || "--%".into(),
        |value| format!("{:3.0}%", value.clamp(0.0, 100.0)),
    )
}
fn number(value: usize) -> f64 {
    value.to_f64().unwrap_or(0.0)
}
