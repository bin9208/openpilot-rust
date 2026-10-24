//! Native orchestration of the current-host exporter, not an exporter/model port.
//! PKLs and libraries are only pinned here. The existing exporter remains an
//! explicit external dependency; no embedded Python wrapper is evaluated.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub const EXPORTER_SOURCES: [&str; 14] = [
    "rust/tools/build_model_pipelines.py",
    "rust/tools/model_export/__init__.py",
    "rust/tools/model_export/binding_order.py",
    "rust/tools/model_export/buffers.py",
    "rust/tools/model_export/capture.py",
    "rust/tools/model_export/compile.py",
    "rust/tools/model_export/native.py",
    "rust/tools/model_export/original.py",
    "rust/tools/model_export/pipeline.py",
    "rust/tools/model_export/publish.py",
    "rust/tools/model_export/qcom.py",
    "rust/tools/model_export/schema.py",
    "tinygrad_repo/tinygrad/runtime/ops_cpu.py",
    "tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py",
];
pub const MODELS: [&str; 5] = [
    "driving_tinygrad.pkl",
    "dmonitoring_model_tinygrad.pkl",
    "dmonitoring_model_metadata.pkl",
    "dm_warp_1344x760_tinygrad.pkl",
    "dm_warp_1928x1208_tinygrad.pkl",
];

#[derive(Clone, Debug)]
pub struct Options {
    pub root: PathBuf,
    pub fixture: PathBuf,
    pub catalog: PathBuf,
    pub receipt: PathBuf,
    pub fixture_sha256: String,
    /// The caller's interpreter spelling, replacing the wrapper's sys.executable.
    /// Do not canonicalize this: a virtualenv interpreter may be a symlink.
    pub python: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Runtime {
    pub dev: String,
    pub llvm: PathBuf,
    /// Extra admission required by this task: never enter cached-code mode.
    pub cached_code_configured: bool,
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

/// Match the wrapper's regular-leaf, before/after identity and SHA256 contract.
pub fn pin(path: &Path) -> Result<Value> {
    let before = fs::symlink_metadata(path)?;
    require(before.is_file(), "expected regular, non-symlink input")?;
    let hash = digest(path)?;
    let after = fs::symlink_metadata(path)?;
    require(
        after.is_file() && identity(&before) == identity(&after),
        "input identity changed while pinning",
    )?;
    Ok(json!({"bytes":before.len(),"sha256":hash}))
}

pub fn cpu_identity(text: &str) -> Result<Value> {
    let mut profiles = Vec::new();
    // Path.read_text uses universal newlines; /proc itself uses LF.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for section in text.split("\n\n") {
        let fields = section
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.trim(), value.trim()))
            .collect::<BTreeMap<_, _>>();
        if let (Some(model), Some(flags)) = (fields.get("model name"), fields.get("flags")) {
            let flags = flags.split_whitespace().collect::<BTreeSet<_>>();
            let profile = json!({"model_name":model,"flags":flags});
            if !profiles.contains(&profile) {
                profiles.push(profile);
            }
        }
    }
    require(!profiles.is_empty(), "current CPU identity missing")?;
    Ok(json!(profiles))
}

fn destination(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let leaf = absolute.file_name().ok_or("destination has no leaf")?;
    let parent = absolute.parent().ok_or("destination has no parent")?;
    // Admit the original parent before the shared resolver consumes any `..`:
    // a missing-prefix/../leaf must not become valid through normalization.
    // Resolve only that existing parent, leaving the leaf un-followed.
    fs::metadata(parent)?;
    Ok(resolve_non_strict(parent)?.join(leaf))
}

pub fn export_destinations(
    fixture: &Path,
    catalog: &Path,
    receipt: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let fixture = fixture.canonicalize()?;
    let catalog = destination(catalog)?;
    let receipt = destination(receipt)?;
    for path in [&catalog, &receipt] {
        match fs::symlink_metadata(path) {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(error) => return Err(error.into()),
            Ok(_) => return Err("export destination already exists or is a symlink".into()),
        }
        require(
            !path.starts_with(&fixture),
            "destination is inside historical fixture",
        )?;
    }
    require(
        !receipt.starts_with(&catalog) && !catalog.starts_with(&receipt),
        "catalog and receipt overlap",
    )?;
    Ok((catalog, receipt))
}

