use openpilot_model_runtime::qcom::QcomBundle;
use serde::Deserialize;
use std::{collections::BTreeMap, env, error::Error, fs, path::PathBuf, process::ExitCode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    inputs: BTreeMap<String, PathBuf>,
    outputs: BTreeMap<String, PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qcom-model-run: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 2 && args[0] == "--check-bundle" {
        let bundle = QcomBundle::load(&PathBuf::from(&args[1]))?;
        println!(
            "{}",
            serde_json::json!({"backend":"qcom-cl","arch":"a630","allocation_bytes":bundle.allocation_bytes(),
            "kernels":bundle.kernel_count(),"calls":bundle.call_count(),"gpu_executed":false})
        );
        return Ok(());
    }
    if args.len() != 3 || args[0] != "--trusted-bundle" {
        return Err(
            "usage: qcom-model-run --check-bundle BUNDLE | --trusted-bundle BUNDLE SEQUENCE.json"
                .into(),
        );
    }
    let directory = PathBuf::from(&args[1]);
    let sequence_path = PathBuf::from(&args[2]);
    let parent = sequence_path
        .parent()
        .ok_or("sequence has no parent directory")?;
    let frames: Vec<Frame> = serde_json::from_slice(&fs::read(&sequence_path)?)?;
    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    {
        use openpilot_model_runtime::qcom::QcomModel;
        let priority = env::var("QCOM_PRIORITY").map_or(Ok(8), |value| value.parse::<u8>())?;
        // SAFETY: the explicit executable-bundle option requires trusted kernels with the documented QCOM buffer contract.
        let mut model = unsafe { QcomModel::load(&directory, priority) }?;
        for frame in frames {
            for (name, path) in frame.inputs {
                model.write_input(&name, &fs::read(parent.join(path))?)?;
            }
            model.run()?;
            for (name, path) in frame.outputs {
                fs::write(parent.join(path), model.read_output(&name)?)?;
            }
        }
        Ok(())
    }
    #[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
    {
        let _ = (directory, parent, frames);
        Err("QCOM execution requires 64-bit Linux".into())
    }
}
