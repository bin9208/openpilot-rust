use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{
        CaptureOptions, ClusterUiRenderer, FrameClock, Options, StatsProvider, SystemStats,
    },
};
use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

#[derive(serde::Deserialize)]
struct Request {
    options: Options,
    state: ClusterUiState,
}
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum Stage {
    ImplicitFirst,
    ReopenHidden,
    ReopenVisible,
}
struct FixedStats;
impl StatsProvider for FixedStats {
    fn sample(&mut self, _now: f64) -> SystemStats {
        SystemStats::default()
    }
    fn close(&mut self) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input] = args.as_slice() else {
        return Err("cluster_output_visibility_fixture ROOT INPUT".into());
    };
    let root = PathBuf::from(root);
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer = ClusterUiRenderer::new(&root, request.options);
    renderer.set_stats_provider(Box::new(FixedStats));
    renderer.set_profile_enabled(true);
    let clock = FrameClock {
        monotonic: 100.0,
        perf_counter: 100.0,
        wall_time: 1_791_792_000.0,
    };
    for stage in [
        Stage::ImplicitFirst,
        Stage::ReopenHidden,
        Stage::ReopenVisible,
    ] {
        match stage {
            Stage::ImplicitFirst => {}
            Stage::ReopenHidden => {
                renderer.close()?;
                renderer.open(true)?;
                renderer.close()?;
            }
            Stage::ReopenVisible => {
                renderer.close()?;
                renderer.open(false)?;
                renderer.close()?;
            }
        }
        renderer.clear_profile_samples();
        let frame =
            renderer.render_to_rgba_bytes_at(&request.state, clock, CaptureOptions::default())?;
        assert_eq!(
            frame.rgba.len(),
            usize::try_from(frame.width * frame.height * 4)?
        );
        println!(
            "{}",
            serde_json::json!({"stage":stage,"profile_names":renderer.profile_samples().iter().map(|(name,_)|name).collect::<Vec<_>>()})
        );
        std::io::stdout().flush()?;
        let mut next = String::new();
        if std::io::stdin().lock().read_line(&mut next)? == 0 {
            return Err("visibility input EOF".into());
        }
    }
    renderer.close()?;
    Ok(())
}
