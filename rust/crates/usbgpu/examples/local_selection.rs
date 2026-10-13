use openpilot_usbgpu::{
    local_artifact::Package,
    model::{self, Paths},
    Error,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

fn run() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(Error::Contract(
            "expected model directory, cache and assets",
        ));
    }
    let paths = Paths {
        models: PathBuf::from(&args[0]),
        cache: PathBuf::from(&args[1]),
        assets: PathBuf::from(&args[2]),
    };
    for line in io::stdin().lock().lines() {
        let phase = line?;
        if phase == "runtime-paths" {
            let runtime = Paths::for_runtime(
                std::path::Path::new("/owned/runtime-root"),
                paths.assets.clone(),
            )?;
            println!(
                "{}",
                serde_json::json!({"cache":runtime.cache,"models":runtime.models,"assets":runtime.assets})
            );
            io::stdout().flush()?;
            continue;
        }
        let status = model::status(&paths)?;
        let selected = model::compiled_path(&paths);
        let full = if phase == "worker-check" {
            selected.as_ref().map(|path| {
                Package::open(path, &paths.assets, None)
                    .map(|_| true)
                    .unwrap_or(false)
            })
        } else {
            None
        };
        println!(
            "{}",
            serde_json::json!({"phase":phase,"compiled":status.compiled,"active_compiled":model::active_compiled_path(&paths).is_some(),
            "compile_pending":status.compile_pending,"selected":selected,"full_worker_validation":full})
        );
        io::stdout().flush()?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
