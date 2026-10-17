//! Original startup bridge and Params ASan orchestration, without Python.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    disk_space,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{Captured, CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Safety,
    Resources,
}
pub struct Options {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub target: PathBuf,
    pub output: PathBuf,
    pub raylib: PathBuf,
    pub raylib_library: Option<PathBuf>,
    pub display: OsString,
}
fn physical(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        cwd.join(path)
    }
}
// pathlib.Path removes redundant separators and '.', but preserves '..' and
// the special exactly-double-leading-slash root without resolving symlinks.
fn python_path(path: &Path) -> PathBuf {
    let bytes = path.as_os_str().as_bytes();
    let mut result = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else if bytes.starts_with(b"/") {
        b"/".to_vec()
    } else {
        vec![]
    };
    for part in bytes
        .split(|b| *b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if !result.is_empty() && !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    if result.is_empty() {
        result.push(b'.');
    }
    PathBuf::from(OsString::from_vec(result))
}
fn headers(cwd: &Path, target: &Path, prefix: &[u8], suffix: &str) -> Result<Vec<PathBuf>> {
    let prefix = std::str::from_utf8(prefix)?;
    crate::python_glob::glob(cwd, target, &format!("build/{prefix}*/{suffix}"))
}
fn newest(cwd: &Path, paths: Vec<PathBuf>) -> Result<PathBuf> {
    let mut best = None;
    for path in paths {
        let m = fs::metadata(physical(cwd, &path))?;
        let time = m.mtime() as f64 + m.mtime_nsec() as f64 * 1e-9;
        if best.as_ref().is_none_or(|(_, old)| time > *old) {
            best = Some((path, time));
        }
    }
    best.map(|(p, _)| p)
        .ok_or_else(|| "missing CXX build artifact".into())
}
fn parents(path: &Path, n: usize) -> Result<PathBuf> {
    let mut path = path;
    for _ in 0..n {
        path = path.parent().ok_or("missing bridge include parent")?;
    }
    Ok(path.into())
}
fn prefixed(prefix: &str, path: &Path) -> OsString {
    let mut value = OsString::from(prefix);
    value.push(path);
    value
}
fn argv_json(args: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        args.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ))
}
struct Context<'a> {
    options: &'a Options,
    output: PathBuf,
    target: PathBuf,
    raylib: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    runner: &'a mut dyn Runner,
    free: &'a mut dyn FnMut(&Path) -> Result<u64>,
    kind: Kind,
}
impl Context<'_> {
    fn run(&mut self, name: &str, args: Vec<OsString>, runtime: bool) -> Result<()> {
        let output = physical(&self.options.cwd, &self.output);
        if matches!(self.kind, Kind::Resources) && name.contains("build") {
            let free = (self.free)(&self.output)?;
            fs::write(
                output.join(format!("{name}-disk.json")),
                format!("{{\"free_bytes\": {free}, \"estimated_growth_bytes\": 1073741824}}"),
            )?;
            if free < 26 * 1024u64.pow(3) {
                return Err("25 GiB reserve plus 1 GiB build growth required".into());
            }
        }
        let prefix = format!("Invocation: {}\n", argv_json(&args)?);
        let mut command = CommandSpec::new_os(args, Duration::from_secs(180));
        command.cwd = Some(self.options.root.clone());
        command.inherit_stdin = true;
        let mut env = self.environment.clone();
        if runtime {
            env.insert(
                "ASAN_OPTIONS".into(),
                "detect_leaks=0:halt_on_error=1".into(),
            );
            env.insert("UBSAN_OPTIONS".into(), "halt_on_error=1".into());
        }
        command.set_os_env(env);
        if self
            .runner
            .run_to_file_with_prefix(
                &command,
                &output.join(format!("{name}.log")),
                prefix.as_bytes(),
            )?
            .code
            != 0
        {
            return Err(format!("startup adapter command {name} failed").into());
        }
        Ok(())
    }
    fn adapter(&mut self) -> Result<()> {
        let root = &self.options.root;
        let header = headers(
            &self.options.cwd,
            &self.target,
            b"openpilot-startup-ui-",
            "out/cxxbridge/include/openpilot-startup-ui/src/bridge.rs.h",
        )?;
        let libraries =
            crate::python_glob::glob(&self.options.cwd, &self.target, "deps/libcxx-*.rlib")?;
        if header.is_empty() || libraries.is_empty() {
            return Err("--target must contain Cargo CXX build artifacts and deps".into());
        }
        let include = parents(&newest(&self.options.cwd, header)?, 3)?;
        let cxx = newest(&self.options.cwd, libraries)?;
        let helpers = root.join("rust/tools/startup_ui_qa");
        let runtime = self.output.join("libcxx_runtime.a");
        self.run(
            "asan-runtime-build",
            vec![
                "rustup".into(),
                "run".into(),
                "1.94.0".into(),
                "rustc".into(),
                "--edition=2024".into(),
                "--crate-type=staticlib".into(),
                helpers.join("cxx_runtime.rs").into_os_string(),
                "--extern".into(),
                prefixed("cxx=", &cxx),
                "-L".into(),
                prefixed("dependency=", &self.target.join("deps")),
                "-o".into(),
                runtime.clone().into_os_string(),
            ],
            false,
        )?;
        let program = self.output.join("bridge-asan");
        let mut compiler = vec![
            "g++".into(),
            "-std=c++17".into(),
            "-g".into(),
            "-fsanitize=address,undefined".into(),
            "-fno-omit-frame-pointer".into(),
            prefixed("-I", &include),
            prefixed("-I", &root.join("rust/crates/startup-ui/native")),
            prefixed("-I", &self.raylib.join("include")),
            helpers.join("bridge_asan.cc").into_os_string(),
        ];
        for name in [
            "bridge.cc",
            "raylib_loader.cc",
            "graphics.cc",
            "egl.cc",
            "camera.cc",
        ] {
            compiler.push(
                root.join("rust/crates/startup-ui/native")
                    .join(name)
                    .into_os_string(),
            );
        }
        compiler.extend([
            runtime.into_os_string(),
            "-ldl".into(),
            "-lpthread".into(),
            "-o".into(),
            program.clone().into_os_string(),
        ]);
        self.run("asan-build", compiler, false)?;
        self.run(
            "asan",
            vec![
                program.into_os_string(),
                root.join("openpilot/selfdrive/assets/img_spinner_track.png")
                    .into_os_string(),
                self.output.join("asan.png").into_os_string(),
            ],
            true,
        )
    }
    fn params(&mut self) -> Result<()> {
        let root = &self.options.root;
        let header = headers(
            &self.options.cwd,
            &self.target,
            b"openpilot-ui-application-",
            "out/cxxbridge/include/openpilot-ui-application/src/params/numeric/bridge.rs.h",
        )?;
        if header.is_empty() {
            return Err("UI CXX bridge must be built first".into());
        }
        let include = parents(&newest(&self.options.cwd, header)?, 5)?;
        let program = self.output.join("params-numeric-asan");
        self.run(
            "params-asan-build",
            vec![
                "g++".into(),
                "-std=c++17".into(),
                "-g".into(),
                "-fsanitize=address,undefined".into(),
                "-fno-omit-frame-pointer".into(),
                prefixed("-I", &include),
                prefixed("-I", &root.join("rust/crates/ui-application/native")),
                root.join("rust/tools/ui_application_qa/params_numeric_asan.cc")
                    .into_os_string(),
                root.join("rust/crates/ui-application/native/params_numeric.cc")
                    .into_os_string(),
                self.output.join("libcxx_runtime.a").into_os_string(),
                "-ldl".into(),
                "-lpthread".into(),
                "-o".into(),
                program.clone().into_os_string(),
            ],
            false,
        )?;
        self.run("params-asan", vec![program.into_os_string()], true)
    }
}
pub fn check(
    options: &Options,
    kind: Kind,
    runner: &mut dyn Runner,
    free: &mut dyn FnMut(&Path) -> Result<u64>,
    printed: &mut dyn Write,
) -> Result<()> {
    let (target, output, raylib, library) = match kind {
        Kind::Safety => {
            if free(&options.root)? <= 25 * 1024u64.pow(3) + 256 * 1024 * 1024 {
                return Err("startup safety disk reserve insufficient".into());
            }
            let resolve = |p: &Path| resolve_non_strict(&physical(&options.cwd, p));
            let output = resolve(&options.output)?;
            fs::create_dir_all(&output)?;
            let library = resolve(
                options
                    .raylib_library
                    .as_ref()
                    .ok_or("raylib library required")?,
            )?;
            let target = resolve(&options.target)?;
            let raylib = resolve(&options.raylib)?;
            (target, output, raylib, library)
        }
        Kind::Resources => {
            let raylib = python_path(&options.raylib);
            (
                python_path(&options.target),
                python_path(&options.output),
                raylib.clone(),
                raylib.join("lib/libopenpilot-raylib.so"),
            )
        }
    };
    if matches!(kind, Kind::Resources) {
        fs::create_dir_all(physical(&options.cwd, &output))?;
    }
    let mut environment = BTreeMap::from([
        (OsString::from("DISPLAY"), options.display.clone()),
        ("STARTUP_UI_RAYLIB_LIBRARY".into(), library.into_os_string()),
    ]);
    if matches!(kind, Kind::Safety) {
        environment.insert("OFFSCREEN".into(), "1".into());
    }
    let mut context = Context {
        options,
        target,
        output,
        raylib,
        environment,
        runner,
        free,
        kind,
    };
    context.adapter()?;
    let result=match kind{Kind::Safety=>{context.params()?;let mut hashes=vec![];for name in ["bridge-asan","params-numeric-asan"]{let bytes=fs::read(physical(&options.cwd,&context.output.join(name)))?;hashes.push(format!("    \"{name}\": \"{:x}\"",Sha256::digest(bytes)));}format!("{{\n  \"verdict\": \"PASS\",\n  \"native_bridge_cycles\": 3,\n  \"borrowed_parser_cycles\": 1000,\n  \"large_input_bytes\": 1048579,\n  \"binary_sha256\": {{\n{}\n  }},\n  \"limits\": \"external prebuilt raylib and Rust CXX runtime uninstrumented; graphics driver leak checks disabled\"\n}}",hashes.join(",\n"))},Kind::Resources=>{"{\"clear_pixels_exact\": true, \"idempotent_dynamic_texture_release\": true, \"asan\": \"PASS\", \"driver_instrumented\": false}".into()}};
    fs::write(
        physical(&options.cwd, &context.output.join("result.json")),
        result,
    )?;
    match kind {
        Kind::Safety => writeln!(printed, "PASS native bridge and Params parser ASAN/UBSAN")?,
        Kind::Resources => writeln!(
            printed,
            "PASS: native CXX ASan, actual background clear pixels, repeated/invalid texture releases and surface destruction"
        )?,
    };
    Ok(())
}
struct BuildGuard<'a> {
    runner: &'a mut dyn Runner,
    free: &'a mut dyn FnMut(&Path) -> Result<u64>,
}
impl Runner for BuildGuard<'_> {
    fn run(&mut self, command: &CommandSpec) -> Result<Captured> {
        self.runner.run(command)
    }
    fn run_to_file_with_prefix(
        &mut self,
        command: &CommandSpec,
        log: &Path,
        prefix: &[u8],
    ) -> Result<Captured> {
        if command
            .effective_argv()
            .first()
            .is_some_and(|v| v == "rustup" || v == "g++")
        {
            disk_space::Space::new(
                (self.free)(log.parent().ok_or("log parent missing")?)?,
                1024u64.pow(3),
            )?
            .require()?;
        }
        self.runner.run_to_file_with_prefix(command, log, prefix)
    }
}
/// Add the repository's per-compiler growth reserve to the original safety
/// preflight, without changing its original checks or creating a false logfile.
pub fn check_guarded(
    options: &Options,
    kind: Kind,
    runner: &mut dyn Runner,
    free: &mut dyn FnMut(&Path) -> Result<u64>,
    build_free: &mut dyn FnMut(&Path) -> Result<u64>,
    printed: &mut dyn Write,
) -> Result<()> {
    if matches!(kind, Kind::Safety) {
        check(
            options,
            kind,
            &mut BuildGuard {
                runner,
                free: build_free,
            },
            free,
            printed,
        )
    } else {
        check(options, kind, runner, free, printed)
    }
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mut specs = vec![
        OptionSpec::value("--target", true),
        OptionSpec::value("--output", true),
        OptionSpec::value("--display", true),
    ];
    specs.extend(match kind {
        Kind::Safety => vec![
            OptionSpec::value("--raylib-root", true),
            OptionSpec::value("--raylib-library", true),
        ],
        Kind::Resources => vec![OptionSpec::value("--raylib", true)],
    });
    let parsed = scan_os(&args, &specs)?;
    if parsed.help {
        println!(
            "Usage: startup adapter sanitizer --target DIRECTORY --output DIRECTORY --display DISPLAY --raylib[-root] DIRECTORY"
        );
        return Ok(());
    }
    let options = Options {
        root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()?,
        cwd: std::env::current_dir()?,
        target: PathBuf::from(&parsed.values["--target"]),
        output: PathBuf::from(&parsed.values["--output"]),
        raylib: PathBuf::from(
            &parsed.values[if matches!(kind, Kind::Safety) {
                "--raylib-root"
            } else {
                "--raylib"
            }],
        ),
        raylib_library: parsed.values.get("--raylib-library").map(PathBuf::from),
        display: parsed.values["--display"].clone(),
    };
    check_guarded(
        &options,
        kind,
        &mut NativeRunner,
        &mut |p| Ok(disk_space::inspect(p, 0)?.free_bytes),
        &mut |p| Ok(disk_space::inspect(p, 1024u64.pow(3))?.free_bytes),
        &mut std::io::stdout().lock(),
    )
}
