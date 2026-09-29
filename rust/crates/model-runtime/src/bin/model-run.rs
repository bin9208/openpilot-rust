use openpilot_model_runtime::CpuModel;
use serde::Deserialize;
use std::{collections::BTreeMap, env, error::Error, fs, path::PathBuf, process::ExitCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    entrypoint: Option<String>,
    inputs: BTreeMap<String, PathBuf>,
    outputs: BTreeMap<String, PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("model-run: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 2 && args[0] == "--check-catalog" {
        let catalog = openpilot_model_runtime::catalog::Catalog::load(&PathBuf::from(&args[1]))?;
        let bundles: Vec<_> = catalog.bundles().iter().map(|bundle| serde_json::json!({
            "kind":format!("{:?}", bundle.descriptor.kind), "camera":bundle.descriptor.camera,
            "backend":bundle.backend, "checkpoint":bundle.descriptor.metadata.model_checkpoint,
        })).collect();
        println!(
            "{}",
            serde_json::json!({"bundles":bundles,"kernels_executed":false})
        );
        return Ok(());
    }
    if args.len() != 3 || args[0] != "--trusted-bundle" {
        return Err(
            "usage: model-run --check-catalog ROOT | --trusted-bundle BUNDLE SEQUENCE.json".into(),
        );
    }
    let directory = PathBuf::from(&args[1]);
    let sequence_path = PathBuf::from(&args[2]);
    let parent = sequence_path
        .parent()
        .ok_or("sequence has no parent directory")?;
    let frames: Vec<Frame> = serde_json::from_slice(&fs::read(&sequence_path)?)?;
    // SAFETY: --trusted-bundle explicitly selects executable model code under the
    // same trust boundary as running a local program; its bundle must be immutable.
    let mut model = unsafe { CpuModel::load(&directory) }?;
    for frame in frames {
        for (name, path) in frame.inputs {
            model.write_input(&name, &fs::read(parent.join(path))?)?;
        }
        match frame.entrypoint {
            Some(name) => model.run_entry(&name)?,
            None => model.run(),
        }
        for (name, path) in frame.outputs {
            let mut output = vec![0; model.output_size(&name)?];
            model.read_output(&name, &mut output)?;
            fs::write(parent.join(path), output)?;
        }
    }
    Ok(())
}
