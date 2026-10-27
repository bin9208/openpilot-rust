//! Source-bound internal model catalogs. Python/compiler caches are build inputs,
//! not runtime dependencies. Original production/compiler provenance is retained.
use crate::{identity, invalid, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const CONTRACT: &str = "model-inputs/source-contract.json";
const SCOPE_PATH: &str = "rust/native-model-source-scope.json";
const LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    schema: u32,
    python_roots: Vec<String>,
    python_files: Vec<String>,
    compiler_roots: Vec<String>,
    compiler_files: Vec<String>,
    plan_files: Vec<String>,
}

fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn git(root: &Path, args: &[&str], limit: u64) -> Result<Vec<u8>> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()?;
    if !output.status.success() || output.stdout.len() as u64 > limit {
        return Err(invalid(
            "model source Git inspection failed or exceeded its bound",
        ));
    }
    Ok(output.stdout)
}
fn regular(path: &Path) -> Result<File> {
    let flags = i32::try_from((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits())
        .map_err(|_| invalid("unsupported model input open flags"))?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("model input must be a regular file"));
    }
    Ok(file)
}
fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = regular(path)?;
    if file.metadata()?.len() > limit {
        return Err(invalid("model metadata exceeds its byte bound"));
    }
    let mut data = Vec::new();
    file.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(invalid("model metadata grew beyond its byte bound"));
    }
    Ok(data)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .ok_or_else(|| invalid("missing model input string"))
}
fn hex(value: &str, length: usize) -> Result<()> {
    if !identity::hex(value, length) {
        return Err(invalid("invalid model input digest/source identity"));
    }
    Ok(())
}

