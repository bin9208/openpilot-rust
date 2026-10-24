//! Original orchestrator calls, side effects and failure precedence.
#![cfg(feature = "encoder-runtime-native")]
use openpilot_dev_tools::{
    encoder_runtime_check::{execute, Options, Peers},
    seal::Result,
};
use serde_json::{json, Value};
use std::{fs, path::Path};
struct Peer {
    scenario: String,
    calls: Vec<Value>,
}
impl Peers for Peer {
    fn create(
        &mut self,
        binary: &Path,
        directory: &Path,
        case: &str,
        native: &Path,
        cpus: &[usize],
    ) -> Result<()> {
        self.calls
            .push(json!(["init", binary, directory, case, native, cpus]));
        fs::create_dir(directory)?;
        let kind = directory
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .rsplit('-')
            .next()
            .unwrap();
        if self.scenario == format!("{kind}-init") {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn exercise(&mut self, kind: &str, case: &str) -> Result<()> {
        self.calls.push(json!(["exercise", kind, case]));
        if self.scenario == format!("{kind}-exercise") {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn close(&mut self, kind: &str, case: &str) -> Result<()> {
        self.calls.push(json!(["close", kind, case]));
        if self.scenario == format!("{kind}-close") {
            return Err(self.scenario.clone().into());
        }
        Ok(())
    }
    fn rows(&self, kind: &str) -> Result<Value> {
        Ok(
            json!({"roadEncodeData":[{"sample":if self.scenario=="rows" && kind=="rust" {2} else {1}}]}),
        )
    }
}
#[test]
fn original_main_success_selection_duplicate_and_failure_effects() {
    let corpus: Value = serde_json::from_str(include_str!(
        "fixtures/encoder-runtime-check/main-original.json"
    ))
    .unwrap();
    for c in corpus["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("source"), b"source").unwrap();
        fs::write(root.join("rust"), b"rust").unwrap();
        fs::create_dir(root.join("native")).unwrap();
        let mut peer = Peer {
            scenario: c["name"].as_str().unwrap().into(),
            calls: vec![],
        };
        let opts = Options {
            source: root.join("source"),
            binary: root.join("rust"),
            output: root.join("output"),
            native: root.join("native"),
            cases: c["selected"]
                .as_array()
                .map(|v| v.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
                .unwrap_or_default(),
        };
        let mut stdout = vec![];
        let result = execute(
            &opts,
            &[2, 4],
            if c["name"] == "space" {
                (1 << 30) - 1
            } else {
                1 << 30
            },
            &mut peer,
            &mut stdout,
        );
        assert_eq!(
            result.err().map(|e| e.to_string()),
            if c["name"] == "space" {
                Some("runtime capture requires 1 GiB operation growth".to_owned())
            } else {
                c["error"].as_str().map(str::to_owned)
            },
            "{}",
            c["name"]
        );
        // Prints occur after each successful pair; preserve their ordering in capture.
        let printed = String::from_utf8(stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        let mut calls = vec![];
        let mut prints = printed.into_iter();
        for call in peer.calls {
            let print = call[0] == "close"
                && call[1] == "rust"
                && !["rows", "rust-close", "rust-exercise"].contains(&peer.scenario.as_str());
            calls.push(call);
            if print {
                calls.push(json!(["print", prints.next().unwrap()]));
            }
        }
        assert!(prints.next().is_none());
        let calls: Value = serde_json::from_str(
            &serde_json::to_string(&calls)
                .unwrap()
                .replace(root.to_str().unwrap(), "$ROOT"),
        )
        .unwrap();
        assert_eq!(calls, c["calls"], "{}", c["name"]);
        for (path, text) in c["artifacts"].as_object().unwrap() {
            assert_eq!(
                fs::read_to_string(opts.output.join(path)).unwrap(),
                text.as_str().unwrap(),
                "{} {path}",
                c["name"]
            );
        }
        let count = fs::read_dir(&opts.output)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count();
        assert_eq!(count, c["artifacts"].as_object().unwrap().len());
    }
}
