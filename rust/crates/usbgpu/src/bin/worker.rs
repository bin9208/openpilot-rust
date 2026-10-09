use openpilot_usbgpu::{
    hcq_gpu::HcqGpu,
    hcq_model::Model,
    native_runtime::{self, FirmwareDirectory},
    worker::{self, Info, Metadata},
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] {
        println!("openpilot-usbgpu-worker MODEL.pkl SHARED_FILE WIDTH HEIGHT");
        return Ok(());
    }
    if args.len() != 4 {
        return Err(Error::Contract(
            "expected model, shared file, camera width and height",
        ));
    }
    if !worker::watch_parent() {
        return Ok(());
    }
    let camera = [
        args[2]
            .parse()
            .map_err(|_| Error::Contract("invalid camera width"))?,
        args[3]
            .parse()
            .map_err(|_| Error::Contract("invalid camera height"))?,
    ];
    let path = PathBuf::from(&args[0]);
    let directory = path
        .parent()
        .ok_or(Error::Contract("model has no directory"))?;
    let installed: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("installed.json"))?)?;
    let packaged = std::env::current_exe()?.with_file_name("usbgpu-assets");
    let digest = installed["pickle"]["sha256"]
        .as_str()
        .ok_or(Error::Contract("missing installed model hash"))?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(Error::Contract("invalid installed model hash"));
    }
    let companions = if path.with_extension("hcq.json").is_file() {
        directory.to_path_buf()
    } else {
        packaged.join("models").join(digest)
    };
    let descriptor_path = if companions == directory {
        path.with_extension("hcq.json")
    } else {
        companions.join("model.hcq.json")
    };
    let metadata_path = if companions == directory {
        path.with_extension("hcq-meta.json")
    } else {
        companions.join("model.hcq-meta.json")
    };
    let metadata: Metadata = serde_json::from_slice(&fs::read(metadata_path)?)?;
    let descriptor = fs::read(descriptor_path)?;
    let descriptor_header: serde_json::Value = serde_json::from_slice(&descriptor)?;
    if installed["format"] != "comma-generic-onnx"
        || installed["gpu_arch"] != "gfx1200"
        || installed["model_checkpoint"] != metadata.checkpoint
        || installed["pickle"]["sha256"] != metadata.model_sha256
        || descriptor_header["model_sha256"] != metadata.model_sha256
    {
        return Err(Error::Contract(
            "precompiled worker artifact metadata mismatch",
        ));
    }
    let info = Info::new(metadata, camera)?;
    let assets = if directory.join("firmware").is_dir() {
        directory.to_path_buf()
    } else {
        packaged
    };
    let warp = fs::read(assets.join(format!("warp-gfx1200-{}x{}.json", camera[0], camera[1])))?;
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
