mod actions;
mod playback;
mod view;
use super::{HelperConfig, Result};
use crate::{
    config::cluster_camera_view_is_road_camera,
    replay_tools::{GuiConfig, RouteReplayToolsWindow, WindowConfig},
    route_replay::{preload, Source, SourceConfig},
    runtime_args::{RouteLog, RouteOverlay, RouteTools, RuntimeArgs},
};
pub use playback::Playback;
use std::{collections::BTreeMap, path::Path};

pub(super) struct Route {
    source: Source,
    config: SourceConfig,
    pub clock: Playback,
    pub time: f64,
    kind: String,
    overlay: RouteOverlay,
    tools_mode: RouteTools,
    tools: Option<RouteReplayToolsWindow>,
    helpers: HelperConfig,
    options: BTreeMap<String, bool>,
    offset: f64,
    mouse_down: bool,
    camera_initialized: bool,
    window: bool,
}
impl Route {
    pub fn new(args: &RuntimeArgs, helpers: &HelperConfig) -> Result<Self> {
        let mut config = SourceConfig::for_preload(preload::Config {
            program: helpers.program.clone(),
            launcher: helpers.launcher.clone(),
            cwd: helpers.cwd.clone(),
            corner_source: args.route_corner_source,
        })?;
        let front_only = std::env::var("CLUSTER_ROUTE_FRONT_RADAR_ONLY").is_ok_and(|v| v == "1");
        config.front_radar_only = Some(front_only);
        let kind = match args.route_log {
            RouteLog::Rlog => "rlog",
            RouteLog::Qlog => "qlog",
        };
        let source = Source::load(
            &args.route,
            kind,
            args.route_start_segment,
            args.route_max_segments,
            config.clone(),
        )?;
        println!(
            "Loaded route replay buffer: {} frames, {:.1}s from {}/{} {kind} files",
            source.frame_count(),
            source.duration(),
            source.loaded_file_count(),
            source.file_count()
        );
        let now = crate::live::monotonic()?;
        let clock = Playback::new(
            args.route_start_time,
            source.duration(),
            now,
            args.route_replay_speed,
        );
        let options = BTreeMap::from([
            ("front_radar_only".into(), front_only),
            ("route_loop".into(), args.replay.route_loop),
            ("pause_on_cutin".into(), args.replay.route_pause_on_cutin),
            (
                "show_route_overlay".into(),
                args.route_overlay != RouteOverlay::Off,
            ),
            (
                "road_camera".into(),
                cluster_camera_view_is_road_camera(args.camera_view_mode.unwrap_or(0)),
            ),
        ]);
        Ok(Self {
            source,
            config,
            time: clock.base,
            clock,
            kind: kind.into(),
            overlay: args.route_overlay,
            tools_mode: args.route_tools,
            tools: None,
            helpers: helpers.clone(),
            options,
            offset: 0.0,
            mouse_down: false,
            camera_initialized: false,
            window: matches!(
                args.output,
                crate::runtime_args::Output::Window | crate::runtime_args::Output::Both
            ),
        })
    }
    fn option(&self, key: &str) -> bool {
        self.options.get(key).copied().unwrap_or(false)
    }
    pub fn start(&mut self) -> Result<()> {
        if self.tools_mode == RouteTools::Separate && self.tools.is_none() {
            self.tools = Some(RouteReplayToolsWindow::new(&WindowConfig {
                program: self.helpers.program.clone(),
                launcher: self.helpers.launcher.clone(),
                cwd: self.helpers.cwd.clone(),
                gui: GuiConfig::installed(self.helpers.program.parent().unwrap_or(Path::new(".")))?,
            })?);
        }
        Ok(())
    }
    fn close_tools(&mut self) -> Result<()> {
        if let Some(mut tools) = self.tools.take() {
            tools.close()?;
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        let tools = self.close_tools();
        let source = self.source.close().map_err(super::Error::from);
        tools.and(source)
    }
}
#[cfg(test)]
mod tests;
