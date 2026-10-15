//! Build unchanged bootlog/loggerd source oracles without Python generators.
use crate::{
    disk_space, json11_stage,
    qa_process::{CaptureError, CommandSpec, Runner},
    seal::{digest, Result},
    services_header,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
const BOOTLOG: [&str; 12] = [
    "openpilot/system/loggerd/bootlog.cc",
    "openpilot/system/loggerd/logger.cc",
    "openpilot/system/loggerd/zstd_writer.cc",
    "openpilot/common/params.cc",
    "openpilot/common/util.cc",
    "openpilot/common/swaglog.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
    "rust/tools/bootlog_paths.cc",
];
const LOGGER: [&str; 12] = [
    "openpilot/system/loggerd/loggerd.cc",
    "openpilot/system/loggerd/logger.cc",
    "openpilot/system/loggerd/zstd_writer.cc",
    "openpilot/system/loggerd/video_writer.cc",
    "openpilot/common/params.cc",
    "openpilot/common/util.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
    "openpilot/common/swaglog.cc",
];
const PRODUCER: [&str; 4] = [
    "openpilot/system/loggerd/encoder/encoder.cc",
    "openpilot/system/loggerd/encoder/ffmpeg_encoder.cc",
    "openpilot/cereal/messaging/socketmaster.cc",
    "rust/tools/loggerd_native_producer.cc",
];
struct Options {
    values: BTreeMap<String, PathBuf>,
    identifier: bool,
}
impl Options {
    fn parse(family: &str, args: &[String]) -> Result<Self> {
        if !["bootlog", "loggerd"].contains(&family) {
            return Err("unknown logging source builder family".into());
        }
        let mut values = BTreeMap::new();
        let mut identifier = false;
        let mut at = 0;
        while at < args.len() {
            let key = &args[at];
            at += 1;
            if family == "bootlog" && key == "--identifier" {
                identifier = true;
                continue;
            }
            if family == "bootlog" && !key.starts_with('-') {
                if values
                    .insert("--output".into(), PathBuf::from(key))
                    .is_some()
                {
                    return Err("one positional output required".into());
                }
                continue;
            }
            let allowed = matches!(key.as_str(), "--capnp-prefix" | "--native-prefix")
                || family == "loggerd" && key == "--output"
                || family == "bootlog"
                    && matches!(key.as_str(), "--json11-prefix" | "--zmq-include");
            if !allowed {
                return Err(format!("unknown logging source builder argument: {key}").into());
            }
            let value = args.get(at).ok_or("argument requires a value")?;
            at += 1;
            values.insert(key.clone(), PathBuf::from(value));
        }
        let output = crate::path_resolve::resolve_non_strict(
            values.get("--output").ok_or("output required")?,
        )?;
        values.insert("--output".into(), output);
        if family == "bootlog" {
            for key in ["--capnp-prefix", "--native-prefix"] {
                values
                    .entry(key.into())
                    .or_insert_with(|| PathBuf::from("/usr"));
            }
        }
        Ok(Self { values, identifier })
    }
    fn path(&self, key: &str) -> Result<&Path> {
        self.values
            .get(key)
            .map(PathBuf::as_path)
            .ok_or_else(|| format!("{key} required").into())
    }
}
fn s(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).into()).collect()
}
fn include(command: &mut Vec<String>, paths: &[PathBuf]) {
    command.extend(paths.iter().map(|path| format!("-I{}", path.display())));
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}
fn source_hashes(root: &Path, names: &[&str]) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for name in names {
        result.insert((*name).into(), json!(digest(&root.join(name))?));
    }
    Ok(Value::Object(result))
}
fn files_with_extension(path: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let files = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    Ok(files
        .into_iter()
        .filter(|path| path.extension().is_some_and(|value| value == extension))
        .collect())
}
fn zmq_include(target: &Path) -> Result<PathBuf> {
    for entry in fs::read_dir(target.join("debug/build"))? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("zmq-sys-") {
            let path = entry.path().join("out/source/include");
            if path.is_dir() {
                return Ok(path);
            }
        }
    }
    Err("build msgq first or provide --zmq-include".into())
}
fn copy_tree(source: &Path, target: &Path, reuse: bool) -> Result<()> {
    if reuse {
        fs::create_dir_all(target)?;
    } else {
        fs::create_dir(target)?;
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &destination, reuse)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
fn schema(root: &Path, output: &Path, strict: bool) -> Result<(PathBuf, PathBuf)> {
    let schema = output.join("schema");
    let generated = output.join("cereal/gen/cpp");
    if strict {
        fs::create_dir(&schema)?;
        fs::create_dir_all(&generated)?;
    } else {
        fs::create_dir_all(&schema)?;
        fs::create_dir_all(&generated)?;
    }
    for name in ["log", "custom", "deprecated"] {
        fs::copy(
            root.join(format!("openpilot/cereal/{name}.capnp")),
            schema.join(format!("{name}.capnp")),
        )?;
    }
    fs::copy(
        root.join("opendbc_repo/opendbc/car/car.capnp"),
        schema.join("car.capnp"),
    )?;
    copy_tree(
        &root.join("openpilot/cereal/include"),
        &schema.join("include"),
        !strict,
    )?;
    Ok((schema, generated))
}
fn execute(mut spec: CommandSpec, log: Option<&Path>, runner: &mut dyn Runner) -> Result<()> {
    if let Some(log) = log {
        fs::write(log, [])?;
        spec.merge_output = true;
    }
    let captured = match runner.run(&spec) {
        Ok(captured) => captured,
        Err(error) => {
            if let (Some(log), Some(CaptureError::Timeout { stdout, .. })) =
                (log, error.downcast_ref::<CaptureError>())
            {
                fs::write(log, stdout)?;
            }
            return Err(error);
        }
    };
    if let Some(log) = log {
        fs::write(log, &captured.stdout)?;
    }
    if captured.code != 0 {
        return Err(format!(
            "logging source compiler exited {}: {}",
            captured.code,
            String::from_utf8_lossy(if log.is_some() {
                &captured.stdout
            } else {
                &captured.stderr
            })
        )
        .into());
    }
    Ok(())
}

fn capnp_compile(
    schema: &Path,
    generated: &Path,
    o: &Options,
    bootlog: bool,
    runner: &mut dyn Runner,
) -> Result<()> {
    let capnp = if bootlog {
        s(o.path("--capnp-prefix")?.join("bin/capnp"))
    } else {
        "capnp".into()
    };
    let mut command = vec![
        capnp,
        "compile".into(),
        format!("-I{}", schema.display()),
        format!("--src-prefix={}", schema.display()),
        format!("-oc++:{}", generated.display()),
    ];
    command.extend(files_with_extension(schema, "capnp")?.iter().map(s));
    let mut spec = CommandSpec::new(command, Duration::from_secs(300));
    if bootlog {
        let prefix = o.path("--capnp-prefix")?;
        spec.env.insert(
            "LD_LIBRARY_PATH".into(),
            s(prefix.join("lib/x86_64-linux-gnu")),
        );
        spec.env.insert(
            "PATH".into(),
            format!(
                "{}:{}",
                prefix.join("bin").display(),
                std::env::var("PATH")?
            ),
        );
    }
    execute(spec, None, runner)
}
/// Target and architecture are supplied explicitly so host discovery is testable.
/// The CLI retains the originals' distinct target-directory discovery policies.
pub fn run(
    root: &Path,
    family: &str,
    args: &[String],
    target: &Path,
    architecture: &str,
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<PathBuf> {
    let o = Options::parse(family, args)?;
    let output = o.path("--output")?;
    fs::create_dir_all(output)?;
    disk_space::Space::new(free_bytes, 512 * 1024 * 1024)?.require()?;
    let (json11, dependency) = if let Some(prefix) = o.values.get("--json11-prefix") {
        (prefix.clone(), None)
    } else {
        let staged = json11_stage::stage(root, output, architecture, runner, free_bytes)?;
        (staged.prefix, Some(staged.dependency))
    };
    if family == "bootlog" {
        bootlog(root, &o, target, &json11, dependency, runner)
    } else {
        loggerd(
            root,
            &o,
            &json11,
            dependency.ok_or("loggerd json11 metadata missing")?,
            runner,
        )
    }
}
fn bootlog(
    root: &Path,
    o: &Options,
    target: &Path,
    json11: &Path,
    dependency: Option<Value>,
    runner: &mut dyn Runner,
) -> Result<PathBuf> {
    let output = o.path("--output")?;
    if let Some(dependency) = dependency {
        write_json(&output.join("dependency.json"), &dependency)?;
    }
    let zmq = if let Some(include) = o.values.get("--zmq-include") {
        include.clone()
    } else {
        zmq_include(target)?
    };
    let (schema, generated) = schema(root, output, true)?;
    capnp_compile(&schema, &generated, o, true, runner)?;
    let mut sources = BOOTLOG.to_vec();
    if o.identifier {
        sources[0] = "rust/tools/logger_identifier_source.cc";
    }
    let binary = output.join(if o.identifier {
        "original-identifier"
    } else {
        "original-bootlog"
    });
    let native = o.path("--native-prefix")?;
    let capnp = o.path("--capnp-prefix")?;
    let mut command = strings(&["g++", "-std=c++17", "-O1", "-pthread"]);
    include(
        &mut command,
        &[
            root.to_path_buf(),
            root.join("openpilot"),
            root.join("msgq_repo"),
            output.to_path_buf(),
            generated.clone(),
            native.join("include"),
        ],
    );
    command.push(format!(
        "-L{}",
        native.join("lib/x86_64-linux-gnu").display()
    ));
    include(
        &mut command,
        &[json11.join("include"), zmq, capnp.join("include")],
    );
    command.extend([
        format!("-L{}", capnp.join("lib/x86_64-linux-gnu").display()),
        format!(
            "-Wl,--disable-new-dtags,-rpath,{}",
            capnp.join("lib/x86_64-linux-gnu").display()
        ),
    ]);
    command.extend(sources.iter().map(|name| s(root.join(name))));
    command.extend(files_with_extension(&generated, "c++")?.iter().map(s));
    command.push(s(json11.join("lib/libjson11.a")));
    command.extend(strings(&["-l:libzmq.so.5","-lzstd","-lcapnp","-lkj","-Wl,--wrap=_ZN4util9read_fileERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE","-Wl,--wrap=_ZN4util17read_files_in_dirERKNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEE","-o"]));
    command.push(s(&binary));
    write_json(&output.join("command.json"), &json!(command))?;
    execute(
        CommandSpec::new(command, Duration::from_secs(600)),
        Some(&output.join("build.log")),
        runner,
    )?;
    write_json(
        &output.join("source-hashes.json"),
        &source_hashes(root, &sources)?,
    )?;
    Ok(binary)
}
fn loggerd(
    root: &Path,
    o: &Options,
    json11: &Path,
    dependency: Value,
    runner: &mut dyn Runner,
) -> Result<PathBuf> {
    let output = o.path("--output")?;
    let zmq = zmq_include(&root.join("rust/target"))?;
    let (schema, generated) = schema(root, output, false)?;
    capnp_compile(&schema, &generated, o, false, runner)?;
    fs::write(
        output.join("cereal/services.h"),
        services_header::header(true),
    )?;
    let mut command = strings(&["g++", "-std=c++17", "-O1", "-pthread"]);
    include(
        &mut command,
        &[
            root.to_path_buf(),
            root.join("openpilot"),
            root.join("msgq_repo"),
            output.to_path_buf(),
            generated.clone(),
            json11.join("include"),
            zmq,
        ],
    );
    if let Some(capnp) = o.values.get("--capnp-prefix") {
        include(&mut command, &[capnp.join("include")]);
        command.extend([
            format!("-L{}", capnp.join("lib/x86_64-linux-gnu").display()),
            format!(
                "-Wl,--disable-new-dtags,-rpath,{}",
                capnp.join("lib/x86_64-linux-gnu").display()
            ),
        ]);
    }
    if let Some(native) = o.values.get("--native-prefix") {
        include(
            &mut command,
            &[
                native.join("include"),
                native.join("include/x86_64-linux-gnu"),
            ],
        );
        command.push(format!(
            "-L{}",
            native.join("lib/x86_64-linux-gnu").display()
        ));
    }
    let binary = output.join("original-loggerd");
    command.extend(LOGGER.iter().map(|name| s(root.join(name))));
    command.extend(files_with_extension(&generated, "c++")?.iter().map(s));
    command.push(s(json11.join("lib/libjson11.a")));
    command.extend(strings(&[
        "-l:libzmq.so.5",
        "-lavformat",
        "-lavcodec",
        "-lavutil",
        "-lzstd",
        "-lcapnp",
        "-lkj",
        "-o",
    ]));
    command.push(s(&binary));
    write_json(&output.join("build-command.json"), &json!(command))?;
    execute(
        CommandSpec::new(command.clone(), Duration::from_secs(600)),
        Some(&output.join("build.log")),
        runner,
    )?;
    let mut producer = command[..command.len() - 2]
        .iter()
        .filter(|part| {
            **part != s(root.join("openpilot/system/loggerd/loggerd.cc"))
                && !part.starts_with("-l")
                && **part != s(json11.join("lib/libjson11.a"))
        })
        .cloned()
        .collect::<Vec<_>>();
    producer.extend(PRODUCER.iter().map(|name| s(root.join(name))));
    producer.push(s(json11.join("lib/libjson11.a")));
    producer.extend(strings(&[
        "-l:libzmq.so.5",
        "-lyuv",
        "-lavformat",
        "-lavcodec",
        "-lavutil",
        "-lzstd",
        "-lcapnp",
        "-lkj",
        "-o",
    ]));
    producer.push(s(output.join("original-encoder-producer")));
    execute(
        CommandSpec::new(producer.clone(), Duration::from_secs(600)),
        Some(&output.join("producer-build.log")),
        runner,
    )?;
    write_json(
        &output.join("producer-build-command.json"),
        &json!(producer),
    )?;
    let mut sources = LOGGER.to_vec();
    sources.extend(PRODUCER);
    write_json(
        &output.join("source-hashes.json"),
        &source_hashes(root, &sources)?,
    )?;
    write_json(&output.join("dependency.json"), &dependency)?;
    write_json(
        &output.join("services-provenance.json"),
        &services_header::provenance(true),
    )?;
    Ok(binary)
}
pub fn cli(family: &str) -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        match family {"bootlog"=>println!("Usage: build-bootlog-source OUTPUT [--identifier] [--capnp-prefix PREFIX] [--json11-prefix PREFIX] [--native-prefix PREFIX] [--zmq-include INCLUDE] [--root ROOT]"),"loggerd"=>println!("Usage: build-loggerd-source --output OUTPUT [--capnp-prefix PREFIX] [--native-prefix PREFIX] [--root ROOT]"),_=>return Err("unknown logging source builder family".into())}
        return Ok(());
    }
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    if let Some(at) = args.iter().position(|arg| arg == "--root") {
        if at + 1 >= args.len() {
            return Err("--root requires value".into());
        }
        root = PathBuf::from(args.remove(at + 1));
        args.remove(at);
    }
    let root = root.canonicalize()?;
    let target = if family == "bootlog" {
        std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("rust/target"))
    } else {
        root.join("rust/target")
    };
    let o = Options::parse(family, &args)?;
    let space = disk_space::inspect(o.path("--output")?, 512 * 1024 * 1024)?;
    let binary = run(
        &root,
        family,
        &args,
        &target,
        std::env::consts::ARCH,
        &mut crate::qa_process::NativeRunner,
        space.free_bytes,
    )?;
    println!("{}", binary.display());
    Ok(())
}
