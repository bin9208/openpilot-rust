use openpilot_updated::{
    agnos::{Agnos, Native},
    Error,
};
use std::path::Path;

fn main() -> Result<(), Error> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [config, manifest, endpoint] = args.as_slice() else {
        return Err(Error::Contract("expected CONFIG MANIFEST LOG_ENDPOINT"));
    };
    let config = serde_json::from_slice(&std::fs::read(config)?)?;
    let factory = openpilot_logging::producer::Factory::new(endpoint.clone())?;
    let mut adapter = Native::new(config, factory.logger());
    let slot = adapter.get_target_slot_number()?;
    adapter.flash_agnos_update(Path::new(manifest), slot)?;
    println!("{}", serde_json::json!({"target_slot": slot}));
    Ok(())
}
