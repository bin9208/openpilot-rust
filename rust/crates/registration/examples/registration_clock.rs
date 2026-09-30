use openpilot_registration::{registration_expiration, system_time_unix_seconds};
use serde_json::json;
use std::{
    io::{self, BufRead},
    time::{Duration, UNIX_EPOCH},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let (seconds, nanoseconds): (i64, u32) = serde_json::from_str(&line?)?;
        let time = if seconds >= 0 {
            UNIX_EPOCH.checked_add(Duration::from_secs(seconds.unsigned_abs()))
        } else {
            UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
        }
        .and_then(|value| value.checked_add(Duration::from_nanos(u64::from(nanoseconds))));
        let converted = time.and_then(|time| system_time_unix_seconds(time).ok());
        println!(
            "{}",
            json!({"seconds":converted,"expiration":converted.and_then(|seconds|registration_expiration(seconds).ok())})
        );
    }
    Ok(())
}
