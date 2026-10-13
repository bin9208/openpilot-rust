use openpilot_usbgpu::{
    model::Manifest,
    model_delivery::{self, precompiled, status},
};
use serde::Deserialize;
use std::{io, path::PathBuf};

#[derive(Deserialize)]
struct Input {
    model: Manifest,
    cache: PathBuf,
    assets: PathBuf,
}

fn main() -> Result<(), model_delivery::Error> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let mut progress = Vec::new();
    let result = precompiled::ensure_fallible(
        &model_delivery::agent(8),
        &model_delivery::agent(30),
        &input.model,
        &input.cache,
        &input.assets,
        &mut |done, total| {
            progress.push([done, total]);
            status::write(
                &input.cache,
                status::Phase::Downloading,
                status::Values {
                    model: Some(&input.model),
                    downloaded: Some(done),
                    detail: Some("precompiled model"),
                },
                10.0,
                None,
            )?;
            Ok(())
        },
    );
    let value = match result {
        Ok(path) => serde_json::json!({"installed":path.is_some(),"progress":progress}),
        Err(error) => serde_json::json!({"error":error.to_string(),"progress":progress}),
    };
    serde_json::to_writer(io::stdout().lock(), &value)?;
    Ok(())
}
