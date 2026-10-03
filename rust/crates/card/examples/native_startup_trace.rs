use openpilot_card::{
    core::StepIo,
    runtime::{self, NativeIo},
};
use openpilot_cereal::car_capnp::car_params;
use openpilot_params::Params;
use serde_json::json;
use std::{
    io::{self, Read, Write},
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).ok_or("missing output path")?;
    let dbc = std::env::args().nth(2).ok_or("missing DBC root")?;
    let assets = std::env::args().nth(3).ok_or("missing model assets")?;
    let numerics = std::env::args()
        .nth(4)
        .ok_or("missing numerical artifact")?;
    let mut transport = NativeIo::new(Params::for_runtime()?, Arc::new(AtomicBool::new(false)))?;
    println!("CARD_STARTUP_READY");
    io::stdout().flush()?;
    let result = runtime::initialize(
        &mut transport,
        Params::for_runtime()?,
        Params::for_runtime()?,
        Path::new(&dbc),
        Path::new(&assets),
        Path::new(&numerics),
    );
    let value = match &result {
        Ok(initialized) => {
            let reader = initialized
                .card
                .params
                .get_root_as_reader::<car_params::Reader>()?;
            let mut params = capnp::message::Builder::new_default();
            params.set_root(reader)?;
            json!({"params": capnp::serialize::write_message_to_words(&params), "identification": initialized.identification,
                "maximum": initialized.card.settings.get("LongitudinalPersonalityMax")?, "frame": transport.subscribers().frame(),
                "model": initialized.common.model_path.as_ref().and_then(|path|path.file_name().and_then(|name|name.to_str()).map(str::to_owned)), "error": false})
        }
        Err(error) => json!({"error": true, "detail": error.to_string()}),
    };
    std::fs::write(&output, serde_json::to_vec(&value)?)?;
    println!("CARD_STARTUP_DONE");
    io::stdout().flush()?;
    io::stdin().read_exact(&mut [0])?;
    drop(result);
    Ok(())
}
