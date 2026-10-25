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
    if args.len() == 2 && (args[0] == "--check-catalog" || args[0] == "--catalog-generation") {
        let catalog = openpilot_model_runtime::catalog::Catalog::load(&PathBuf::from(&args[1]))?;
        if args[0] == "--catalog-generation" {
            println!("{}", catalog.generation());
            return Ok(());
        }
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
    if !matches!(args.len(), 3 | 7 | 9)
        || args[0] != "--trusted-bundle"
        || (args.len() >= 7
            && (args[3] != "--cached-admission" || args[5] != "--cached-admission-sha256"))
        || (args.len() == 9 && args[7] != "--driver-prestate-476")
    {
        return Err(
            "usage: model-run --check-catalog ROOT | --catalog-generation ROOT | --trusted-bundle BUNDLE SEQUENCE.json [--cached-admission PATH --cached-admission-sha256 SHA256]".into(),
        );
    }
    #[cfg(not(unix))]
    if args.len() == 9 {
        return Err("driver476 prestate requires Unix cached runtime".into());
    }
    let directory = PathBuf::from(&args[1]);
    let sequence_path = PathBuf::from(&args[2]);
    let parent = sequence_path
        .parent()
        .ok_or("sequence has no parent directory")?;
    let frames: Vec<Frame> = serde_json::from_slice(&fs::read(&sequence_path)?)?;
    // SAFETY: --trusted-bundle explicitly selects executable model code under the
    // same trust boundary as running a local program; its bundle must be immutable.
    #[cfg(unix)]
    let admission = if args.len() >= 7 {
        let expected = args[6].to_str().ok_or("cached admission SHA is not UTF8")?;
        Some(
            openpilot_model_runtime::cached_code::Admission::read_authenticated(
                &PathBuf::from(&args[4]),
                expected,
            )
            .map_err(openpilot_model_runtime::Error::CachedCode)?,
        )
    } else {
        None
    };
    #[cfg(unix)]
    // SAFETY: explicit paired admission additionally authenticates original/current
    // issuer expectations; neither argument is derived from a candidate sidecar.
    let mut model = unsafe {
        match &admission {
            Some(expected) => CpuModel::load_cached(&directory, expected),
            None => CpuModel::load(&directory),
        }
    }?;
    #[cfg(not(unix))]
    let mut model = {
        if args.len() != 3 {
            return Err("cached admission requires x86_64 Linux".into());
        }
        // SAFETY: ordinary --trusted-bundle contract above applies.
        unsafe { CpuModel::load(&directory) }?
    };
    #[cfg(unix)]
    let trace = (args.len() == 9).then(|| PathBuf::from(&args[8]));
    #[cfg(unix)]
    if let Some(output) = &trace {
        if frames.len() != 4 {
            return Err("driver476 requires the exact warmup/frame0 four-stage sequence".into());
        }
        model.write_driver_prestate_476(
            &directory,
            output,
            "loaded",
            admission.as_ref().ok_or("driver476 admission")?,
        )?;
    }
    for (_index, frame) in frames.into_iter().enumerate() {
        for (name, path) in frame.inputs {
            model.write_input(&name, &fs::read(parent.join(path))?)?;
        }
        #[cfg(unix)]
        if let Some(output) = &trace {
            let phase = [
                "warmup-prepare",
                "warmup-model",
                "frame-0-prepare",
                "frame-0-model",
            ][_index];
            let entry = if _index % 2 == 0 { "prepare" } else { "model" };
            if frame.entrypoint.as_deref() != Some(entry) {
                return Err("driver476 entrypoint order".into());
            }
            model.run_driver_entry_476(
                &directory,
                output,
                phase,
                entry,
                admission.as_ref().ok_or("driver476 admission")?,
            )?;
        }
        if args.len() != 9 {
            match frame.entrypoint {
                Some(name) => model.run_entry(&name)?,
                None => model.run(),
            }
        }
        for (name, path) in frame.outputs {
            let mut output = vec![0; model.output_size(&name)?];
            model.read_output(&name, &mut output)?;
            fs::write(parent.join(path), output)?;
        }
    }
    Ok(())
}
