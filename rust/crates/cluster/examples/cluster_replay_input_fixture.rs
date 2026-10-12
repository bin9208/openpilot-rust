use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{ClusterUiRenderer, FrameClock, Options, StatsProvider, SystemStats},
};
use serde::Deserialize;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Request {
    options: Options,
    state: ClusterUiState,
}
#[derive(Deserialize)]
struct Step {
    name: String,
    duration_s: f64,
    tuning_visible: bool,
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
    let [_, root, input, output] = args.as_slice() else {
        return Err("cluster_replay_input_fixture ROOT INPUT OUTPUT".into());
    };
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer = ClusterUiRenderer::new(&PathBuf::from(root), request.options);
    renderer.set_stats_provider(Box::new(FixedStats));
    let clock = FrameClock {
        monotonic: 100.0,
        perf_counter: 100.0,
        wall_time: 1_791_792_000.0,
    };
    renderer.render_route_replay_frame_at(&request.state, clock, 20.0, 100.0, false)?;
    println!("{}", serde_json::json!({"ready":true}));
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        let step: Step = serde_json::from_str(&line?)?;
        renderer.camera_overlay_state_mut().tuning_visible = step.tuning_visible;
        renderer.render_route_replay_frame_at(
            &request.state,
            clock,
            20.0,
            step.duration_s,
            false,
        )?;
        let result = renderer.route_replay_control_input(20.0, step.duration_s, 0.25);
        let down = renderer.route_replay_mouse_down();
        renderer.render_route_replay_frame_at(
            &request.state,
            clock,
            20.0,
            step.duration_s,
            false,
        )?;
        let path = output.join(format!("{}.png", step.name));
        renderer.write_window_png(&path)?;
        let overlay = renderer.camera_overlay_state();
        println!(
            "{}",
            serde_json::json!({"name":step.name,"input":result,"mouse_down":down,"height_m":overlay.z_offset_m,"pitch_deg":overlay.pitch_offset_deg,"path":path})
        );
        io::stdout().flush()?;
    }
    renderer.close()?;
    Ok(())
}
