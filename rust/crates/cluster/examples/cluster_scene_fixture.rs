use openpilot_cluster::{
    config::{dark_theme, light_theme},
    models::ClusterUiState,
    scene::SceneBuilder,
};
use std::io::{self, BufRead, Write};

#[derive(serde::Deserialize)]
struct Request {
    state: ClusterUiState,
    dark: bool,
    highlight_lit: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = SceneBuilder::default();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let theme = if request.dark {
            dark_theme()
        } else {
            light_theme()
        };
        let mut profile_names = Vec::new();
        let mut capture = |name: &str, elapsed: f64| {
            assert!(elapsed.is_finite() && elapsed >= 0.0);
            profile_names.push(name.to_owned());
        };
        let scene = builder.build(
            &request.state,
            request.highlight_lit,
            &theme,
            Some(&mut capture),
        );
        serde_json::to_writer(
            &mut output,
            &serde_json::json!({"scene": scene, "profile_names": profile_names}),
        )?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