pub fn source_namespace(root: &Path, compiler: bool) -> Result<String> {
    // Read the retained source's versioned scope, not the updater's current
    // scope bytes. A newer builder must still verify an older valid generation.
    let raw = git(root, &["show", &format!("HEAD:{SCOPE_PATH}")], LIMIT)?;
    let scope: Scope = serde_json::from_slice(&raw)?;
    if scope.schema != 1 {
        return Err(invalid("unsupported model source scope"));
    }
    for (values, required) in [
        (
            &scope.python_roots,
            &[
                "rust/tools/model_export",
                "tinygrad_repo/tinygrad",
                "openpilot/common",
                "openpilot/selfdrive/modeld",
                "openpilot/system/camerad/cameras",
            ][..],
        ),
        (
            &scope.python_files,
            &[
                "rust/tools/native_model_inputs.py",
                "rust/tools/native_model_cache.py",
                "rust/tools/native_model_generation.py",
                "rust/tools/native_model_export.py",
                "rust/tools/build_offline_model_pipelines.py",
                "pyproject.toml",
            ][..],
        ),
        (
            &scope.compiler_roots,
            &["rust/crates/model-runtime/src"][..],
        ),
        (
            &scope.compiler_files,
            &[
                "rust/Cargo.toml",
                "rust/Cargo.lock",
                "rust/crates/model-runtime/Cargo.toml",
            ][..],
        ),
        (
            &scope.plan_files,
            &[
                SCOPE_PATH,
                "rust/native-python-toolchain.json",
                "rust/native-model-wheels.json",
                "rust/native-model-compilers.json",
                "rust/native-rust-toolchain.json",
                "rust/native-cargo-inputs.json",
                "rust/native-system-sdk.json",
            ][..],
        ),
    ] {
        let unique: BTreeSet<_> = values.iter().collect();
        if unique.len() != values.len()
            || values.iter().any(|p| !identity::clean_path(Path::new(p)))
            || required
                .iter()
                .any(|p| !values.iter().any(|v| v.as_str() == *p))
        {
            return Err(invalid(
                "model scope omits a version-1 input or contains unsafe/duplicate paths",
            ));
        }
    }
    let mut exact: BTreeSet<PathBuf> = scope.compiler_files.iter().map(PathBuf::from).collect();
    if !compiler {
        exact.extend(
            scope
                .python_files
                .iter()
                .chain(&scope.plan_files)
                .map(PathBuf::from),
        );
        for root in &scope.python_roots {
            let mut parent = Path::new(root).parent();
            while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
                exact.insert(path.join("__init__.py"));
                parent = path.parent();
            }
        }
    }
    let raw = git(root, &["ls-tree", "-r", "-z", "HEAD"], 32 * 1024 * 1024)?;
    let mut digest = Sha256::new();
    for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let (metadata, name) = row.split_at(
            row.iter()
                .position(|b| *b == b'\t')
                .ok_or_else(|| invalid("invalid model Git entry"))?,
        );
        let name =
            std::str::from_utf8(&name[1..]).map_err(|_| invalid("non-UTF8 model source path"))?;
        let path = Path::new(name);
        let selected = exact.contains(path)
            || scope
                .compiler_roots
                .iter()
                .any(|root| path.starts_with(root))
            || (!compiler
                && scope.python_roots.iter().any(|root| {
                    path.starts_with(root)
                        && (name.ends_with(".py") || root == "tinygrad_repo/tinygrad")
                }));
        if selected {
            if !metadata.starts_with(b"100644 blob ") && !metadata.starts_with(b"100755 blob ") {
                return Err(invalid("model source input is not a Git file"));
            }
            digest.update(row);
            digest.update([0]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    schema: u32,
    kind: String,
    pipelines: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Entry {
    Directory,
    File {
        sha256: String,
        bytes: u64,
        mode: u32,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tree {
    schema: u32,
    entries: BTreeMap<PathBuf, Entry>,
}

fn file_hash(path: &Path, expected: &str, size: u64, mode: u32) -> Result<()> {
    hex(expected, 64)?;
    if size > 2 * 1024 * 1024 * 1024 || mode != 0o444 {
        return Err(invalid("model output exceeds its size/mode contract"));
    }
    let file = regular(path)?;
    if file.metadata()?.len() != size || file.metadata()?.permissions().mode() & 0o7777 != mode {
        return Err(invalid("model output differs from its size/mode contract"));
    }
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut input = file.take(size + 1);
    let mut buffer = [0; 65536];
    loop {
        let length = input.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        total += length as u64;
        digest.update(&buffer[..length]);
    }
    if total != size || format!("{:x}", digest.finalize()) != expected {
        return Err(invalid(
            "model output differs from its recorded producer bytes",
        ));
    }
    Ok(())
}
fn tree(root: &Path, expected: &Tree) -> Result<BTreeMap<PathBuf, String>> {
    if expected.schema != 1
        || expected.entries.len() > 20000
        || !fs::symlink_metadata(root)?.is_dir()
    {
        return Err(invalid("invalid model output tree"));
    }
    for required in ["graph.json", "pipeline.json", "weights.bin"] {
        if !matches!(
            expected.entries.get(Path::new(required)),
            Some(Entry::File { .. })
        ) {
            return Err(invalid(
                "model output tree omits graph, descriptor or weights",
            ));
        }
    }
    let mut actual = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    let mut total = 0_u64;
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(root.join(parent))? {
            let entry = entry?;
            let path = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| invalid("model tree escape"))?
                .to_owned();
            if !identity::clean_path(&path)
                || path.components().any(|c| c.as_os_str() == ".git")
                || !actual.insert(path.clone())
            {
                return Err(invalid("invalid model tree member"));
            }
            match expected
                .entries
                .get(&path)
                .ok_or_else(|| invalid("extra model tree member"))?
            {
                Entry::Directory if entry.file_type()?.is_dir() => pending.push(path),
                Entry::File {
                    sha256,
                    bytes,
                    mode,
                } => {
                    total = total
                        .checked_add(*bytes)
                        .ok_or_else(|| invalid("model tree size overflow"))?;
                    if total > 2 * 1024 * 1024 * 1024 {
                        return Err(invalid("model tree exceeds its output bound"));
                    }
                    file_hash(&entry.path(), sha256, *bytes, *mode)?;
                    files.insert(path, sha256.clone());
                }
                _ => return Err(invalid("model tree member is redirected")),
            }
        }
    }
    if actual != expected.entries.keys().cloned().collect() {
        return Err(invalid("model tree omits an expected output"));
    }
    Ok(files)
}
fn roster(root: &Path, expected: &BTreeSet<PathBuf>) -> Result<()> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(invalid("model catalog is redirected"));
    }
    let mut complete = expected.clone();
    for path in expected {
        let mut parent = path.parent();
        while let Some(path) = parent.filter(|p| !p.as_os_str().is_empty()) {
            complete.insert(path.to_owned());
            parent = path.parent();
        }
    }
    let mut actual = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            actual.insert(
                path.strip_prefix(root)
                    .map_err(|_| invalid("model catalog escape"))?
                    .to_owned(),
            );
            if kind.is_dir() {
                pending.push(path);
            } else if !kind.is_file() {
                return Err(invalid("model catalog contains a link or special file"));
            }
            if actual.len() + pending.len() > 100000 {
                return Err(invalid("model catalog exceeds its member bound"));
            }
        }
    }
    if actual != complete {
        return Err(invalid(
            "model catalog file roster differs from the selected generation",
        ));
    }
    Ok(())
}
fn model(root: &Path, kind: &str, input: &Value) -> Result<()> {
    let name = match kind {
        "driving" => "openpilot/selfdrive/modeld/models/driving_supercombo.onnx",
        "driver" => "openpilot/selfdrive/modeld/models/dmonitoring_model.onnx",
        _ => return Err(invalid("unsupported model kind")),
    };
    let expected = text(input, "sha256")?;
    hex(expected, 64)?;
    let size = input["bytes"]
        .as_u64()
        .ok_or_else(|| invalid("model input byte length required"))?;
    if text(input, "path")? != name || size == 0 || size > 128 * 1024 * 1024 {
        return Err(invalid(
            "model input identity/size differs from the source role",
        ));
    }
    let declared = git(root, &["cat-file", "-s", &format!("HEAD:{name}")], 64)?;
    let length: u64 = std::str::from_utf8(&declared)
        .map_err(|_| invalid("invalid Git model size"))?
        .trim()
        .parse()
        .map_err(|_| invalid("invalid Git model size"))?;
    if length > 128 * 1024 * 1024 {
        return Err(invalid("Git model blob exceeds its byte bound"));
    }
    let data = git(root, &["show", &format!("HEAD:{name}")], length)?;
    if input["lfs"] == true {
        let pointer = format!(
            "version https://git-lfs.github.com/spec/v1\noid sha256:{expected}\nsize {size}\n"
        );
        if data != pointer.as_bytes() {
            return Err(invalid("model differs from the current LFS object"));
        }
    } else if input["lfs"] != false || data.len() as u64 != size || hash(&data) != expected {
        return Err(invalid("model differs from the current ordinary Git blob"));
    }
    Ok(())
}

