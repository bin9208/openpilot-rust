use openpilot_driving_modeld::{
    usb_selection::{self, Configuration},
    Error,
};
use openpilot_params::Params;
use openpilot_usbgpu::model::Paths;
use serde_json::json;
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

fn present(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path.join("fixture"))?;
    fs::write(path.join("fixture/idVendor"), "add1")?;
    fs::write(path.join("fixture/idProduct"), "0001")?;
    Ok(())
}
fn flags(params: &Params) -> Result<serde_json::Value, Error> {
    let mut values = serde_json::Map::new();
    for name in [
        "UsbGpuPresent",
        "UsbGpuCompiled",
        "UsbGpuHardwareSeen",
        "UsbGpuLoading",
        "UsbGpuActive",
        "UsbGpuStartupFailed",
    ] {
        values.insert(name.into(), params.get_bool(name)?.into());
    }
    Ok(values.into())
}
fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(Error::Contract("expected worker, metadata and evidence"));
    }
    let root = Path::new(&args[2]);
    fs::create_dir_all(root)?;
    let metadata = fs::read(&args[1])?;
    let mut rows = Vec::new();
    for scenario in [
        "absent",
        "missing-compiled",
        "startup-failed",
        "normal",
        "grace",
        "load-error",
        "retry",
        "disconnect",
    ] {
        let directory = root.join(scenario);
        let cache = directory.join("cache");
        let models = directory.join("models");
        let devices = directory.join("sysfs");
        for path in [&cache, &models, &devices] {
            fs::create_dir_all(path)?;
        }
        let params = Params::open(&directory.join("params"), "fixture")?;
        let config = Configuration {
            paths: Paths {
                cache: cache.clone(),
                models: models.clone(),
            },
            devices: devices.clone(),
            worker: args[0].clone().into(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let digest = "1".repeat(64);
        fs::write(
            cache.join("state.json"),
            serde_json::to_vec(
                &json!({"active":{"model_id":"fixture","filename":"fixture.onnx",
            "size":1,"sha256":digest,"url":"https://fixture.invalid/fixture.onnx"},"previous":null}),
            )?,
        )?;
        fs::write(cache.join(format!("fixture-{}.onnx", "1".repeat(16))), b"x")?;
        if scenario != "missing-compiled" {
            let path = models.join(format!("big_driving_{}_tinygrad.pkl", "1".repeat(16)));
            fs::write(&path, &metadata)?;
            fs::write(path.with_extension("pkl.chunkmanifest"), b"fixture")?;
        }
        if !["absent", "grace"].contains(&scenario) {
            present(&devices)?;
        }
        if scenario == "startup-failed" {
            params.put_bool("UsbGpuStartupFailed", true)?;
        }
        if scenario == "grace" {
            params.put_bool("UsbGpuHardwareSeen", true)?;
            let devices = devices.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                present(&devices).expect("owned sysfs");
            });
        }
        std::env::set_var("USBGPU_WORKER_TEST_LOG", directory.join("inputs.jsonl"));
        if scenario == "load-error" {
            std::env::set_var("USBGPU_WORKER_TEST_LOAD_ERROR", "owned loader failure");
        }
        if scenario == "retry" {
            std::env::set_var("USBGPU_WORKER_TEST_RETRY_FILE", directory.join("attempts"));
        }
        if scenario == "disconnect" {
            std::env::set_var("USBGPU_WORKER_TEST_DISCONNECT", "1");
        }
        let started = Instant::now();
        let loaded = usb_selection::start(&config, &params, [1344, 760]);
        let initial = flags(&params)?;
        let mut failure = None;
        match loaded {
            Ok(Some(mut model)) => {
                if !["normal", "grace", "retry", "disconnect"].contains(&scenario)
                    || initial["UsbGpuActive"] != true
                    || initial["UsbGpuLoading"] != true
                {
                    return Err(Error::Contract("unexpected selected worker"));
                }
                let frame = vec![0; model.client.info.frame_size];
                let transforms = [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2];
                model.infer(&frame, &frame, transforms)?;
                usb_selection::published(&params)?;
                if scenario == "disconnect" {
                    let error = model
                        .infer(&frame, &frame, transforms)
                        .err()
                        .ok_or(Error::Contract("disconnect did not fail"))?;
                    failure = Some(error.to_string());
                    usb_selection::fail(&params)?;
                }
            }
            Ok(None) if ["absent", "missing-compiled", "startup-failed"].contains(&scenario) => {}
            Err(error) if scenario == "load-error" => {
                failure = Some(error.to_string());
            }
            _ => return Err(Error::Contract("unexpected selection result")),
        }
        std::env::remove_var("USBGPU_WORKER_TEST_LOAD_ERROR");
        std::env::remove_var("USBGPU_WORKER_TEST_RETRY_FILE");
        std::env::remove_var("USBGPU_WORKER_TEST_DISCONNECT");
        let final_flags = flags(&params)?;
        if ["load-error", "disconnect"].contains(&scenario)
            && (final_flags["UsbGpuActive"] != false
                || final_flags["UsbGpuLoading"] != false
                || final_flags["UsbGpuStartupFailed"] != true)
        {
            return Err(Error::Contract("failure flags mismatch"));
        }
        if scenario == "retry" && fs::read_to_string(directory.join("attempts"))? != "2" {
            return Err(Error::Contract("retry attempts mismatch"));
        }
        rows.push(json!({"scenario":scenario,"initial":initial,"final":final_flags,"failure":failure,"seconds":started.elapsed().as_secs_f64(),"passed":true}));
    }
    let receipt = json!({"scenario":"Real Rust USB selection, owned sysfs and native worker lifecycle", "rows":rows,"passed":true,
        "scope":"Selection, grace, retry, loading/publication and failure flags; no GPU numerical or whole-daemon startup acceptance."});
    fs::write(
        root.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    Ok(())
}
