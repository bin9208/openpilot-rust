//! Native QCOM graph admission and model ownership. No CPU fallback is available.
use crate::{
    neural::{Neural, SourceSort},
    neural_sequence::Error,
    qcom::{Binding, Runner, Session},
};
use openpilot_model_runtime::qcom::{QcomBundle, QcomModel};
use openpilot_runtime_package::VerifiedManifest;
use serde_json::Value;
use std::path::Path;
pub type Network<R> = Neural<Session<R>, Session<R>, SourceSort>;
const TREE: &str = "2c7501785916e365318c31976d4aef5b6e44a332";
pub struct Admitted {
    bundle: QcomBundle,
    input: Binding,
    output: Binding,
}
fn shape(value: &Value) -> Result<Vec<i64>, Error> {
    value
        .as_array()
        .ok_or("missing fixed tensor shape")?
        .iter()
        .map(|dimension| {
            dimension
                .as_i64()
                .ok_or_else(|| "invalid fixed tensor dimension".into())
        })
        .collect()
}
fn admit(package: &VerifiedManifest, name: &str) -> Result<Admitted, Error> {
    let pin: Value = serde_json::from_str(include_str!("../../../upstream/signal-runtime.json"))?;
    let expected = &pin["models"][name];
    let directory = std::path::PathBuf::from(format!("rust/assets/signal-qcom/{name}"));
    let receipt: Value = serde_json::from_slice(
        &package.read_artifact(&directory.join("signal-qcom.json"), 4 << 20)?,
    )?;
    let identity = &receipt["identity"];
    if identity["schema"] != 1
        || identity["source_commit"] != pin["source_commit"]
        || identity["tinygrad_tree"] != TREE
        || identity["model"] != name
        || identity["model_sha256"] != expected["sha256"]
        || identity["input"] != expected["input"]
        || identity["input_shape"] != expected["shape"]
        || identity["output"] != expected["output"]
        || identity["output_shape"] != expected["output_shape"]
    {
        return Err("signal QCOM bundle disagrees with pinned model/runtime contract".into());
    }
    let (graph_path, digest) = package.artifact(&directory.join("graph.json"))?;
    let bundle = QcomBundle::load_verified(
        graph_path.parent().ok_or("QCOM graph directory missing")?,
        digest,
    )?;
    let input = Binding {
        name: expected["input"]
            .as_str()
            .ok_or("input name missing")?
            .into(),
        shape: shape(&expected["shape"])?,
    };
    let output = Binding {
        name: expected["output"]
            .as_str()
            .ok_or("output name missing")?
            .into(),
        shape: shape(&expected["output_shape"])?,
    };
    if bundle.input_size(&input.name) != Some(input.bytes()?)
        || bundle.output_size(&output.name) != Some(output.bytes()?)
        || bundle.kernel_count() == 0
    {
        return Err("signal QCOM graph binding/kernel contract".into());
    }
    Ok(Admitted {
        bundle,
        input,
        output,
    })
}
/// A factory boundary allows admission and RAII cleanup to be checked without
/// constructing a GPU driver. Production uses the actual QCOM model below.
pub trait Factory {
    type Model: Runner;
    fn open(&mut self, bundle: QcomBundle) -> Result<Self::Model, Error>;
}
pub fn open_with<F: Factory>(
    root: &Path,
    manifest_digest: &str,
    factory: &mut F,
) -> Result<Network<F::Model>, Error> {
    let package = VerifiedManifest::read_expected(root, manifest_digest)?;
    // Admit both full snapshots before the first device is opened.
    let detector = admit(&package, "detector")?;
    let classifier = admit(&package, "classifier")?;
    let detector = Session::new(
        factory.open(detector.bundle)?,
        detector.input,
        detector.output,
    )?;
    let classifier = Session::new(
        factory.open(classifier.bundle)?,
        classifier.input,
        classifier.output,
    )?;
    Ok(Neural {
        detector,
        classifier,
        sort: SourceSort,
    })
}
impl Runner for QcomModel {
    fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
        self.write_input(name, bytes).map_err(Error::from)
    }
    fn run(&mut self) -> Result<(), Error> {
        self.run().map_err(Error::from)
    }
    fn read_output(&self, name: &str) -> Result<&[u8], Error> {
        self.read_output(name).map_err(Error::from)
    }
}
struct Native;
impl Factory for Native {
    type Model = QcomModel;
    #[allow(unsafe_code)]
    fn open(&mut self, bundle: QcomBundle) -> Result<QcomModel, Error> {
        // SAFETY: open_with obtains this snapshot only after the trusted launcher
        // manifest digest, source/model identities and all package files verify.
        // The authenticated package producer owns the kernel semantic contract;
        // hashes alone from arbitrary adjacent metadata do not establish trust.
        unsafe { QcomModel::from_bundle(bundle, 8).map_err(Error::from) }
    }
}
/// # Safety
/// The digest must originate from the trusted launcher after its normal package
/// verification, never from untrusted adjacent model metadata or settings.
#[allow(unsafe_code)]
pub unsafe fn open(root: &Path, manifest_digest: &str) -> Result<Network<QcomModel>, Error> {
    open_with(root, manifest_digest, &mut Native)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_runtime_package::{Artifact, Manifest};
    use sha2::{Digest, Sha256};
    use std::{cell::RefCell, fs, rc::Rc};
    fn fixture() -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        let pins: Value =
            serde_json::from_str(include_str!("../../../upstream/signal-runtime.json")).unwrap();
        let mut files = Vec::new();
        let mut save = |path: std::path::PathBuf, data: Vec<u8>| {
            fs::create_dir_all(root.path().join(&path).parent().unwrap()).unwrap();
            fs::write(root.path().join(&path), &data).unwrap();
            files.push(Artifact {
                path,
                sha256: format!("{:x}", Sha256::digest(&data)),
                executable: false,
            });
        };
        let binary = include_bytes!("../../model-runtime/tests/fixtures/qcom/buffer_add.bin");
        for name in ["detector", "classifier"] {
            let pin = &pins["models"][name];
            let directory = std::path::PathBuf::from(format!("rust/assets/signal-qcom/{name}"));
            let input = Binding {
                name: pin["input"].as_str().unwrap().into(),
                shape: shape(&pin["shape"]).unwrap(),
            };
            let output = Binding {
                name: "output".into(),
                shape: shape(&pin["output_shape"]).unwrap(),
            };
            let graph = serde_json::json!({"version":1,"backend":"qcom-cl","arch":"a630","weights_sha256":format!("{:x}",Sha256::digest([])),
                "allocations":[{"bytes":input.bytes().unwrap(),"weight_offset":null},{"bytes":output.bytes().unwrap(),"weight_offset":null}],
                "views":[{"allocation":0,"offset":0,"bytes":input.bytes().unwrap()},{"allocation":1,"offset":0,"bytes":output.bytes().unwrap()}],
                "inputs":[{"name":input.name,"view":0}],"outputs":[{"name":"output","view":1}],
                "kernels":[{"name":"buffer_add","binary_sha256":format!("{:x}",Sha256::digest(binary)),"binary_bytes":binary.len(),"arguments":[[{"kind":"buffer"}],[{"kind":"buffer"}]]}],
                "calls":[{"op":"kernel","kernel":0,"views":[1,0],"scalars":[],"global":[output.bytes().unwrap()/4,1,1],"local":[1,1,1]}]});
            save(
                directory.join("graph.json"),
                serde_json::to_vec(&graph).unwrap(),
            );
            save(directory.join("weights.bin"), vec![]);
            save(directory.join("kernel-0.bin"), binary.to_vec());
            let identity = serde_json::json!({"schema":1,"source_commit":pins["source_commit"],"tinygrad_tree":TREE,"model":name,"model_sha256":pin["sha256"],
                "input":pin["input"],"input_shape":pin["shape"],"output":pin["output"],"output_shape":pin["output_shape"],
                "fixture_scope":"inert admission/lifecycle only; buffer_add is NOT either real signal model"});
            save(
                directory.join("signal-qcom.json"),
                serde_json::to_vec(&serde_json::json!({"identity":identity})).unwrap(),
            );
        }
        let manifest = Manifest {
            schema: 1,
            source_commit: "0123456789abcdef0123456789abcdef01234567".into(),
            agnos_version: "19.8-carrot-bt1".into(),
            files,
        };
        fs::write(
            root.path().join("build.json"),
            serde_json::to_vec(
                &serde_json::json!({"openpilot":{"git_commit":manifest.source_commit}}),
            )
            .unwrap(),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        fs::write(root.path().join("rust/runtime-manifest.json"), bytes).unwrap();
        (root, digest)
    }
    struct Inert {
        trace: Rc<RefCell<Vec<String>>>,
        name: String,
        output: Vec<u8>,
    }
    impl Drop for Inert {
        fn drop(&mut self) {
            self.trace.borrow_mut().push(format!("drop:{}", self.name));
        }
    }
    impl Runner for Inert {
        fn write_input(&mut self, name: &str, bytes: &[u8]) -> Result<(), Error> {
            self.trace
                .borrow_mut()
                .push(format!("write:{name}:{}", bytes.len()));
            Ok(())
        }
        fn run(&mut self) -> Result<(), Error> {
            self.trace.borrow_mut().push(format!("run:{}", self.name));
            Ok(())
        }
        fn read_output(&self, _name: &str) -> Result<&[u8], Error> {
            Ok(&self.output)
        }
    }
    struct InertFactory {
        trace: Rc<RefCell<Vec<String>>>,
        opened: usize,
        fail_second: bool,
    }
    impl Factory for InertFactory {
        type Model = Inert;
        fn open(&mut self, bundle: QcomBundle) -> Result<Inert, Error> {
            self.opened += 1;
            let name = if self.opened == 1 {
                "detector"
            } else {
                "classifier"
            };
            self.trace.borrow_mut().push(format!("open:{name}"));
            if self.opened == 2 && self.fail_second {
                return Err("injected second-driver failure".into());
            }
            Ok(Inert {
                trace: self.trace.clone(),
                name: name.into(),
                output: vec![0; bundle.output_size("output").unwrap()],
            })
        }
    }
    #[test]
    fn verified_two_model_admission_warmup_and_cleanup_open_no_gpu() {
        let (root, digest) = fixture();
        for fail in [false, true] {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let mut factory = InertFactory {
                trace: trace.clone(),
                opened: 0,
                fail_second: fail,
            };
            let result = open_with(root.path(), &digest, &mut factory);
            if fail {
                assert!(result.is_err());
                assert_eq!(
                    *trace.borrow(),
                    ["open:detector", "open:classifier", "drop:detector"]
                );
            } else {
                let mut network = result.unwrap();
                network.warm_up().unwrap();
                drop(network);
                let trace = trace.borrow();
                assert_eq!(
                    trace
                        .iter()
                        .filter(|value| value.starts_with("run:"))
                        .count(),
                    6
                );
                assert_eq!(
                    &trace[trace.len() - 2..],
                    ["drop:detector", "drop:classifier"]
                );
            }
        }
        fs::write(
            root.path()
                .join("rust/assets/signal-qcom/classifier/graph.json"),
            b"changed",
        )
        .unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut factory = InertFactory {
            trace: trace.clone(),
            opened: 0,
            fail_second: false,
        };
        assert!(open_with(root.path(), &digest, &mut factory).is_err());
        assert!(trace.borrow().is_empty());
    }
}
