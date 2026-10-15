//! Native drivers for the unchanged model and training CXX sanitizer probes.
use crate::python_fs::json_string as json_os;
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    disk_space,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Model,
    Training,
}
fn physical(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        root.join(path)
    }
}
fn path(path: &Path) -> OsString {
    path.as_os_str().to_owned()
}
fn prefixed(prefix: &str, path: &Path) -> OsString {
    let mut value = OsString::from(prefix);
    value.push(path);
    value
}
fn report(commands: &[Vec<OsString>], capture_bytes: u64) -> Result<String> {
    let invocations = commands
        .iter()
        .map(|command| {
            let args = command
                .iter()
                .map(|arg| Ok(format!("      {}", json_os(arg)?)))
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("    [\n{}\n    ]", args.join(",\n")))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("{{\n  \"passed\": true,\n  \"invocations\": [\n{}\n  ],\n  \"observable\": \"zero exit; ASAN/UBSAN halt-on-error; nonempty real-GL capture\",\n  \"capture_bytes\": {capture_bytes}\n}}",invocations.join(",\n")))
}
fn newest(root: &Path, candidates: Vec<PathBuf>) -> Result<PathBuf> {
    let mut best: Option<(PathBuf, f64)> = None;
    for path in candidates {
        let metadata = fs::metadata(physical(root, &path))?;
        let stamp = metadata.mtime() as f64 + metadata.mtime_nsec() as f64 * 1e-9;
        if best.as_ref().is_none_or(|(_, old)| stamp > *old) {
            best = Some((path, stamp));
        }
    }
    best.map(|(p, _)| p)
        .ok_or_else(|| "no matching UI adapter build artifact".into())
}
fn entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.collect::<std::io::Result<_>>()?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error.into()),
    }
}
pub fn check(
    root: &Path,
    target: &Path,
    raylib: &Path,
    evidence: &Path,
    kind: Kind,
    runner: &mut dyn Runner,
    guard: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let evidence_path = physical(root, evidence);
    fs::create_dir_all(&evidence_path)?;
    let mut headers = vec![];
    for entry in entries(&physical(root, &target.join("build")))? {
        if entry
            .file_name()
            .as_bytes()
            .starts_with(b"openpilot-startup-ui-")
        {
            let candidate = target
                .join("build")
                .join(entry.file_name())
                .join("out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h");
            match fs::symlink_metadata(physical(root, &candidate)) {
                Ok(_) => headers.push(candidate),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    let header = newest(root, headers)?;
    let include = header
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("missing include parent")?;
    let mut libraries = vec![];
    for entry in entries(&physical(root, &target.join("deps")))? {
        let name = entry.file_name();
        let text = name.as_bytes();
        if text.starts_with(b"libcxx-") && text.ends_with(b".rlib") {
            libraries.push(target.join("deps").join(name));
        }
    }
    let cxx = newest(root, libraries)?;
    let runtime = evidence.join("libcxx_runtime.a");
    let (name, source) = match kind {
        Kind::Model => (
            "model-adapter-asan",
            "rust/tools/ui_model_qa/bridge_asan.cc",
        ),
        Kind::Training => (
            "training-adapter-asan",
            "rust/tools/ui_home_qa/bridge_asan.cc",
        ),
    };
    let program = evidence.join(name);
    let rustc = vec![
        "rustup".into(),
        "run".into(),
        "1.94.0".into(),
        "rustc".into(),
        "--edition=2024".into(),
        "--crate-type=staticlib".into(),
        path(&root.join("rust/tools/startup_ui_qa/cxx_runtime.rs")),
        "--extern".into(),
        prefixed("cxx=", &cxx),
        "-L".into(),
        prefixed("dependency=", &target.join("deps")),
        "-o".into(),
        path(&runtime),
    ];
    let mut compiler = vec![
        "clang++".into(),
        "-std=c++17".into(),
        "-g".into(),
        "-fsanitize=address,undefined".into(),
        "-fno-omit-frame-pointer".into(),
        prefixed("-I", include),
        prefixed("-I", &root.join("rust/crates/startup-ui/native")),
        prefixed("-I", &raylib.join("include")),
        path(&root.join(source)),
    ];
    for name in ["bridge.cc", "raylib_loader.cc", "graphics.cc", "egl.cc"] {
        compiler.push(path(&root.join("rust/crates/startup-ui/native").join(name)));
    }
    compiler.extend([
        path(&runtime),
        "-ldl".into(),
        "-lpthread".into(),
        "-o".into(),
        path(&program),
    ]);
    let mut probe = vec![path(&program)];
    if matches!(kind, Kind::Training) {
        probe.push(path(&root.join("openpilot/selfdrive/assets/training")));
    }
    probe.push(path(&evidence.join("capture.png")));
    let commands = vec![rustc, compiler, probe];
    for (index, argv) in commands.iter().enumerate() {
        if index < 2 {
            guard(&evidence_path)?;
        }
        let mut spec = CommandSpec::new_os(
            argv.clone(),
            Duration::from_secs(if index < 2 { 1200 } else { 300 }),
        );
        spec.cwd = Some(root.into());
        spec.inherit_stdin = true;
        spec.env = BTreeMap::from([
            (
                "ASAN_OPTIONS".into(),
                "detect_leaks=0:halt_on_error=1".into(),
            ),
            ("UBSAN_OPTIONS".into(), "halt_on_error=1".into()),
        ]);
        let result = runner.run_to_file(&spec, &evidence_path.join(format!("{index}.log")))?;
        if result.code != 0 {
            return Err(format!("UI adapter command {index} exited {}", result.code).into());
        }
    }
    let receipt = report(
        &commands,
        fs::metadata(evidence_path.join("capture.png"))?.len(),
    )?;
    fs::write(evidence_path.join("results.json"), receipt)?;
    Ok(())
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_os(
        &args,
        &[
            OptionSpec::value("--target", true),
            OptionSpec::value("--raylib", true),
            OptionSpec::value("--evidence", true),
        ],
    )?;
    if parsed.help {
        println!(
            "Usage: check-ui-adapter --target DIRECTORY --raylib DIRECTORY --evidence DIRECTORY"
        );
        return Ok(());
    }
    check(
        &std::env::current_dir()?,
        Path::new(&parsed.values["--target"]),
        Path::new(&parsed.values["--raylib"]),
        Path::new(&parsed.values["--evidence"]),
        kind,
        &mut NativeRunner,
        &mut |path| {
            disk_space::inspect(path, 512 * 1024 * 1024)?.require()?;
            Ok(())
        },
    )
}
