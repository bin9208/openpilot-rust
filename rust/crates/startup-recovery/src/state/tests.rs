use super::*;
use serde_json::{json, Value};
struct Mock {
    input: Value,
    calls: Vec<&'static str>,
}
impl Mock {
    fn check(&self, name: &str) -> Result<(), String> {
        self.input[name].as_str().map_or(Ok(()), |s| Err(s.into()))
    }
}
impl Backend for Mock {
    fn disable_prompt(&mut self) {
        self.calls.push("prompt_off");
    }
    fn with_lock<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        self.calls.push("lock");
        self.check("lock_error")?;
        let result = run(self);
        self.calls.push("unlock");
        result
    }
    fn pull(&mut self) -> Result<(String, bool), String> {
        self.calls.push("pull");
        self.check("pull_error")?;
        Ok((
            self.input["target"].as_str().unwrap().into(),
            self.input["changed"].as_bool().unwrap(),
        ))
    }
    fn request_rebuild(&mut self) -> Result<(), String> {
        self.calls.push("request");
        self.check("request_error")
    }
    fn reboot(&mut self) -> Result<(), String> {
        self.calls.push("reboot");
        self.check("reboot_error")
    }
}
#[test]
fn final_source_complete_state_and_callback_order() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/startup-recovery-state-final.json"
    ))
    .unwrap();
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        let update = Update::default();
        for (j, step) in row.as_array().unwrap().iter().enumerate() {
            let mut backend = Mock {
                input: step["input"].clone(),
                calls: Vec::new(),
            };
            let actual = match step["op"].as_str().unwrap() {
                "start" | "automatic" | "start_rebuild" => {
                    let started = update.begin(step["op"] == "start_rebuild");
                    if started {
                        backend.calls.push("thread");
                    }
                    json!(started)
                }
                "run" | "auto_run" => {
                    update.run_update(&mut backend, step["op"] == "auto_run");
                    Value::Null
                }
                "rebuild" => {
                    update.run_rebuild(&mut backend);
                    Value::Null
                }
                _ => unreachable!(),
            };
            let state = update.snapshot();
            assert_eq!(actual, step["result"], "result {i}/{j}");
            assert_eq!(
                json!([state.status, state.detail]),
                step["state"],
                "state {i}/{j}"
            );
            assert_eq!(
                json!(update.0.lock().unwrap().pending_reboot),
                step["pending"],
                "pending {i}/{j}"
            );
            assert_eq!(json!(backend.calls), step["calls"], "calls {i}/{j}");
        }
    }
}
