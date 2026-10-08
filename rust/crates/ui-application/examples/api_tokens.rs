use openpilot_timed::{
    clock::{datetime, Clock},
    Error,
};
use openpilot_ui_application::api::{self, TokenCache, TokenPaths};
use serde::Deserialize;
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};
#[derive(Deserialize)]
struct Step {
    identity: String,
    wall: f64,
    monotonic: u64,
    pair: bool,
}
impl Clock for Step {
    fn wall_nanos(&self) -> Result<u64, Error> {
        use num_traits::ToPrimitive;
        (self.wall * 1e9)
            .to_u64()
            .ok_or(Error::Contract("fixture time invalid"))
    }
    fn monotonic(&self) -> Result<u64, Error> {
        Ok(self.monotonic)
    }
    fn local(&self, epoch: f64) -> Result<chrono::NaiveDateTime, Error> {
        Ok(datetime(epoch)?.naive_utc())
    }
    fn sleep(&self, _: Duration, _: &AtomicBool) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, persist, input] = args.as_slice() else {
        return Err("api_tokens PERSIST INPUT".into());
    };
    let steps: Vec<Step> = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut cache = TokenCache::default();
    let mut output = Vec::new();
    for step in steps {
        let result = if step.pair {
            api::pairing(&step.identity, step.wall, Path::new(persist))
        } else {
            cache.get(
                &step.identity,
                &step,
                TokenPaths {
                    persist: Path::new(persist),
                    systemd: &Path::new(persist).join("absent-systemd"),
                },
            )
        };
        output.push(match result {
            Ok(token) => serde_json::json!({"token":token}),
            Err(error) => serde_json::json!({"error":error.to_string()}),
        });
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
