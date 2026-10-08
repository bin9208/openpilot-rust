use openpilot_timed::{
    clock::{datetime, Clock},
    Error,
};
use openpilot_ui_application::{
    context::PrimeStatus,
    params::Read,
    services::{
        firehose::{self, Firehose},
        prime::{self, Prime},
        Api,
    },
};
use serde::Deserialize;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
struct FixedClock;
impl Clock for FixedClock {
    fn wall_nanos(&self) -> Result<u64, Error> {
        Ok(1790812800000000000)
    }
    fn monotonic(&self) -> Result<u64, Error> {
        Ok(1000000000)
    }
    fn local(&self, epoch: f64) -> Result<chrono::NaiveDateTime, Error> {
        Ok(datetime(epoch)?.naive_utc())
    }
    fn sleep(&self, _: Duration, _: &AtomicBool) {}
}
#[derive(Deserialize)]
struct Scene {
    mode: String,
    initial: Option<String>,
    environment: Option<String>,
    identities: Vec<String>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, persist, host, input, output] = args.as_slice() else {
        return Err("api_services ROOT PERSIST HOST INPUT OUTPUT".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(input)?)?;
    let params = Arc::new(openpilot_params::Params::open(
        &Path::new(output).join("params"),
        "d",
    )?);
    let key = if scene.mode == "prime" {
        "PrimeType"
    } else {
        firehose::KEY
    };
    if let Some(value) = scene.initial {
        params.put(key, value.as_bytes())?;
    }
    let mut api = Api::new(host.into(), root.into(), persist.into());
    api.clock = Arc::new(FixedClock);
    api.systemd = Path::new(output).join("absent-systemd");
    let mut results = Vec::new();
    if scene.mode == "prime" {
        let status = Arc::new(PrimeStatus::new(prime::initial(
            params.as_ref(),
            scene.environment.as_deref(),
        )?));
        let mut worker = Prime::new(status.clone(), params.clone(), api)?;
        results.push(
            serde_json::json!({"value":status.get(),"params":params.string(key)?,"error":false}),
        );
        for identity in scene.identities {
            params.put("DongleId", identity.as_bytes())?;
            let error = worker.fetch().is_err();
            results.push(serde_json::json!({"value":status.get(),"params":params.string(key)?,"error":error}));
        }
    } else {
        let mut worker = Firehose::new(params.clone(), api)?;
        let value = worker
            .count
            .lock()
            .map_err(|_| "count poisoned")?
            .to_json()?;
        results.push(
            serde_json::json!({"value_json":value,"params":params.string(key)?,"error":false}),
        );
        for identity in scene.identities {
            params.put("DongleId", identity.as_bytes())?;
            let error = worker.fetch().is_err();
            let value = worker
                .count
                .lock()
                .map_err(|_| "count poisoned")?
                .to_json()?;
            results.push(
                serde_json::json!({"value_json":value,"params":params.string(key)?,"error":error}),
            );
        }
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
