use openpilot_runtime_installer::model_inputs;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
pub fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Owned model input fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
pub fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "Owned model input fixture"]);
}
pub fn file(path: &Path, data: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    fs::write(path, data).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
}
pub struct Fixture {
    pub temporary: tempfile::TempDir,
    pub source: PathBuf,
    pub provider: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        Self::with_scope(include_bytes!("../../../../native-model-source-scope.json"))
    }
    pub fn with_scope(scope: &[u8]) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source");
        let provider = temporary.path().join("models");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&provider).unwrap();
        let value: Value = serde_json::from_slice(scope).unwrap();
        for field in ["python_files", "compiler_files", "plan_files"] {
            for name in value[field].as_array().unwrap() {
                file(
                    &source.join(name.as_str().unwrap()),
                    b"owned source input\n",
                );
            }
        }
        for field in ["python_roots", "compiler_roots"] {
            for name in value[field].as_array().unwrap() {
                file(
                    &source.join(name.as_str().unwrap()).join("owned.py"),
                    b"owned source namespace\n",
                );
            }
        }
        file(&source.join("rust/native-model-source-scope.json"), scope);
        file(
            &source.join("openpilot/selfdrive/modeld/models/driving_supercombo.onnx"),
            format!(
                "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 17\n",
                "a".repeat(64)
            )
            .as_bytes(),
        );
        file(
            &source.join("openpilot/selfdrive/modeld/models/dmonitoring_model.onnx"),
            b"owned ordinary model",
        );
        git(&source, &["init", "-q"]);
        commit(&source);
        let source_id = String::from_utf8(git(&source, &["rev-parse", "HEAD"]))
            .unwrap()
            .trim()
            .to_owned();
        let tree_id = String::from_utf8(git(&source, &["rev-parse", "HEAD^{tree}"]))
            .unwrap()
            .trim()
            .to_owned();
        let namespace = model_inputs::source_namespace(&source, false).unwrap();
        let compiler_namespace = model_inputs::source_namespace(&source, true).unwrap();
        let build = json!({"schema":1,"source":{"commit":source_id,"tree":tree_id},
            "compiler_sources_sha256":compiler_namespace,"compiler_sha256":"c".repeat(64),"compiler_bytes":80,
            "features":["default","native-skip-miri"],"rust_toolchain_identity":"b".repeat(64),
            "rust_toolchain_tree_sha256":"d".repeat(64),"cargo_sources_tree_sha256":"e".repeat(64),
            "system_sdk_receipt_sha256":"f".repeat(64),"cargo_lock_sha256":hash(&fs::read(source.join("rust/Cargo.lock")).unwrap())});
        let mut build_inputs = build.as_object().unwrap().clone();
        build_inputs.remove("source");
        let mut receipts = BTreeMap::new();
        for kind in ["driving", "driver"] {
            let mut entries = Vec::new();
            let mut output = Vec::new();
            for camera in [[1928, 1208], [1344, 760]] {
                let tag = format!("{kind}-{}x{}", camera[0], camera[1]);
                let model = if kind == "driving" {
                    json!({"path":"openpilot/selfdrive/modeld/models/driving_supercombo.onnx","sha256":"a".repeat(64),"bytes":17,"lfs":true})
                } else {
                    json!({"path":"openpilot/selfdrive/modeld/models/dmonitoring_model.onnx","sha256":hash(b"owned ordinary model"),"bytes":20,"lfs":false})
                };
                let nv12 = json!({"stride":1,"y_height":2,"uv_height":3,"bytes":4});
                let pipeline=serde_json::to_vec(&json!({"kind":kind,"camera":camera,"nv12":nv12,"sources":{"onnx":model["sha256"]}})).unwrap();
                let graph = b"{\"owned_publication_fixture\":true}".to_vec();
                let entry = json!({"kind":kind,"camera":camera,"directory":tag,"graph_sha256":hash(&graph),"pipeline_sha256":hash(&pipeline)});
                let data = BTreeMap::from([
                    ("graph.json", graph),
                    ("pipeline.json", pipeline),
                    ("weights.bin", b"owned weights".to_vec()),
                ]);
                let tree: BTreeMap<_, _> = data
                    .iter()
                    .map(|(p, b)| {
                        (
                            *p,
                            json!({"kind":"file","mode":292,"sha256":hash(b),"bytes":b.len()}),
                        )
                    })
                    .collect();
                let receipt=serde_json::to_vec(&json!({"schema":1,"input":{"schema":1,"kind":kind,"camera":camera,"nv12":nv12,"model":model,
                    "common":{"source_namespace":namespace,"compiler_inputs":build_inputs,"compiler":{"sha256":"c".repeat(64),"bytes":80}}},
                    "origin":{"commit":source_id,"tree":tree_id,"compiler_build":build},"entry":entry,"tree":{"schema":1,"entries":tree}})).unwrap();
                file(&provider.join(format!("model-inputs/{tag}.json")), &receipt);
                receipts.insert(tag.clone(), hash(&receipt));
                entries.push(entry);
                output.push((tag, data));
            }
            let index = serde_json::to_vec(&json!({"version":1,"bundles":entries})).unwrap();
            let id = hash(&index);
            let root = provider.join(format!("{kind}-models"));
            file(
                &root.join("current.json"),
                &serde_json::to_vec(&json!({"version":1,"generation":id})).unwrap(),
            );
            file(&root.join(&id).join("index.json"), &index);
            for (tag, data) in output {
                for (name, bytes) in data {
                    file(&root.join(&id).join(&tag).join(name), &bytes);
                }
            }
        }
        file(
            &provider.join(model_inputs::CONTRACT),
            &serde_json::to_vec(
                &json!({"schema":1,"kind":"openpilot-native-model-inputs-v1","pipelines":receipts}),
            )
            .unwrap(),
        );
        Self {
            temporary,
            source,
            provider,
        }
    }
}