/// Return exactly the provider-relative files to bind/map into rust/bin.
/// Runtime callers pass root/rust/bin; no Python/compiler cache is required.
pub fn verify(source: &Path, root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let namespace = source_namespace(source, false)?;
    let compiler_namespace = source_namespace(source, true)?;
    let raw = bytes(&root.join(CONTRACT), LIMIT)?;
    let contract: Contract = serde_json::from_slice(&raw)?;
    if contract.schema != 1
        || contract.kind != "openpilot-native-model-inputs-v1"
        || contract.pipelines.len() != 4
    {
        return Err(invalid("incomplete native model contract"));
    }
    let mut files = BTreeMap::from([(PathBuf::from(CONTRACT), hash(&raw))]);
    let mut catalogs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for kind in ["driving", "driver"] {
        for camera in [[1928_u64, 1208], [1344, 760]] {
            let tag = format!("{kind}-{}x{}", camera[0], camera[1]);
            let digest = contract
                .pipelines
                .get(&tag)
                .ok_or_else(|| invalid("missing selected model pipeline"))?;
            hex(digest, 64)?;
            let receipt_path = PathBuf::from(format!("model-inputs/{tag}.json"));
            let raw = bytes(&root.join(&receipt_path), LIMIT)?;
            if hash(&raw) != *digest {
                return Err(invalid("model production receipt changed"));
            }
            let receipt: Value = serde_json::from_slice(&raw)?;
            let input = &receipt["input"];
            if receipt["schema"] != 1
                || input["schema"] != 1
                || input["kind"] != kind
                || input["camera"] != serde_json::json!(camera)
                || input["common"]["source_namespace"] != namespace
            {
                return Err(invalid(
                    "model input differs from current source/kind/camera",
                ));
            }
            hex(text(&receipt["origin"], "commit")?, 40)?;
            hex(text(&receipt["origin"], "tree")?, 40)?;
            let build = &receipt["origin"]["compiler_build"];
            if build["schema"] != 1
                || build["compiler_bytes"]
                    .as_u64()
                    .is_none_or(|b| b == 0 || b > 512 * 1024 * 1024)
                || build["features"] != serde_json::json!(["default", "native-skip-miri"])
            {
                return Err(invalid(
                    "missing native compiler version/feature/binary metadata",
                ));
            }
            for field in [
                "compiler_sha256",
                "rust_toolchain_identity",
                "rust_toolchain_tree_sha256",
                "cargo_lock_sha256",
                "cargo_sources_tree_sha256",
                "system_sdk_receipt_sha256",
            ] {
                hex(text(build, field)?, 64)?;
            }
            if build["cargo_lock_sha256"] != hash(&bytes(&source.join("rust/Cargo.lock"), LIMIT)?) {
                return Err(invalid(
                    "model compiler Cargo lock differs from retained source",
                ));
            }
            hex(text(&build["source"], "commit")?, 40)?;
            hex(text(&build["source"], "tree")?, 40)?;
            let mut build_inputs = build
                .as_object()
                .ok_or_else(|| invalid("original compiler build record required"))?
                .clone();
            build_inputs.remove("source");
            if build["compiler_sources_sha256"] != compiler_namespace
                || input["common"]["compiler_inputs"] != Value::Object(build_inputs)
                || build["compiler_sha256"] != input["common"]["compiler"]["sha256"]
                || build["compiler_bytes"] != input["common"]["compiler"]["bytes"]
            {
                return Err(invalid(
                    "model compiler provenance differs from its source/actual binary",
                ));
            }
            model(source, kind, &input["model"])?;
            let catalog = format!("{kind}-models");
            let pointer_raw = bytes(&root.join(&catalog).join("current.json"), 4096)?;
            let pointer: Value = serde_json::from_slice(&pointer_raw)?;
            let generation = text(&pointer, "generation")?;
            hex(generation, 64)?;
            if pointer["version"] != 1 {
                return Err(invalid("unsupported model pointer"));
            }
            let directory = PathBuf::from(&catalog).join(generation).join(&tag);
            let output: Tree = serde_json::from_value(receipt["tree"].clone())?;
            for (path, sha) in tree(&root.join(&directory), &output)? {
                files.insert(directory.join(path), sha);
            }
            let entry = &receipt["entry"];
            if entry["kind"] != kind
                || entry["camera"] != serde_json::json!(camera)
                || entry["directory"] != tag
            {
                return Err(invalid(
                    "model catalog entry differs from the selected pipeline",
                ));
            }
            for (name, field) in [
                ("graph.json", "graph_sha256"),
                ("pipeline.json", "pipeline_sha256"),
            ] {
                if files.get(&directory.join(name)).map(String::as_str) != Some(text(entry, field)?)
                {
                    return Err(invalid(
                        "model catalog entry differs from actual graph/descriptor",
                    ));
                }
            }
            let descriptor: Value = serde_json::from_slice(&bytes(
                &root.join(&directory).join("pipeline.json"),
                1024 * 1024,
            )?)?;
            if descriptor["kind"] != kind
                || descriptor["camera"] != input["camera"]
                || descriptor["sources"]["onnx"] != input["model"]["sha256"]
                || descriptor["nv12"] != input["nv12"]
            {
                return Err(invalid("model descriptor differs from its source input"));
            }
            files.insert(receipt_path, digest.clone());
            files.insert(
                PathBuf::from(&catalog).join("current.json"),
                hash(&pointer_raw),
            );
            catalogs.entry(catalog).or_default().push(entry.clone());
        }
    }
    for (catalog, entries) in catalogs {
        let pointer: Value =
            serde_json::from_slice(&bytes(&root.join(&catalog).join("current.json"), 4096)?)?;
        let generation = text(&pointer, "generation")?;
        let path = PathBuf::from(&catalog).join(generation).join("index.json");
        let raw = bytes(&root.join(&path), 1024 * 1024)?;
        let index: Value = serde_json::from_slice(&raw)?;
        if hash(&raw) != generation || index != serde_json::json!({"version":1,"bundles":entries}) {
            return Err(invalid(
                "model index differs from the complete selected receipt roster",
            ));
        }
        files.insert(path, generation.into());
    }
    for directory in ["driving-models", "driver-models", "model-inputs"] {
        let expected = files
            .keys()
            .filter_map(|p| p.strip_prefix(directory).ok().map(Path::to_owned))
            .collect();
        roster(&root.join(directory), &expected)?;
    }
    Ok(files)
}
