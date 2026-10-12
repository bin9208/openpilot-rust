use openpilot_cluster::{
    route_replay::{preload, Source, SourceConfig},
    runtime_args::CornerSource,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Step {
    time: f64,
    repeat: bool,
}
#[derive(Deserialize)]
struct Query {
    files: Vec<PathBuf>,
    steps: Vec<Step>,
    launcher: PathBuf,
    #[serde(default)]
    front_radar_only: Option<bool>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--route-preload-helper")
    {
        return Ok(preload::run_helper(
            Path::new(args.get(2).ok_or("socket path missing")?),
            CornerSource::Live,
        )?);
    }
    let query: Query = serde_json::from_reader(std::io::stdin().lock())?;
    let mut config = SourceConfig::for_preload(preload::Config {
        program: std::env::current_exe()?,
        launcher: query.launcher,
        cwd: std::env::current_dir()?,
        corner_source: CornerSource::Live,
    })?;
    config.front_radar_only = query.front_radar_only;
    let mut source = Source::new(query.files, config)?;
    let mut results = Vec::new();
    for step in query.steps {
        let state = source.state_at(step.time, step.repeat, true, 1)?;
        results.push(serde_json::json!({"state": state, "status":source.status_text(step.time, step.repeat),
            "duration":source.duration(), "loaded":source.loaded_file_count(),
            "path":source.log_path_at(step.time), "folder":source.log_folder_at(step.time),
            "offset":source.log_offset_at(step.time), "finished":source.is_finished(step.time, step.repeat)?,
        }));
    }
    source.close()?;
    source.close()?;
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
