use super::{Result, Route, Source};
use crate::{replay_tools::RouteReplayToolAction, route_replay::paths, runtime_args::RouteOverlay};
use std::path::{Path, PathBuf};

impl Route {
    pub(super) fn actions(&mut self, now: f64, camera: &mut i64) -> Result<()> {
        let action = self
            .tools
            .as_ref()
            .and_then(crate::replay_tools::RouteReplayToolsWindow::poll);
        if let Some(action) = action {
            self.apply(action, now, camera)?;
        } else if let Some(tools) = &mut self.tools {
            if !tools.is_alive()? {
                self.close_tools()?;
            }
        }
        Ok(())
    }
    fn apply(&mut self, action: RouteReplayToolAction, now: f64, camera: &mut i64) -> Result<()> {
        let mut reload = false;
        for (name, value) in action.option_updates {
            let Some(option) = self.options.get_mut(&name) else {
                continue;
            };
            *option = value;
            match name.as_str() {
                "front_radar_only" => {
                    self.config.front_radar_only = Some(value);
                    reload = true;
                }
                "route_loop" => {
                    if value && self.clock.waiting {
                        self.clock.resume_loop(now);
                        self.time = 0.0;
                    }
                }
                "pause_on_cutin" if !value => self.clock.reset_cutin(),
                "show_route_overlay" => {
                    self.overlay = if value {
                        RouteOverlay::Compact
                    } else {
                        RouteOverlay::Off
                    }
                }
                "road_camera" => *camera = if value { 2 } else { 0 },
                _ => (),
            }
        }
        if action.toggle_pause {
            self.clock.toggle(now, self.time);
        }
        if let Some(time) = action.seek_s {
            self.clock.seek(now, time);
            self.time = time;
        }
        let mut path = action.open_path.map(PathBuf::from);
        if path.is_none() && (action.previous_log || action.next_log) {
            let direction = if action.previous_log { -1 } else { 1 };
            path = paths::adjacent_route_log_path(
                self.source.log_path_at(self.time),
                direction,
                &self.kind,
            )?;
            if path.is_none() {
                println!(
                    "No {} numbered replay log found",
                    if direction < 0 { "previous" } else { "next" }
                );
            }
        }
        if let Some(path) = path {
            if self.switch(&path, now, 0.0, false)? {
                self.time = 0.0;
            }
        } else if reload {
            let path = self.source.log_path_at(self.time).to_path_buf();
            let offset = self.source.log_offset_at(self.time);
            if self.switch(&path, now, offset, self.clock.paused)? {
                self.time = self.clock.base;
            }
        }
        if action.closed {
            self.close_tools()?;
        }
        Ok(())
    }
    pub(super) fn switch(
        &mut self,
        path: &Path,
        now: f64,
        resume: f64,
        paused: bool,
    ) -> Result<bool> {
        let path = paths::resolve(path)?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_lowercase();
        let kind = if name.starts_with("qlog.") {
            "qlog"
        } else if name.starts_with("rlog.") {
            "rlog"
        } else {
            &self.kind
        }
        .to_owned();
        self.config.lateral_offset = self.offset;
        let source = match Source::load(&path, &kind, None, Some(1), self.config.clone()) {
            Ok(source) => source,
            Err(error) => {
                println!("Failed to open replay log {}: {error}", path.display());
                return Ok(false);
            }
        };
        let mut old = std::mem::replace(&mut self.source, source);
        self.kind = kind;
        self.clock = super::Playback::new(resume, self.source.duration(), now, self.clock.speed);
        self.clock.paused = paused;
        old.close()?;
        println!(
            "Playing replay log: {} ({:.1}s)",
            self.source
                .first_file()
                .parent()
                .unwrap_or(Path::new("."))
                .display(),
            self.source.duration()
        );
        Ok(true)
    }
}
