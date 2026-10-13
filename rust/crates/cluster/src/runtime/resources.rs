use super::{input::Input, output::Geometry, Error, HelperConfig, Result};
use crate::{
    monitors::{CoreUsageSampler, GitBranchStatusProvider, NetworkAddressProvider},
    renderer::{ClusterUiRenderer, Options},
    runtime_args::{InputSource, Language, OutputRate, RuntimeArgs, Theme},
    runtime_preferences::Snapshot,
};
use num_traits::ToPrimitive;
use std::path::{Path, PathBuf};

pub(super) struct Resources {
    pub input: Input,
    pub renderer: ClusterUiRenderer,
    pub network: NetworkAddressProvider,
    pub git: GitBranchStatusProvider,
    pub cores: Option<CoreUsageSampler>,
    pub active: Snapshot,
    pub started: f64,
}
pub(super) fn renderer_options(
    args: &RuntimeArgs,
    active: &Snapshot,
    geometry: Geometry,
    rate: OutputRate,
) -> Result<Options> {
    Ok(Options {
        width: geometry.frame[0],
        height: geometry.frame[1],
        target_fps: rate
            .target_fps
            .round_ties_even()
            .to_i32()
            .map(|fps| fps.max(0))
            .ok_or_else(|| Error::Invalid("render FPS cannot become an integer".into()))?,
        theme_mode: args
            .theme
            .map_or(active.theme, |theme| match theme {
                Theme::Auto => "auto",
                Theme::Light => "light",
                Theme::Dark => "dark",
            })
            .into(),
        language: args
            .language
            .map_or(active.display.language, |language| match language {
                Language::Ko => "ko",
                Language::En => "en",
            })
            .into(),
        is_metric: args.is_metric.unwrap_or(active.display.is_metric),
        screen_mode: if args.input == InputSource::Navi {
            crate::config::CLUSTER_SCREEN_MODE_NAVI
        } else {
            args.screen_mode.as_deref().map_or(
                active.screen_mode,
                crate::config::normalize_cluster_screen_mode,
            )
        },
        panel_layout: args.panel_layout.as_deref().map_or(
            active.panel_layout,
            crate::config::normalize_cluster_panel_layout,
        ),
        ..Options::default()
    })
}
impl Resources {
    pub fn new(
        root: &Path,
        args: &RuntimeArgs,
        mut active: Snapshot,
        geometry: Geometry,
        rate: OutputRate,
        helpers: &HelperConfig,
    ) -> Result<Self> {
        active.orientation = if active.orientation == 2 { 2 } else { 0 };
        active.camera_view_mode = args.camera_view_mode.unwrap_or(active.camera_view_mode);
        let options = renderer_options(args, &active, geometry, rate)?;
        let mut input = Input::new(root, args, crate::live::monotonic()?, helpers)?;
        input.configure_live(options.screen_mode, active.gate.mode);
        let mut value = Self {
            input,
            renderer: ClusterUiRenderer::new(root, options),
            network: NetworkAddressProvider::new(
                openpilot_params::Params::for_runtime_at(Path::new("/dev/shm/params")).ok(),
                2.0,
            ),
            git: GitBranchStatusProvider::new(
                &root.join("openpilot/selfdrive/carrot/cluster"),
                60.0,
                4.0,
                true,
            ),
            cores: (!args.runtime.no_cluster_core_usage).then(|| {
                CoreUsageSampler::new(
                    PathBuf::from("/proc"),
                    i64::from(std::process::id()),
                    1.0,
                    args.diagnostics.cluster_core_usage_debug,
                )
            }),
            started: crate::live::monotonic()?,
            active,
        };
        value
            .renderer
            .set_profile_enabled(args.diagnostics.profile_render);
        if args.cluster_hud_mode.is_some()
            && !crate::runtime_preferences::Reader::for_runtime()
                .gate()
                .allowed
        {
            println!("ClusterHudDebug=0 and IsOnroad=0; cluster HUD output remains off");
            return Err(Error::Stopped);
        }
        value.input.start()?;
        value
            .renderer
            .open(args.output == crate::runtime_args::Output::Usb)?;
        Ok(value)
    }
    pub fn close(&mut self) -> Result<()> {
        let network = self.network.close().map_err(Error::from);
        let cores = self
            .cores
            .as_mut()
            .map_or(Ok(()), |sampler| sampler.close().map_err(Error::from));
        let git = self.git.close().map_err(Error::from);
        let input = self.input.close();
        let renderer = self.renderer.close().map_err(Error::from);
        network.and(cores).and(git).and(input).and(renderer)
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("cluster resources cleanup: {error}");
        }
    }
}