fn pins(root: &Path, names: &[&str]) -> Result<Value> {
    let entries = names
        .iter()
        .map(|name| Ok(((*name).to_owned(), pin(&root.join(name))?)))
        .collect::<Result<serde_json::Map<String, Value>>>()?;
    Ok(Value::Object(entries))
}

fn catalog_files(root: &Path) -> Result<Value> {
    fn visit(
        root: &Path,
        directory: &Path,
        files: &mut serde_json::Map<String, Value>,
    ) -> Result<()> {
        let mut paths = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_file() {
                let name = path
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("catalog name is not UTF-8")?;
                files.insert(name.to_owned(), pin(&path)?);
            } else if metadata.is_dir() {
                visit(root, &path, files)?;
            } else {
                return Err("catalog contains a symlink or special file".into());
            }
        }
        Ok(())
    }
    let mut files = serde_json::Map::new();
    visit(root, root, &mut files)?;
    Ok(Value::Object(files))
}

fn text_command<R: Runner>(runner: &mut R, args: Vec<OsString>) -> Result<String> {
    let mut command = CommandSpec::new_os(args, Duration::from_secs(30));
    command.inherit_stderr = true;
    let result = runner.run(&command)?;
    require(result.code == 0, "metadata command failed")?;
    Ok(String::from_utf8(result.stdout)?)
}

/// Injectable runner/CPU reads permit focused tests with inert data only.
/// On helper/post-check errors the partial catalog remains and no receipt is
/// created. A racing receipt is never overwritten (the original open('x')).
pub fn export_with<R: Runner>(
    options: &Options,
    runtime: &Runtime,
    runner: &mut R,
    cpuinfo: &mut impl FnMut() -> Result<String>,
) -> Result<Value> {
    let root = options.root.canonicalize()?;
    let fixture = options.fixture.canonicalize()?;
    let (catalog, receipt_path) =
        export_destinations(&fixture, &options.catalog, &options.receipt)?;
    require(runtime.dev == "CPU:LLVM", "DEV must be CPU:LLVM")?;
    require(
        !runtime.cached_code_configured,
        "cached-code configuration is not admitted",
    )?;
    require(
        pin(&fixture.join("fixture.json"))?["sha256"] == options.fixture_sha256,
        "historical fixture manifest pin mismatch",
    )?;
    let historical: Value = serde_json::from_slice(&fs::read(fixture.join("fixture.json"))?)?;
    let models = pins(&fixture.join("models"), &MODELS)?;
    for name in MODELS {
        require(
            models[name] == historical["files"][format!("models/{name}")],
            "historical model pin mismatch",
        )?;
    }
    let sources = pins(&root, &EXPORTER_SOURCES)?;
    let lock = pin(&root.join("uv.lock"))?;
    let llvm_path = runtime.llvm.canonicalize()?;
    let llvm = json!({
        "library_path":llvm_path.to_str().ok_or("LLVM path is not UTF-8")?,
        "library":pin(&llvm_path)?,
        "package_version":text_command(runner, ["dpkg-query", "-W", "-f=${Version}", "libllvm18"].map(OsString::from).into())?
    });
    let host_cpu = cpu_identity(&cpuinfo()?)?;
    let mut helper = CommandSpec::new_os(
        vec![
            options.python.clone().into_os_string(),
            "-P".into(),
            root.join(EXPORTER_SOURCES[0]).into_os_string(),
            "--trusted-model-directory".into(),
            fixture.join("models").into_os_string(),
            "--backend".into(),
            "cpu".into(),
            "--output".into(),
            catalog.clone().into_os_string(),
        ],
        Duration::from_secs(900),
    );
    helper.cwd = Some(root.clone());
    helper.inherit_output = true;
    require(
        runner.run(&helper)?.code == 0,
        "current-host exporter failed",
    )?;
    require(
        models == pins(&fixture.join("models"), &MODELS)?,
        "model inputs changed",
    )?;
    require(
        sources == pins(&root, &EXPORTER_SOURCES)?,
        "exporter sources changed",
    )?;
    require(lock == pin(&root.join("uv.lock"))?, "lock changed")?;
    require(llvm["library"] == pin(&llvm_path)?, "LLVM library changed")?;
    require(
        host_cpu == cpu_identity(&cpuinfo()?)?,
        "CPU identity changed",
    )?;
    let files = catalog_files(&catalog)?;
    let current: Value = serde_json::from_slice(&fs::read(catalog.join("current.json"))?)?;
    let generation = current["generation"]
        .as_str()
        .ok_or("catalog generation is not text")?;
    require(
        files[format!("{generation}/index.json")]["sha256"] == generation,
        "catalog generation/index pin mismatch",
    )?;
    let source_commit = text_command(
        runner,
        vec![
            "git".into(),
            "-C".into(),
            root.into_os_string(),
            "rev-parse".into(),
            "HEAD".into(),
        ],
    )?;
    let historical_generation = historical
        .get("catalog_generation")
        .ok_or("historical catalog generation missing")?;
    let mut receipt = json!({
        "version":1,"provider":"CPU:LLVM","source_commit":source_commit.trim(),
        "historical_fixture_manifest_sha256":options.fixture_sha256,
        "historical_catalog_generation":historical_generation,
        "models":models,"exporter_sources":sources,"lock":lock,
        "host_cpu":host_cpu,"llvm":llvm,"catalog_generation":generation,"catalog_files":files
    });
    receipt.sort_all_objects();
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt_path)?;
    serde_json::to_writer_pretty(&mut output, &receipt)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(json!({
        "catalog":catalog.to_str().ok_or("catalog path is not UTF-8")?,
        "receipt":receipt_path.to_str().ok_or("receipt path is not UTF-8")?,
        "receipt_pin":pin(&receipt_path)?,"model_compilation_repeated":false
    }))
}

