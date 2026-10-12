use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{ClusterUiRenderer, Options, SystemPanelOptions, SystemStats},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    options: Options,
    stats: SystemStats,
    detailed: bool,
}
#[derive(Serialize)]
struct Observation {
    name: String,
    warm: usize,
    error: Option<String>,
    path: Option<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output] = args.as_slice() else {
        return Err("cluster_stats_render_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let state = ClusterUiState::default();
    let mut observations = Vec::new();
    for case in request.cases {
        let mut renderer = ClusterUiRenderer::new(&root, case.options.clone());
        renderer.open(true)?;
        for warm in 0..2 {
            let result = renderer.capture_draw(|renderer| {
                renderer.gfx.clear(renderer.current_theme().bg)?;
                renderer.gfx.push_matrix()?;
                renderer.gfx.scale(
                    f64::from(case.options.width) / 1920.0,
                    f64::from(case.options.height) / 480.0,
                    1.0,
                )?;
                let result = renderer.draw_system_stats_panel(
                    &state,
                    SystemPanelOptions {
                        stats: Some(&case.stats),
                        show_runtime_info: case.detailed,
                        ..Default::default()
                    },
                );
                let pop = renderer.gfx.pop_matrix();
                result.and(pop)
            });
            let (error, path) = match result {
                Ok(frame) => {
                    let path = output.join(format!("{}-{warm}.png", case.name));
                    renderer.write_png(&frame, &path)?;
                    (None, Some(path.to_string_lossy().into_owned()))
                }
                Err(error) => (Some(error.to_string()), None),
            };
            observations.push(Observation {
                name: case.name.clone(),
                warm,
                error,
                path,
            });
        }
        renderer.close()?;
    }
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&observations)?,
    )?;
    Ok(())
}
