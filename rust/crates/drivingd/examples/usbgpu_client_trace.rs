use openpilot_driving_modeld::{
    usb_model::{Controls, UsbModel},
    Error,
};
use openpilot_usbgpu::client::Client;
use serde_json::json;
use std::{path::Path, process::Command, time::Instant};

fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(Error::Contract("expected worker, metadata, evidence, mode"));
    }
    let directory = Path::new(&args[2]);
    std::fs::create_dir_all(directory)?;
    let mut rows = Vec::new();
    for camera in [[1344, 760], [1928, 1208]] {
        let mut command = Command::new(&args[0]);
        command.env(
            "USBGPU_WORKER_TEST_LOG",
            directory.join(format!("inputs-{}x{}.jsonl", camera[0], camera[1])),
        );
        match args[3].as_str() {
            "nonfinite" => {
                command.env("USBGPU_WORKER_TEST_NONFINITE", "1");
            }
            "disconnect" => {
                command.env("USBGPU_WORKER_TEST_DISCONNECT", "1");
            }
            "timeout" => {
                command.env("USBGPU_WORKER_TEST_TIMEOUT", "1");
            }
            "normal" => {}
            _ => return Err(Error::Contract("unknown client fixture mode")),
        }
        let mut model =
            UsbModel::from_client(Client::load_command(command, Path::new(&args[1]), camera)?)?;
        let frame = vec![0; model.client.info.frame_size];
        let transforms = [[1., 0., 0., 0., 1., 0., 0., 0., 1.]; 2];
        let mut values = Vec::new();
        let mut failure = None;
        let started = Instant::now();
        for desire in [1, 1, 0, 1] {
            model.update(Controls {
                desire,
                is_rhd: false,
                lateral_time: 0.05,
                longitudinal_time: 0.05,
            });
            match model.infer(&frame, &frame, transforms) {
                Ok(prediction) => {
                    if !prediction
                        .plan
                        .iter()
                        .flatten()
                        .all(|value| value.is_finite())
                    {
                        return Err(Error::Contract("nonfinite parsed plan"));
                    }
                    values.push(f32::from_le_bytes(
                        model.client.raw_output()[..4]
                            .try_into()
                            .map_err(|_| Error::Contract("raw scalar"))?,
                    ));
                }
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        let expected = match args[3].as_str() {
            "normal" => failure.is_none() && values == [1., 2., 3., 4.],
            "nonfinite" => {
                values.is_empty()
                    && failure
                        .as_ref()
                        .is_some_and(|error| error.contains("invalid precompiled"))
            }
            "disconnect" => {
                values == [1.]
                    && failure
                        .as_ref()
                        .is_some_and(|error| error.contains("disconnection"))
            }
            "timeout" => {
                values == [1.]
                    && failure
                        .as_ref()
                        .is_some_and(|error| error.contains("timed out"))
                    && started.elapsed().as_secs_f64() < 2.
            }
            _ => false,
        };
        if !expected {
            return Err(Error::Contract("unexpected Rust client observation"));
        }
        drop(model);
        rows.push(json!({"camera":camera,"values":values,"failure":failure,"seconds":started.elapsed().as_secs_f64(),"passed":true}));
    }
    let receipt = json!({"scenario":"Rust client and driving prediction binding against owned native worker protocol", "mode":args[3], "rows":rows,
        "scope":"Client layout, error/timeout lifecycle, fused output decoding; no GPU model numeric acceptance."});
    std::fs::write(
        directory.join("comparison.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{receipt}");
    Ok(())
}