pub fn parse_args(args: &[OsString]) -> Result<Option<Options>> {
    let parsed = scan_os(
        args,
        &[
            OptionSpec::value("--root", true),
            OptionSpec::value("--fixture", true),
            OptionSpec::value("--catalog", true),
            OptionSpec::value("--receipt", true),
            OptionSpec::value("--fixture-sha256", true),
            OptionSpec::value("--python", true),
        ],
    )?;
    if parsed.help {
        return Ok(None);
    }
    let value = |name: &str| -> Result<&std::ffi::OsStr> {
        Ok(parsed
            .values
            .get(name)
            .ok_or("export option missing")?
            .as_os_str())
    };
    Ok(Some(Options {
        root: value("--root")?.into(),
        fixture: value("--fixture")?.into(),
        catalog: value("--catalog")?.into(),
        receipt: value("--receipt")?.into(),
        fixture_sha256: value("--fixture-sha256")?
            .to_str()
            .ok_or("fixture hash is not UTF-8")?
            .into(),
        python: value("--python")?.into(),
    }))
}

pub fn cli() -> Result<()> {
    let Some(options) = parse_args(&std::env::args_os().skip(1).collect::<Vec<_>>())? else {
        println!("Usage: export-current-driving-catalog --root ROOT --fixture DIR --catalog DIR --receipt JSON --fixture-sha256 SHA256 --python CURRENT_INTERPRETER");
        return Ok(());
    };
    let runtime = Runtime {
        dev: std::env::var("DEV")?,
        llvm: std::env::var_os("LLVM_PATH")
            .ok_or("LLVM_PATH is required")?
            .into(),
        cached_code_configured: std::env::vars_os()
            .any(|(name, _)| name.as_bytes().starts_with(b"MODEL_CACHED")),
    };
    let summary = export_with(&options, &runtime, &mut NativeRunner, &mut || {
        Ok(fs::read_to_string("/proc/cpuinfo")?)
    })?;
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}
