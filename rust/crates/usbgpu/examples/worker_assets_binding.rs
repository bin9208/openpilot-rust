use openpilot_usbgpu::{
    client::{Client, Launch},
    worker_artifact, Error,
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !(3..=4).contains(&args.len())
        || std::env::var("USB_FIXTURE_MODE").as_deref() != Ok("open_error")
    {
        return Err(Error::Contract(
            "expected owned USB open-error fixture and model/assets/worker",
        )
        .into());
    }
    let model = Path::new(&args[0]);
    let binding = worker_artifact::bind(model, Path::new(&args[1]))?;
    if let Some(flag) = args.get(3) {
        if flag != "--change-manifest" {
            return Err(Error::Contract("unknown binding fixture flag").into());
        }
        OpenOptions::new()
            .append(true)
            .open(binding.root().join("manifest.json"))?
            .write_all(b"\n")?;
    }
    let result = Client::launch_with_assets(
        Launch {
            worker: Path::new(&args[2]),
            model,
            camera: [1928, 1208],
            timeout: Duration::from_secs(40),
            cancelled: Arc::new(AtomicBool::new(false)),
        },
        &binding,
    );
    let Err(Error::Protocol(error)) = result else {
        return Err(Error::Contract("expected actual worker protocol rejection").into());
    };
    serde_json::to_writer(
        std::io::stdout().lock(),
        &serde_json::json!({"root":binding.root(),"manifest_sha256":binding.manifest_sha256(),"error":error}),
    )?;
    Ok(())
}
