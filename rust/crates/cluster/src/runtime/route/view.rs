use super::{Result, Route, RouteOverlay, RouteTools};
use crate::{
    config::cluster_camera_view_is_road_camera,
    models::ClusterUiState,
    renderer::{ClusterUiRenderer, FrameClock},
    replay_tools::ToolsState,
    route_replay::paths,
    runtime::input::Frame,
};
use std::io::Write;

impl Route {
    pub fn update(
        &mut self,
        now: f64,
        renderer: &mut ClusterUiRenderer,
        mut camera: i64,
    ) -> Result<Frame> {
        if !self.camera_initialized {
            self.options.insert(
                "road_camera".into(),
                cluster_camera_view_is_road_camera(camera),
            );
            self.camera_initialized = true;
        }
        self.time = self.clock.time(now);
        self.actions(now, &mut camera)?;
        if self.window {
            self.controls(now, renderer);
        }
        if self.clock.waiting
            || self
                .source
                .is_finished(self.time, self.option("route_loop"))?
        {
            self.clock.finish(now, self.source.duration());
            self.time = self.source.duration();
            if self.clock.retry_due(now) {
                let path = paths::adjacent_route_log_path(
                    self.source.log_path_at(self.time),
                    1,
                    &self.kind,
                )?;
                if let Some(path) = path {
                    if self.switch(&path, now, 0.0, false)? {
                        self.time = 0.0;
                    }
                }
            }
        }
        self.source.set_corner_lateral_offset(self.offset);
        let keep = cluster_camera_view_is_road_camera(camera);
        let mut state = self.source.state_at(
            self.time,
            self.option("route_loop"),
            self.overlay != RouteOverlay::Off || keep,
            camera,
        )?;
        state.route_overlay = overlay_for_mode(state.route_overlay, self.overlay, keep);
        if self.clock.cutin(
            state.recorded_cutin_sound,
            self.option("pause_on_cutin"),
            now,
            self.time,
        ) {
            std::thread::Builder::new()
                .name("cutin-alert".into())
                .spawn(|| {
                    let mut stdout = std::io::stdout().lock();
                    if let Err(error) = stdout.write_all(b"\x07").and_then(|()| stdout.flush()) {
                        eprintln!("cut-in bell: {error}");
                    }
                })?;
            println!("LOG CUT-IN at {:.2}s; replay paused", self.time);
        }
        let mut status = self
            .source
            .status_text(self.time, self.option("route_loop"));
        if self.clock.waiting {
            status.push_str(" | waiting for next log");
        }
        self.update_tools(&state, &status)?;
        if self.tools_mode != RouteTools::Overlay {
            if let Some(overlay) = &mut state.route_overlay {
                overlay.panel_visible = false;
            }
        }
        Ok(Frame {
            state,
            status: Some(status),
            camera_view: camera,
        })
    }
    fn controls(&mut self, now: f64, renderer: &mut ClusterUiRenderer) {
        let mouse = renderer.route_replay_mouse_down();
        if self.tools_mode == RouteTools::Overlay {
            let control =
                renderer.route_replay_control_input(self.time, self.source.duration(), self.offset);
            if mouse && !control.control_active && !self.mouse_down {
                self.clock.toggle(now, self.time);
            }
            if let Some(time) = control.seek_s {
                self.clock.seek(now, time);
                self.time = time;
            }
            if control.corner_lateral_offset_m.partial_cmp(&self.offset)
                != Some(std::cmp::Ordering::Equal)
            {
                self.offset = control.corner_lateral_offset_m;
                self.source.set_corner_lateral_offset(self.offset);
                self.clock.pause(now, self.time);
            }
        } else if mouse && !self.mouse_down {
            self.clock.toggle(now, self.time);
        }
        self.mouse_down = mouse;
    }
    fn update_tools(&mut self, state: &ClusterUiState, status: &str) -> Result<()> {
        if let Some(tools) = &mut self.tools {
            let overlay = state.route_overlay.as_ref();
            tools.update(ToolsState {
                playback_s: self.time,
                duration_s: self.source.duration(),
                paused: self.clock.paused,
                status_text: status.into(),
                current_log_folder: self
                    .source
                    .log_folder_at(self.time)
                    .to_string_lossy()
                    .into_owned(),
                options: self.options.clone(),
                video_width: overlay.map_or(0, |o| o.video_width),
                video_height: overlay.map_or(0, |o| o.video_height),
                video_status: overlay.and_then(|o| o.video_status.clone()),
                cutin_status: overlay.and_then(|o| o.cutin_status.clone()),
                data_lines: overlay.map_or_else(Vec::new, |o| o.data_lines.clone()),
                video_rgba: overlay.and_then(|o| o.video_rgba.clone()),
            })?;
        }
        Ok(())
    }
    pub fn window_frame(
        &self,
        renderer: &mut ClusterUiRenderer,
        state: &ClusterUiState,
        clock: FrameClock,
    ) -> crate::renderer::Result<()> {
        if self.tools_mode == RouteTools::Overlay {
            renderer.render_route_replay_frame_at(
                state,
                clock,
                self.time,
                self.source.duration(),
                self.clock.paused,
            )
        } else {
            renderer.render_frame_at(state, clock, None)
        }
    }
}
pub(super) fn overlay_for_mode(
    overlay: Option<crate::models::RouteOverlay>,
    mode: RouteOverlay,
    keep: bool,
) -> Option<crate::models::RouteOverlay> {
    let mut overlay = overlay?;
    match mode {
        RouteOverlay::Off if !keep => return None,
        RouteOverlay::Off => {
            overlay.panel_visible = false;
            overlay.data_lines.clear();
        }
        RouteOverlay::Compact => {
            overlay.panel_visible = true;
            overlay.data_lines.truncate(4);
        }
        RouteOverlay::Full => overlay.panel_visible = true,
    }
    Some(overlay)
}
