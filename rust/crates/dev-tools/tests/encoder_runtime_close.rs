//! Source close-finally precedence, including incomplete cleanup after file errors.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::{
    encoder_runtime_process::{close, Cleanup},
    seal::Result,
};
use serde_json::{json, Value};
struct Peer {
    scenario: String,
    actions: Vec<Value>,
    server: bool,
}
impl Cleanup for Peer {
    fn shutdown(&mut self) -> Result<()> {
        self.actions.push(json!(["stop"]));
        if self.scenario == "shutdown-error" {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn drain(&mut self) -> Result<()> {
        self.actions.push(json!(["drain"]));
        if self.scenario == "drain-error" {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn close_files(&mut self) -> Result<()> {
        self.actions.push(json!(["close", "raw"]));
        self.actions.push(json!(["close", "output"]));
        Ok(())
    }
    fn write_packets(&mut self) -> Result<()> {
        self.actions.push(json!(["write", "packets.json", "{}\n"]));
        if self.scenario == "write-error" {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn write_inputs(&mut self) -> Result<()> {
        self.actions.push(json!(["write", "inputs.json", "[]\n"]));
        Ok(())
    }
    fn release_ipc(&mut self) -> Result<()> {
        self.server = false;
        self.actions.push(json!(["clear"]));
        self.actions.push(json!(["collect"]));
        self.actions.push(json!(["rmtree", "shm"]));
        self.actions
            .push(json!(["unlink", "/tmp/PREFIX_visionipc_camerad", true]));
        self.actions
            .push(json!(["unlink", "/tmp/logmessagePREFIX", true]));
        Ok(())
    }
    fn outcome(&mut self) -> Result<()> {
        self.actions.push(json!(["read", "daemon.log"]));
        self.actions.push(json!(["outcome", 0, "log"]));
        if self.scenario == "outcome-error" {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
}
#[test]
fn original_cleanup_effects_and_finally_failure_precedence() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-runtime-check/close-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let mut peer = Peer {
            scenario: c["name"].as_str().unwrap().into(),
            actions: vec![],
            server: true,
        };
        let result = close(&mut peer);
        assert_eq!(
            result.err().map(|e| e.to_string()),
            c["error"].as_str().map(str::to_owned)
        );
        assert_eq!(json!(peer.actions), c["actions"], "{}", c["name"]);
        assert_eq!(peer.server, c["server_present"].as_bool().unwrap());
    }
}
