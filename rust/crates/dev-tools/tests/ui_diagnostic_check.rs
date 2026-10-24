use openpilot_dev_tools::{
    qa_process::{Captured, CommandSpec, Runner},
    seal::Result,
    ui_diagnostic_check::check,
};
use serde_json::Value;
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};
struct Peer<'a> {
    case: &'a Value,
    root: &'a Path,
    output: &'a Path,
    fixtures: PathBuf,
    calls: usize,
    binary: OsString,
}
impl Runner for Peer<'_> {
    fn run(&mut self, spec: &CommandSpec) -> Result<Captured> {
        let call = &self.case["calls"][self.calls];
        self.calls += 1;
        let expected = call["argv"].as_array().unwrap();
        let source = expected[0] == "@PYTHON@";
        let mode = expected[expected.len() - 2].as_str().unwrap();
        let image = self.output.join(format!(
            "{}-{mode}.png",
            if source { "source" } else { "native" }
        ));
        let mut argv = vec![if source {
            OsString::from("reference")
        } else {
            self.binary.clone()
        }];
        if !source {
            argv.push(self.root.as_os_str().to_owned());
        }
        argv.extend([OsString::from(mode), image.as_os_str().to_owned()]);
        assert_eq!(spec.effective_argv(), argv);
        let env = spec.effective_env();
        assert_eq!(env[OsStr::new("DISPLAY")], ":owned");
        assert_eq!(env[OsStr::new("OFFSCREEN")], "1");
        assert_eq!(env[OsStr::new("PYTHONPATH")], self.root.as_os_str());
        assert!(spec.inherit_stdin && spec.merge_output);
        assert_eq!(spec.timeout, std::time::Duration::MAX);
        assert_eq!(spec.stdout_limit, usize::MAX);
        if self.case["name"] == "source-error" && source
            || self.case["name"] == "native-error" && !source
        {
            return Ok(Captured {
                code: 7,
                stdout: vec![],
                stderr: vec![],
            });
        }
        let name = image.file_name().unwrap().to_str().unwrap();
        if let Some(hash) = self.case["images"].get(name).and_then(Value::as_str) {
            fs::write(&image, fs::read(self.fixtures.join("blobs").join(hash))?)?;
        }
        Ok(Captured {
            code: 0,
            stdout: vec![],
            stderr: vec![],
        })
    }
}
#[test]
fn unchanged_original_checker_process_arrays_output_and_failure_artifacts() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-diagnostics");
    let captured: Value = serde_json::from_str(include_str!(
        "fixtures/ui-diagnostics/checker-original.json"
    ))
    .unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for case in captured["cases"].as_array().unwrap() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output");
        let raw = case["binary_hex"].as_str().unwrap();
        let binary = OsString::from_vec(
            (0..raw.len())
                .step_by(2)
                .map(|n| u8::from_str_radix(&raw[n..n + 2], 16).unwrap())
                .collect(),
        );
        let mut peer = Peer {
            case,
            root: &repository,
            output: &output,
            fixtures: fixtures.clone(),
            calls: 0,
            binary: binary.clone(),
        };
        let mut printed = vec![];
        let result = check(
            &repository,
            Path::new(&binary),
            Path::new("reference"),
            &output,
            OsStr::new(":owned"),
            &mut peer,
            &mut printed,
        );
        assert_eq!(result.is_ok(), case["exit"] == 0, "{}", case["name"]);
        assert_eq!(peer.calls, case["calls"].as_array().unwrap().len());
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            case["stdout"].as_str().unwrap(),
            "{} stdout",
            case["name"]
        );
        let observed = fs::read_dir(&output)
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension() != Some(OsStr::new("png")))
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
        assert_eq!(observed, expected, "{} files", case["name"]);
    }
}
