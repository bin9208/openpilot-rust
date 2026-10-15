//! Vehicle half of the pinned Jetson boot gate. The host verifies the signed release.
use super::{Client, Transport};
use crate::{Deadline, Error};
use serde_json::Value;
use std::time::{Duration, Instant};

impl<T: Transport> Client<T> {
    pub(super) fn boot_update(&mut self, hello: &Value) -> Result<(), Error> {
        if hello["carrot_host"] != "jetson" || hello["carrot_boot_update_v1"] != true {
            return Ok(());
        }
        let release = include_bytes!("../../data/host_release.json");
        while self.app.as_ref().is_some_and(|app| (app.connected)()) {
            let deadline = Deadline::after(Duration::from_secs(2))?;
            self.send(0x4003, release, deadline)?;
            self.provision_wifi(false)?;
            let sequence = self.send(12, b"{}", deadline)?;
            let state: Value =
                serde_json::from_slice(&self.expect_setup(13, sequence, deadline, false)?.payload)?;
            let update = state
                .get("carrot_update")
                .filter(|value| value.is_object())
                .filter(|value| serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 16384))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            self.report("updating", "boot-update", Some(hello), None);
            if let Ok(mut status) = self.status.lock() {
                status["host_update"] = update.clone();
            }
            if update["state"] == "ready" {
                break;
            }
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(500) {
                if self.app.as_ref().is_none_or(|app| !(app.connected)()) {
                    return Err(Error::Closed);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        // A gate cannot serve inference. Require a fresh runtime HELLO even after ready.
        Err(Error::Closed)
    }
}
