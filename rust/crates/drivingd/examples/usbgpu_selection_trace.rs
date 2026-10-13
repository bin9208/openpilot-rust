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
    fs::write(path.join("fixture/speed"), "5000.0")?;
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
fn configure(root: &Path, worker: &Path, assets: &Path) -> Configuration {
    Configuration {
        paths: Paths {
            cache: root.join("cache"),
            models: root.join("models"),
            assets: assets.into(),
        },
        devices: root.join("sysfs"),
        worker: worker.into(),
        cancelled: Arc::new(AtomicBool::new(false)),
    }
}
fn execute(
    config: &Configuration,
    params: &Params,
    scenario: &str,
) -> Result<serde_json::Value, Error> {
    let loaded = usb_selection::start(config, params, [1344, 760]);
    let initial = flags(params)?;
    let mut failure = None;
    let mut parsed_plan_finite = None;
    let mut admitted = None;
    let selected = match loaded {
        Ok(Some(mut model)) => {
            let frame = vec![0; model.client.info.frame_size];
            let transforms = [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2];
            let prediction = model.infer(&frame, &frame, transforms)?;
            parsed_plan_finite = Some(
                prediction
                    .plan
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite()),
            );
            admitted = Some(json!({"checkpoint":model.client.info.checkpoint,
                "output_count":model.client.info.output_count,"frame_size":model.client.info.frame_size}));
            usb_selection::published(params)?;
            if scenario == "disconnect" {
                failure = Some(
                    model
                        .infer(&frame, &frame, transforms)
                        .err()
                        .ok_or(Error::Contract("disconnect did not fail"))?
                        .to_string(),
                );
                usb_selection::fail(params)?;
            }
            true
        }
        Ok(None) => false,
        Err(error) => {
            failure = Some(error.to_string());
            false
        }
    };
    let expected = ["normal", "grace", "retry", "disconnect"].contains(&scenario);
    let final_flags = flags(params)?;
    let expected_failure = ["load-error", "disconnect"].contains(&scenario);
    let passed = selected == expected
        && (!selected || parsed_plan_finite == Some(true))
        && (!selected || (initial["UsbGpuActive"] == true && initial["UsbGpuLoading"] == true))
        && (!expected_failure
            || (final_flags["UsbGpuActive"] == false
                && final_flags["UsbGpuLoading"] == false
                && final_flags["UsbGpuStartupFailed"] == true));
    Ok(
        json!({"scenario":scenario,"initial":initial,"final":final_flags,
        "failure":failure,"selected":selected,"admitted":admitted,
        "parsed_plan_finite":parsed_plan_finite,"passed":passed}),
    )
}
fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(Error::Contract(
            "expected worker, prepared fixture root, trusted assets and scenario",
        ));
    }
    let root = Path::new(&args[1]);
    let scenario = args[3].as_str();
    if ![
        "absent",
        "missing-compiled",
        "startup-failed",
        "normal",
        "grace",
        "load-error",
        "retry",
        "disconnect",
    ]
    .contains(&scenario)
    {
        return Err(Error::Contract("unknown selection scenario"));
    }
    let config = configure(root, Path::new(&args[0]), Path::new(&args[2]));
    let params = Params::open(&root.join("params"), "fixture")?;
    if scenario == "startup-failed" {
        params.put_bool("UsbGpuStartupFailed", true)?;
    }
    if scenario == "grace" {
        params.put_bool("UsbGpuHardwareSeen", true)?;
        let devices = config.devices.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            present(&devices).expect("owned sysfs");
        });
    }
    std::env::set_var("USBGPU_WORKER_TEST_LOG", root.join("inputs.jsonl"));
    let started = Instant::now();
    let mut receipt = execute(&config, &params, scenario)?;
    receipt["seconds"] = started.elapsed().as_secs_f64().into();
    receipt["scope"] = "Actual generic model/companions admitted through drivingd selection; owned native protocol worker, not GPU numerical or whole-daemon acceptance.".into();
    if let Ok(value) = fs::read_to_string(root.join("attempts")) {
        receipt["attempts"] = value
            .parse::<u32>()
            .map_err(|_| Error::Contract("attempt counter"))?
            .into();
    }
    fs::write(
        root.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    if receipt["passed"] != true {
        return Err(Error::Contract("selection observable mismatch"));
    }
    Ok(())
}
