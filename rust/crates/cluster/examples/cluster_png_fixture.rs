use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{ClusterUiRenderer, FrameClock, Options, StatsProvider, SystemStats},
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
    portrait: bool,
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
        return Err("cluster_png_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut captures = Vec::new();
    for case in request.cases {
        let mut renderer = ClusterUiRenderer::new(&root, case.options.clone());
        renderer.set_stats_provider(Box::new(FixedStats(request.system_stats.clone())));
        renderer.open(true)?;
        renderer.set_profile_enabled(true);
        let clock = FrameClock {
            monotonic: case.monotonic,
            perf_counter: case.monotonic,
            wall_time: case.wall_time,
        };
        for warm in 0..2 {
            renderer.clear_profile_samples();
            let bytes = renderer.render_to_png_bytes_at(&case.state, clock, case.portrait)?;
            let path = output.join(format!("{}-{warm}.png", case.name));
            std::fs::write(&path, &bytes)?;
            let (width, height) = if case.portrait {
                (case.options.height, case.options.width)
            } else {
                (case.options.width, case.options.height)
            };
            captures.push(serde_json::json!({"name":case.name,"warm":warm,"width":width,"height":height,"path":path,"profile_names":renderer.profile_samples().iter().map(|(name,_)|name).collect::<Vec<_>>() }));
        }
        renderer.close()?;
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":captures.len(),"captures":captures}),
        )?,
    )?;
    Ok(())
}
