use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{
        CaptureOptions, ClusterUiRenderer, FrameClock, Options, StatsProvider, SystemStats,
    },
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    system_stats: SystemStats,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    options: Options,
    state: ClusterUiState,
    monotonic: f64,
    wall_time: f64,
    #[serde(default)]
    capture: CaptureOptions,
}
struct FixedStats(SystemStats);
impl StatsProvider for FixedStats {
    fn sample(&mut self, _now: f64) -> SystemStats {
        self.0.clone()
    }
    fn close(&mut self) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output] = args.as_slice() else {
        return Err("cluster_frame_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer: Option<ClusterUiRenderer> = None;
    let mut dimensions = None;
    let mut captures = Vec::new();
    for case in request.cases {
        let size = (case.options.width, case.options.height);
        if dimensions != Some(size) {
            if let Some(value) = renderer.as_mut() {
                value.close()?;
            }
            let mut value = ClusterUiRenderer::new(&root, case.options.clone());
            value.set_stats_provider(Box::new(FixedStats(request.system_stats.clone())));
            value.open(true)?;
            value.set_profile_enabled(true);
            renderer = Some(value);
            dimensions = Some(size);
        }
        let value = renderer.as_mut().ok_or("renderer missing")?;
        value.set_theme_mode(&case.options.theme_mode);
        value.set_screen_mode(case.options.screen_mode);
        value.set_panel_layout(case.options.panel_layout);
        value.set_display_preferences(&case.options.language, case.options.is_metric);
        let clock = FrameClock {
            monotonic: case.monotonic,
            perf_counter: case.monotonic,
            wall_time: case.wall_time,
        };
        for warm in 0..2 {
            value.clear_profile_samples();
            let frame = value.capture_state_at(&case.state, clock, case.capture)?;
            let path = output.join(format!("{}-{warm}.png", case.name));
            value.write_png(&frame, &path)?;
            captures.push(serde_json::json!({"name":case.name,"warm":warm,"width":frame.width,"height":frame.height,"path":path,"profile_names":value.profile_samples().iter().map(|(name,_)|name).collect::<Vec<_>>() }));
        }
    }
    if let Some(mut value) = renderer {
        value.close()?;
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":captures.len(),"captures":captures}),
        )?,
    )?;
    Ok(())
}
