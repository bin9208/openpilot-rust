//! Source-captured exercise schedules and each original assertion branch.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::{
    encoder_runtime_exercise::{exercise, Driver},
    seal::Result,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
struct Peer {
    case: String,
    fault: String,
    active: bool,
    restarted: bool,
    actions: Vec<Value>,
    rows: BTreeMap<String, Vec<Value>>,
}
impl Peer {
    fn append(&mut self, frame: u32) {
        for (s, rows) in &mut self.rows {
            if s == "thumbnail" {
                if frame == 100 && self.fault != "missing-thumbnail"
                    || self.fault == "lag-thumbnail" && frame == 106
                {
                    rows.push(json!({"frameId":frame}));
                }
            } else {
                let segment = if self.fault == "rotation"
                    || self.case == "--carrot-vision-road" && self.fault != "active-rotation"
                {
                    0
                } else {
                    u32::from(frame >= 180)
                };
                rows.push(json!({"idx":{"frameId":frame,"segmentNum":segment}}));
            }
        }
    }
}
impl Driver for Peer {
    fn send(&mut self, frame: u32, pause: f64) -> Result<()> {
        self.actions.push(json!([
            "send",
            frame,
            if pause == 0.0 { json!(0) } else { json!(pause) },
            if self.active { "1" } else { "0" }
        ]));
        if self.case == "main-lag" && frame < 170
            || self.case == "main-restart" && self.restarted && self.fault != "restart-output"
            || self.case == "--carrot-vision-road"
                && !self.active
                && !(self.fault == "prewarm" && frame == 100
                    || self.fault == "inactive" && frame == 206)
        {
            return Ok(());
        }
        self.append(frame);
        Ok(())
    }
    fn await_frame(&mut self, frame: u32) -> Result<()> {
        self.actions.push(json!(["await", frame]));
        if self.case == "main-lag" && frame == 169 {
            for n in if self.fault == "overwritten" {
                100
            } else {
                106
            }..170
            {
                self.append(n)
            }
        }
        Ok(())
    }
    fn set_active(&mut self, active: bool) -> Result<()> {
        self.active = active;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.actions.push(json!(["signal", 19]));
        Ok(())
    }
    fn resume(&mut self) -> Result<()> {
        self.actions.push(json!(["signal", 18]));
        Ok(())
    }
    fn restart(&mut self) -> Result<()> {
        self.actions.push(json!(["collect"]));
        self.actions.push(json!(["restart"]));
        self.restarted = true;
        Ok(())
    }
    fn drain(&mut self) -> Result<()> {
        self.actions.push(json!(["drain"]));
        Ok(())
    }
    fn rows(&self) -> &BTreeMap<String, Vec<Value>> {
        &self.rows
    }
}
#[test]
fn original_schedule_rotation_activation_overwrite_restart_and_failures() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-runtime-check/exercise-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let mut peer = Peer {
            case: c["case"].as_str().unwrap().into(),
            fault: c["fault"].as_str().unwrap().into(),
            active: false,
            restarted: false,
            actions: vec![],
            rows: c["counts"]
                .as_object()
                .unwrap()
                .keys()
                .map(|s| (s.clone(), vec![]))
                .collect(),
        };
        let result = exercise(c["case"].as_str().unwrap(), &mut peer);
        assert_eq!(
            result.err().map(|e| e.to_string()),
            c["error"].as_str().map(str::to_owned),
            "{} {}",
            c["case"],
            c["fault"]
        );
        assert_eq!(
            json!(peer.actions),
            c["actions"],
            "{} {}",
            c["case"],
            c["fault"]
        );
        assert_eq!(
            json!(peer
                .rows
                .iter()
                .map(|(k, v)| (k.clone(), v.len()))
                .collect::<BTreeMap<_, _>>()),
            c["counts"]
        );
    }
}
