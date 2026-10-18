#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_network_check::check,
};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};
struct Peer<'a> {
    case: &'a Value,
    root: &'a Path,
    output: &'a Path,
    fixtures: &'a Path,
    calls: usize,
    binary: OsString,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = &self.case["calls"][self.calls];
        self.calls += 1;
        let expected = call["argv"].as_array().unwrap();
        let source = expected[0] == "@PYTHON@";
        let substitute = |s: &str| {
            s.replace("@ROOT@", self.root.to_str().unwrap())
                .replace("@OUTPUT@", self.output.to_str().unwrap())
        };
        let mut args = vec![if source {
            "reference".into()
        } else {
            self.binary.clone()
        }];
        if !source {
            args.push(self.root.as_os_str().to_owned());
        }
        for v in &expected[expected.len() - 2..] {
            args.push(substitute(v.as_str().unwrap()).into());
        }
        assert_eq!(spec.effective_argv(), args);
        let env = spec.effective_env();
        for name in ["DISPLAY", "OFFSCREEN", "PYTHONPATH"] {
            assert_eq!(
                env[OsStr::new(name)],
                OsStr::new(&substitute(call["env"][name].as_str().unwrap()))
            );
        }
        assert!(spec.inherit_stdin && spec.merge_output);
        assert_eq!(spec.timeout, std::time::Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        let scene = Path::new(&args[args.len() - 2]);
        let name = scene.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            fs::read_to_string(scene)?,
            self.case["files"][name].as_str().unwrap()
        );
        if self.case["name"] == "source-error" && source
            || self.case["name"] == "native-error" && !source
        {
            return Ok(Captured {
                code: 7,
                stdout: vec![],
                stderr: vec![],
            });
        }
        let image = Path::new(args.last().unwrap());
        let name = image.file_name().unwrap().to_str().unwrap();
        if let Some(hash) = self.case["images"].get(name).and_then(Value::as_str) {
            fs::write(image, fs::read(self.fixtures.join("blobs").join(hash))?)?;
        }
        let state = image.with_extension("json");
        let name = state.file_name().unwrap().to_str().unwrap();
        if let Some(value) = self.case["files"].get(name).and_then(Value::as_str) {
            fs::write(state, value)?;
        }
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn unchanged_original_network_comparison_process_and_artifacts() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-network");
    let captures: Value =
        serde_json::from_str(include_str!("fixtures/ui-network/checker-original.json")).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for case in captures["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let binary = OsString::from("native");
        let mut peer = Peer {
            case,
            root: &root,
            output: &output,
            fixtures: &fixtures,
            calls: 0,
            binary: binary.clone(),
        };
        let mut printed = vec![];
        let result = check(
            &root,
            Path::new(&binary),
            Path::new("reference"),
            &output,
            OsStr::new(":owned"),
            &mut peer,
            &mut printed,
        );
        assert_eq!(
            result.is_ok(),
            case["exit"] == 0,
            "{} {:?}",
            case["name"],
            result.err()
        );
        assert_eq!(
            peer.calls,
            case["calls"].as_array().unwrap().len(),
            "{}",
            case["name"]
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            case["stdout"].as_str().unwrap(),
            "{} stdout",
            case["name"]
        );
        let files = fs::read_dir(&output)
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.is_file() && p.extension() != Some(OsStr::new("png")))
            .map(|p| {
                (
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    fs::read_to_string(p).unwrap(),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let expected = case["files"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect();
        assert_eq!(files, expected, "{} files", case["name"]);
    }
}
