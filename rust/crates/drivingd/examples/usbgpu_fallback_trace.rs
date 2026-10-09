use openpilot_driving_modeld::{
    runtime::DrivingRuntime,
    usb_model::{Controls, UsbModel},
    Error,
};
use openpilot_model_runtime::catalog::{Catalog, Kind};
use openpilot_usbgpu::client::Client;
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(Error::Contract(
            "expected trusted catalog, worker, metadata and evidence",
        ));
    }
    let directory = Path::new(&args[3]);
    fs::create_dir_all(directory)?;
    let catalog = Catalog::load(Path::new(&args[0]))?;
    let mut rows = Vec::new();
    for camera in [[1344, 760], [1928, 1208]] {
        let bundle = catalog.select(Kind::Driving, camera)?;
        // SAFETY: the explicitly supplied trusted catalog contains the previously verified immutable native CPU model artifacts.
        let (mut baseline, mut runtime) = unsafe {
            (
                DrivingRuntime::load(bundle, 8)?,
                DrivingRuntime::load(bundle, 8)?,
            )
        };
        let mut command = Command::new(&args[1]);
        command.env("USBGPU_WORKER_TEST_DISCONNECT", "1").env(
            "USBGPU_WORKER_TEST_LOG",
            directory.join(format!("inputs-{}.jsonl", camera[0])),
        );
        let usb =
            UsbModel::from_client(Client::load_command(command, Path::new(&args[2]), camera)?)?;
        runtime.enable_usb(usb);
        let frame = (0..bundle.descriptor.nv12.bytes)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        let transforms = [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2];
        runtime.update_inputs(Controls {
            desire: 1,
            is_rhd: false,
            lateral_time: 0.05,
            longitudinal_time: 0.05,
        });
        if runtime.infer(&frame, &frame, transforms, false)?.is_none() {
            return Err(Error::Contract("missing selected USB prediction"));
        }
        let controls = Controls {
            desire: 3,
            is_rhd: true,
            lateral_time: 0.11,
            longitudinal_time: 0.13,
        };
        runtime.update_inputs(controls);
        let failure = runtime
            .infer(&frame, &frame, transforms, false)
            .err()
            .ok_or(Error::Contract("missing USB failure"))?;
        runtime.disable_usb();
        let prediction = runtime
            .infer(&frame, &frame, transforms, false)?
            .ok_or(Error::Contract("missing same-frame internal fallback"))?;
        baseline.update_inputs(controls);
        baseline
            .infer(&frame, &frame, transforms, false)?
            .ok_or(Error::Contract("missing internal baseline"))?;
        let identical = baseline.raw_predictions() == runtime.raw_predictions();
        let finite = prediction
            .plan
            .iter()
            .flatten()
            .all(|value| value.is_finite());
        if !identical || !finite || runtime.uses_usb() {
            return Err(Error::Contract("warm internal fallback mismatch"));
        }
        fs::write(
            directory.join(format!("fallback-{}.bin", camera[0])),
            runtime.raw_predictions(),
        )?;
        fs::write(
            directory.join(format!("baseline-{}.bin", camera[0])),
            baseline.raw_predictions(),
        )?;
        rows.push(json!({"camera":camera,"worker_error":failure.to_string(),"raw_output_bytes":runtime.raw_predictions().len(),
            "same_frame_raw_exact":identical,"parsed_plan_finite":finite,"usb_active":runtime.uses_usb(),"passed":true}));
    }
    let receipt = json!({"scenario":"Allocated native internal model retained through USB activation and disconnect; same frame and latest controls match direct native baseline", "rows":rows,"passed":true,
        "scope":"Native CPU artifact and real worker binding evidence; no GPU/device performance or complete daemon startup acceptance."});
    fs::write(
        directory.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    Ok(())
}
