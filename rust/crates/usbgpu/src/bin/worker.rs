use openpilot_usbgpu::{
    hcq_gpu::HcqGpu,
    hcq_model::Model,
    native_runtime::{self, FirmwareDirectory},
    worker::{self},
    worker_artifact,
    worker_native::NativeRuntime,
    Error,
};
use std::{
    fs::{self, OpenOptions},
    io,
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
};

fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] {
        println!("openpilot-usbgpu-worker MODEL.pkl SHARED_FILE WIDTH HEIGHT");
        return Ok(());
    }
    let structural = args
        .first()
        .is_some_and(|value| value == "--check-artifacts");
    if structural {
        args.remove(0);
    }
    if args.len() != if structural { 3 } else { 4 } {
        return Err(Error::Contract(
            "expected model, shared file, camera width and height",
        ));
    }
    if !structural && !worker::watch_parent() {
        return Ok(());
    }
    let width = if structural { 1 } else { 2 };
    let camera = [
        args[width]
            .parse()
            .map_err(|_| Error::Contract("invalid camera width"))?,
        args[width + 1]
            .parse()
            .map_err(|_| Error::Contract("invalid camera height"))?,
    ];
    let path = PathBuf::from(&args[0]);
    let assets = std::env::var_os("USBGPU_ASSETS_ROOT")
        .map(PathBuf::from)
        .map_or_else(
            || std::env::current_exe().map(|path| path.with_file_name("usbgpu-assets")),
            Ok,
        )?;
    let expected = match std::env::var("USBGPU_ASSETS_MANIFEST_SHA256") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Error::Contract("invalid native asset manifest identity"))
        }
    };
    let artifact = worker_artifact::prepare(&path, &assets, camera, expected.as_deref())
        .map_err(|error| Error::Protocol(format!("native model assets: {error}")))?;
    if structural {
        use sha2::{Digest, Sha256};
        serde_json::to_writer(
            io::stdout().lock(),
            &serde_json::json!({
            "assets":artifact.assets,"manifest_sha256":artifact.manifest_sha256,
            "descriptor_sha256":format!("{:x}",Sha256::digest(&artifact.descriptor)),
            "warp_sha256":format!("{:x}",Sha256::digest(&artifact.warp)),
            "camera":camera,"checkpoint":artifact.info.checkpoint,
            "frame_size":artifact.info.frame_size,"output_count":artifact.info.output_count,
            "scope":"native artifact preparation; no GPU open or execution"}),
        )?;
        return Ok(());
    }
    let worker_artifact::Artifact {
        assets,
        descriptor,
        warp,
        info,
        ..
    } = artifact;
    let mut firmware = FirmwareDirectory(assets.join("firmware"));
    let cancelled = Arc::new(AtomicBool::new(false));
    let gpu = HcqGpu::new(native_runtime::open(&mut firmware)?, cancelled)?;
    let mut runtime = NativeRuntime::new(Model::load(&descriptor, &path, gpu)?);
    runtime.load_warp(&warp)?;
    let family = fs::read_to_string("/sys/firmware/devicetree/base/model").unwrap_or_default();
    if openpilot_usbgpu::warp_validation::device_family(&family) {
        let qcom = assets.join(format!("warp-qcom-a630-{}x{}", camera[0], camera[1]));
        // SAFETY: startup uses the packaged immutable source warp bundle, subject to the native kernel trust boundary.
        let loaded = unsafe { openpilot_usbgpu::qcom_warp::LocalWarp::load(&qcom) }
            .and_then(|local| runtime.enable_local(local, camera));
        match loaded {
            Ok(()) => eprintln!("QCOM pre-upload warp active"),
            Err(error) => eprintln!(
                "QCOM pre-upload warp initialization/validation failed; using AMD warp: {error}"
            ),
        }
    }
    let file = OpenOptions::new().read(true).write(true).open(&args[1])?;
    worker::serve(
        &mut runtime,
        &file,
        &info,
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
    )
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = worker::report_error(&error, &mut io::stdout().lock());
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
