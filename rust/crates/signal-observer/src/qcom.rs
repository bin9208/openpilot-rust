//! Fixed-graph QCOM session adapter. Admission authenticates kernels separately.
use crate::{
    neural::{Inference, Tensor},
    neural_sequence::Error,
};

/// Own one native model. Implementations must retain output storage through the
/// read and release all model/device allocations when dropped.
pub trait Runner {
    fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error>;
    fn run(&mut self) -> Result<(), Error>;
    fn read_output(&self, name: &str) -> Result<&[u8], Error>;
}
#[derive(Clone)]
pub struct Binding {
    pub name: String,
    pub shape: Vec<i64>,
}
impl Binding {
    pub fn bytes(&self) -> Result<usize, Error> {
        if self.name.is_empty() || self.shape.is_empty() {
            return Err("empty QCOM binding".into());
        }
        self.shape.iter().try_fold(4_usize, |bytes, dimension| {
            let dimension = usize::try_from(*dimension)
                .ok()
                .filter(|value| *value > 0)
                .ok_or("invalid QCOM binding shape")?;
            bytes
                .checked_mul(dimension)
                .filter(|bytes| *bytes <= 512 << 20)
                .ok_or_else(|| "QCOM binding size limit".into())
        })
    }
}
pub struct Session<R: Runner> {
    runner: R,
    input: Binding,
    output: Binding,
    failed: bool,
}
impl<R: Runner> Session<R> {
    pub fn new(runner: R, input: Binding, output: Binding) -> Result<Self, Error> {
        input.bytes()?;
        output.bytes()?;
        Ok(Self {
            runner,
            input,
            output,
            failed: false,
        })
    }
    fn execute(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error> {
        if self.failed {
            return Err("QCOM session failed; owner must be replaced".into());
        }
        if name != self.input.name
            || input.shape != self.input.shape
            || input.values.len().checked_mul(4) != Some(self.input.bytes()?)
        {
            return Err("QCOM input shape/name contract changed".into());
        }
        let bytes: Vec<_> = input
            .values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        self.runner.write_input(name, &bytes)?;
        self.runner.run()?;
        let output = self.runner.read_output(&self.output.name)?;
        if output.len() != self.output.bytes()? {
            return Err("QCOM output byte contract changed".into());
        }
        Ok(Tensor {
            shape: self.output.shape.clone(),
            values: output
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                .collect(),
        })
    }
}
impl<R: Runner> Inference for Session<R> {
    fn run(&mut self, name: &str, input: &Tensor) -> Result<Tensor, Error> {
        let result = self.execute(name, input);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    struct Inert {
        trace: Rc<RefCell<Vec<String>>>,
        output: Vec<u8>,
        fail: bool,
    }
    impl Drop for Inert {
        fn drop(&mut self) {
            self.trace.borrow_mut().push("drop".into());
        }
    }
    impl Runner for Inert {
        fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
            self.trace
                .borrow_mut()
                .push(format!("write:{name}:{}", bytes.len()));
            assert_eq!(bytes, [0, 0, 0, 128, 0, 0, 192, 127]);
            Ok(())
        }
        fn run(&mut self) -> Result<(), Error> {
            self.trace.borrow_mut().push("run".into());
            if self.fail {
                Err("injected submit failure".into())
            } else {
                Ok(())
            }
        }
        fn read_output(&self, name: &str) -> Result<&[u8], Error> {
            self.trace.borrow_mut().push(format!("read:{name}"));
            Ok(&self.output)
        }
    }
    #[test]
    fn fixed_session_preserves_bits_orders_sync_read_and_poison_releases_owner() {
        for fail in [false, true] {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let runner = Inert {
                trace: trace.clone(),
                output: vec![0, 0, 0, 128, 0x21, 0, 192, 127],
                fail,
            };
            let binding = |name: &str| Binding {
                name: name.into(),
                shape: vec![1, 2],
            };
            let mut session = Session::new(runner, binding("images"), binding("output")).unwrap();
            let input = Tensor {
                shape: vec![1, 2],
                values: vec![-0., f32::from_bits(0x7fc00000)],
            };
            let result = session.run("images", &input);
            if fail {
                assert!(result.is_err());
                assert!(session.run("images", &input).is_err());
            } else {
                let result = result.unwrap();
                assert_eq!(
                    result
                        .values
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    [0x80000000, 0x7fc00021]
                );
                assert!(session.run("other", &input).is_err());
                assert!(session.run("images", &input).is_err());
            }
            drop(session);
            let expected = if fail {
                vec!["write:images:8", "run", "drop"]
            } else {
                vec!["write:images:8", "run", "read:output", "drop"]
            };
            assert_eq!(*trace.borrow(), expected);
        }
    }
}

/// Original automatic-child cache prerequisite, preserving short-circuit order.
/// The caller maps the legacy local-cache names to the native package format.
pub fn require_cached(
    check: &mut impl FnMut(&str, &str) -> Result<bool, Error>,
) -> Result<(), Error> {
    for model in ["detector", "classifier"] {
        for suffix in ["pkl", "json"] {
            if !check(model, suffix)? {
                return Err("auto neural mode requires locally validated QCOM caches".into());
            }
        }
    }
    Ok(())
}
/// Python3.12 Path.is_file ignores only its documented missing/path errors.
/// Other stat errors remain construction failures rather than missing caches.
pub fn cached_file(path: &std::path::Path) -> Result<bool, Error> {
    use std::os::unix::ffi::OsStrExt;
    if path.as_os_str().as_bytes().contains(&0) {
        return Ok(false);
    }
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 9 | 40)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
#[cfg(test)]
mod presence_tests {
    use super::*;
    #[test]
    fn final_source_cache_prerequisite_keeps_reads_and_stat_failures() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/qcom-presence-final.json")).unwrap();
        for row in fixture["cases"].as_array().unwrap() {
            let request = &row["request"];
            let mut trace = Vec::new();
            let result = require_cached(&mut |model, suffix| {
                let index = trace.len();
                trace.push(format!("{model}.qcom.{suffix}"));
                if request["fault"]["index"].as_u64() == Some(index as u64) {
                    return Err("injected stat error".into());
                }
                Ok(request["files"][index].as_bool().unwrap())
            });
            let route = match result {
                Ok(()) => {
                    trace.push("run:qcom".into());
                    "qcom"
                }
                Err(error) if error.to_string() == "injected stat error" => "stat_error",
                Err(_) => "missing",
            };
            assert_eq!(
                serde_json::json!({"route":route,"trace":trace}),
                row["expected"]
            );
        }
    }
    #[test]
    fn actual_cache_file_predicate_preserves_missing_directories_nul_and_symlink_loops() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("cache");
        std::fs::write(&file, b"local").unwrap();
        assert!(cached_file(&file).unwrap());
        assert!(!cached_file(root.path()).unwrap());
        assert!(!cached_file(&root.path().join("missing")).unwrap());
        assert!(!cached_file(std::path::Path::new("invalid\0cache")).unwrap());
        let link = root.path().join("loop");
        std::os::unix::fs::symlink("loop", &link).unwrap();
        assert!(!cached_file(&link).unwrap());
    }
}

/// Native data-format mapping of the source's four cache-presence predicates.
pub fn require_native_cached(root: &std::path::Path) -> Result<(), Error> {
    require_cached(&mut |model, suffix| {
        let file = if suffix == "pkl" {
            "graph.json"
        } else {
            "signal-qcom.json"
        };
        cached_file(&root.join("rust/bin/signal-qcom").join(model).join(file))
    })
}
