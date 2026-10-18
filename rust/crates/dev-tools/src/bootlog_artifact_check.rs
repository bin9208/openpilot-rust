//! Independent boot artifact comparator and source-derived uploader selection.
//! The C++ reference remains a real separately built process. Neither selection
//! nor artifact projection calls the Rust bootlog/uploader implementation.
use crate::{
    argparse_scan::{scan_os_with_positionals, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use capnp::{dynamic_struct, dynamic_value::Reader as Dynamic};
use openpilot_cereal::log_capnp::event;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{Cursor, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/check_bootlog.py",
        "a3cc60c6353d2ec374f48953b59d392e08670b6e0a1f255e58faa1924c151592",
    ),
    (
        "rust/tools/check_uploader.py",
        "b90a41b13ed1a9a99e23c771feaea3ad97ff9999c394e309e9d714f69f10a291",
    ),
    (
        "rust/tools/bootlog_output_fault.c",
        "df2e803d963ad93ded7f4044085e8a956583f08a081b493cd88eda6f9849137f",
    ),
    (
        "openpilot/system/loggerd/uploader.py",
        "8af80c2411a3303d9f3e9e2255dd0732177d9af280abab6e7f568c7576e3a02e",
    ),
    (
        "openpilot/system/loggerd/xattr_cache.py",
        "35b6460ce0d1ea99066d86d6e81ef10edcd989a2f0967f07a8bae3957720e631",
    ),
    (
        "openpilot/cereal/log.capnp",
        "fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620",
    ),
    (
        "openpilot/cereal/custom.capnp",
        "102d0aeb45effe3d15abb02a8d930834f8355f9817bf6e135a98435d9a832693",
    ),
    (
        "openpilot/cereal/deprecated.capnp",
        "65079ff035a29c4c267d07ef7240c316c7eaa81b52f795142ed7603489a61355",
    ),
    (
        "opendbc_repo/opendbc/car/car.capnp",
        "6a4d28799933e7728a0a3d4d5a4d47c64fb11e9a6d75e3d4494d2344aefebfda",
    ),
];

pub fn validate_source(root: &Path) -> Result<Value> {
    let mut pins = Map::new();
    for &(name, expected) in SOURCES {
        match fs::read(root.join(name)) {
            Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) == expected => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && name == "rust/tools/check_bootlog.py" => {}
            _ => return Err(format!("bootlog checker source changed or missing: {name}").into()),
        }
        pins.insert(name.into(), json!(expected));
    }
    Ok(Value::Object(pins))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err("odd byte encoding".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}
fn bytes_config(config: &Value, name: &str, default: &[u8]) -> Result<Vec<u8>> {
    match config.get(name) {
        None => Ok(default.to_vec()),
        Some(v) => unhex(v["hex"].as_str().ok_or("expected encoded bytes")?),
    }
}
#[derive(Serialize)]
pub struct Scenario {
    pub name: String,
    pub config: Value,
}
pub fn scenarios(fault_library: Option<&Path>) -> Vec<Scenario> {
    let mut rows = vec![
        ("ordinary".into(), json!({})),
        ("missing_pstore".into(), json!({"missing_pstore":true})),
        ("missing_launch".into(), json!({"missing_launch":true})),
        ("missing_journal".into(), json!({"missing_journal":true})),
        ("nonexec_journal".into(), json!({"journal_nonexec":true})),
        ("nonzero_journal".into(), json!({"journal_exit":7})),
    ];
    let mut journal = b"a\0b\nc\0d".to_vec();
    journal.extend(vec![b'q'; 140]);
    journal.extend(b"\nlast");
    rows.push((
        "nul_journal".into(),
        json!({"journal":{"hex":hex(&journal)}}),
    ));
    rows.extend([
        ("logs_not_directory".into(), json!({"logs_file":true})),
        (
            "counter_directory".into(),
            json!({"counter_directory":true}),
        ),
        (
            "current_directory".into(),
            json!({"current_directory":true}),
        ),
    ]);
    for count in [
        b"".as_slice(),
        b"junk",
        b" -1suffix",
        b"4294967295",
        b"18446744073709551616",
        b"  +55rest",
    ] {
        rows.push((
            format!("counter_{}", hex(count)),
            json!({"count":{"hex":hex(count)}}),
        ));
    }
    if let Some(library) = fault_library {
        let library = library
            .to_str()
            .map(|v| json!(v))
            .unwrap_or_else(|| json!({"os_hex":hex(library.as_os_str().as_bytes())}));
        for (name, fault, large) in [
            ("open_failure", "open", false),
            ("small_write_failure", "full", false),
            ("large_write_failure", "full", true),
            ("close_failure", "close", false),
        ] {
            rows.push((
                name.into(),
                json!({"output_fault":fault,"large":large,"fault_library":library}),
            ));
        }
    }
    rows.into_iter()
        .map(|(name, config)| Scenario { name, config })
        .collect()
}

#[derive(Debug)]
pub struct UploadFile {
    pub name: OsString,
    pub key: PathBuf,
    pub path: PathBuf,
}
fn priority(name: &OsStr) -> u16 {
    if name == "qlog" || name == "qlog.zst" {
        0
    } else if name == "qcamera.ts" {
        1
    } else {
        1000
    }
}
fn directory_sort(name: &OsStr) -> Vec<Vec<u32>> {
    let points = crate::python_fs::codepoints(name);
    let text = name.as_bytes();
    let split = points.windows(2).rposition(|p| p == [45, 45]);
    let parts = match split {
        Some(i) => vec![points[..i].to_vec(), points[i + 2..].to_vec()],
        None => vec![points],
    };
    let mut result = vec![vec![if text.starts_with(b"2024-") { 48 } else { 49 }]];
    result.extend(parts.into_iter().map(|mut p| {
        if p.len() < 10 {
            let mut padded = vec![48; 10 - p.len()];
            padded.append(&mut p);
            padded
        } else {
            p
        }
    }));
    result
}
fn uploaded(path: &Path) -> std::io::Result<bool> {
    // Linux xattr values are bounded to64KiB. Preserve ENODATA-only absence;
    // every other metadata/attribute error makes the original skip this entry.
    let mut buf = vec![0; 65536];
    match rustix::fs::getxattr(path, "user.upload", buf.as_mut_slice()) {
        Ok(n) => Ok(&buf[..n] == b"1"),
        Err(rustix::io::Errno::NODATA) => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub fn upload_files(
    root: &Path,
    metered: bool,
    requested: Option<&str>,
) -> Result<Vec<UploadFile>> {
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let mut directories =
        match fs::read_dir(root).and_then(|r| r.collect::<std::io::Result<Vec<_>>>()) {
            Ok(rows) => rows
                .into_iter()
                .filter(|r| r.path().is_dir())
                .collect::<Vec<_>>(),
            Err(_) => return Ok(vec![]),
        };
    directories.sort_by_cached_key(|r| directory_sort(&r.file_name()));
    let routes = requested
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for directory in directories {
        let mut entries = match fs::read_dir(directory.path())
            .and_then(|r| r.collect::<std::io::Result<Vec<_>>>())
        {
            Ok(rows) => rows,
            Err(_) => continue,
        };
        if entries
            .iter()
            .any(|r| r.file_name().as_bytes().ends_with(b".lock"))
        {
            continue;
        }
        // Stable sort retains real readdir order within each priority, as Python does.
        entries.sort_by_key(|r| priority(&r.file_name()));
        for entry in entries {
            let path = entry.path();
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if !matches!(uploaded(&path), Ok(false)) {
                continue;
            }
            if metered {
                let directory_name = directory.file_name();
                // Preserve the original literal trailing slashes (normally unmatched).
                if directory_name == "crash/" || directory_name == "boot/" {
                    use std::os::unix::fs::MetadataExt;
                    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
                    let ctime = metadata.ctime() as f64 + metadata.ctime_nsec() as f64 / 1e9;
                    if now - ctime < 12.0 * 3600.0 {
                        continue;
                    }
                }
                if entry.file_name() == "qcamera.ts"
                    && !routes.iter().any(|r| {
                        directory_name
                            .as_bytes()
                            .starts_with(r.rsplit('|').next().unwrap_or("").as_bytes())
                    })
                {
                    continue;
                }
            }
            result.push(UploadFile {
                name: entry.file_name(),
                key: Path::new(&directory.file_name()).join(entry.file_name()),
                path,
            });
        }
    }
    Ok(result)
}
pub fn next_upload(
    root: &Path,
    metered: bool,
    requested: Option<&str>,
) -> Result<Option<UploadFile>> {
    let rows = upload_files(root, metered, requested)?;
    let immediate = rows.iter().position(|r| {
        r.path
            .as_os_str()
            .as_bytes()
            .windows(6)
            .any(|w| w == b"crash/")
            || r.path
                .as_os_str()
                .as_bytes()
                .windows(5)
                .any(|w| w == b"boot/")
    });
    let index = immediate.or_else(|| rows.iter().position(|r| priority(&r.name) < 1000));
    Ok(index.and_then(|i| rows.into_iter().nth(i)))
}

fn project(value: Dynamic<'_>) -> Result<Value> {
    Ok(match value {
        Dynamic::Void => Value::Null,
        Dynamic::Bool(v) => json!(v),
        Dynamic::Int8(v) => json!(v),
        Dynamic::Int16(v) => json!(v),
        Dynamic::Int32(v) => json!(v),
        Dynamic::Int64(v) => json!(v),
        Dynamic::UInt8(v) => json!(v),
        Dynamic::UInt16(v) => json!(v),
        Dynamic::UInt32(v) => json!(v),
        Dynamic::UInt64(v) => json!(v),
        Dynamic::Float32(v) => json!(v),
        Dynamic::Float64(v) => json!(v),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::Data(v) => json!({"hex":hex(v)}),
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("Member was null.")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::List(v) => Value::Array(v.iter().map(|v| project(v?)).collect::<Result<_>>()?),
        Dynamic::Struct(v) => Value::Object(structure(v)?),
        Dynamic::AnyPointer(_) | Dynamic::Capability(_) => {
            return Err("unsupported boot artifact schema value".into())
        }
    })
}
fn structure(value: dynamic_struct::Reader<'_>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    if let Some(field) = value.which()? {
        out.insert(
            field.get_proto().get_name()?.to_str()?.into(),
            project(value.get(field)?)?,
        );
    }
    for field in value.get_schema().get_non_union_fields()? {
        if value.has(field)? {
            out.insert(
                field.get_proto().get_name()?.to_str()?.into(),
                project(value.get(field)?)?,
            );
        }
    }
    Ok(out)
}
pub fn decode_zstd(compressed: &[u8]) -> Result<(Vec<u8>, Value)> {
    let raw = zstd::stream::decode_all(Cursor::new(compressed))?;
    let events = decode_raw(&raw)?;
    Ok((raw, events))
}
fn decode_raw(raw: &[u8]) -> Result<Value> {
    let mut cursor = Cursor::new(raw);
    let mut events = Vec::new();
    while (cursor.position() as usize) < raw.len() {
        let message =
            capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::new())?;
        events.push(project(Dynamic::from(
            message.get_root::<event::Reader<'_>>()?,
        ))?);
    }
    Ok(Value::Array(events))
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .try_into()?)
}
fn enabled(config: &Value, name: &str) -> bool {
    config.get(name).and_then(Value::as_bool).unwrap_or(false)
}
fn exclusive_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("missing parent"))?,
    )?;
    fs::create_dir(path)
}
fn json_os(value: &OsStr) -> String {
    crate::python_fs::json_string(value).expect("OS strings have JSON filesystem representations")
}

/// Mechanical df/journalctl stand-ins use this already built immutable ELF.
/// Case data files are closed before any child starts; no executable is authored
/// while another process may fork. argv0 remains the original command name.
pub fn command_fixture() -> Option<Result<i32>> {
    let root = std::env::var_os("BOOTLOG_ARTIFACT_FIXTURE_ROOT")?;
    let argv0 = std::env::args_os().next()?;
    let name = Path::new(&argv0).file_name()?;
    if name != "df" && name != "journalctl" {
        return None;
    }
    Some((|| {
        if name == "df" {
            std::io::stdout().write_all(b"synthetic df\n")?;
            Ok(0)
        } else {
            std::io::stdout().write_all(&fs::read(Path::new(&root).join("journal.stdout"))?)?;
            std::io::stdout().flush()?;
            let text = fs::read_to_string(Path::new(&root).join("journal.exit"))?;
            // Execute the original shell's exit syntax rather than approximating
            // its signed range, diagnostics or signal status with Rust casts.
            // Blank lines retain the original script's line3 error location;
            // exec preserves the command PID and signal behavior without a child.
            Err(std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(format!("\n\nexit {text}\n"))
                .arg(Path::new(&root).join("commands/journalctl"))
                .exec()
                .into())
        }
    })())
}
fn write_invocation(
    path: &Path,
    argv: &[OsString],
    cwd: &Path,
    env: &[(OsString, OsString)],
    code: i32,
    interval: (u64, u64),
    binary: &Path,
) -> Result<()> {
    let (started, ended) = interval;
    let argv = argv
        .iter()
        .map(|s| json_os(s))
        .collect::<Vec<_>>()
        .join(",");
    let env = env
        .iter()
        .map(|(k, v)| format!("{}:{}", json_os(k), json_os(v)))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(path,format!("{{\"argv\":[{argv}],\"cwd\":{},\"environment\":{{{env}}},\"exit\":{code},\"start_wall_ns\":{started},\"end_wall_ns\":{ended},\"binary_sha256\":{}}}",json_os(cwd.as_os_str()),json!(digest(binary)?)))?;
    Ok(())
}

pub fn run_case(
    binary: &Path,
    output: &Path,
    config: &Value,
    native: bool,
    runner: &[OsString],
    process: &mut dyn Runner,
) -> Result<Value> {
    exclusive_directory(output)?;
    let text = output
        .to_str()
        .ok_or("UnicodeEncodeError: surrogates not allowed in output path")?;
    let prefix = format!(
        "boot-fixture-{}",
        &format!("{:x}", Sha256::digest(text.as_bytes()))[..12]
    );
    let params = output.join("params").join(&prefix);
    exclusive_directory(&params)?;
    let files = [
        ("BootCount", bytes_config(config, "count", b"42")?),
        ("DongleId", b"boot-fixture".to_vec()),
        ("GitCommit", vec![b'1'; 40]),
        ("AccessToken", b"secret-synthetic".to_vec()),
    ];
    for (name, value) in &files {
        fs::write(params.join(name), value)?;
    }
    if enabled(config, "counter_directory") {
        fs::remove_file(params.join("BootCount"))?;
        fs::create_dir(params.join("BootCount"))?;
    }
    if enabled(config, "current_directory") {
        fs::create_dir(params.join("CurrentBootlog"))?;
    }
    let copy = output.join("copy").join(&prefix);
    exclusive_directory(&copy)?;
    for (name, value) in &files {
        fs::write(
            copy.join(name),
            if *name == "DongleId" {
                b"snapshot-fixture".as_slice()
            } else {
                value.as_slice()
            },
        )?;
    }
    let pstore = output.join("pstore");
    if !enabled(config, "missing_pstore") {
        fs::create_dir(&pstore)?;
        let bytes = if enabled(config, "large") {
            let mut rng = crate::journald_check::Random::new(7);
            (0..100_000)
                .flat_map(|_| (rng.getrandbits(32) as u32).to_le_bytes())
                .collect::<Vec<_>>()
        } else {
            b"crash\0\xff\n".to_vec()
        };
        fs::write(pstore.join("z-crash"), bytes)?;
        fs::write(pstore.join("a-crash"), b"first\n")?;
        fs::create_dir(pstore.join("directory"))?;
        symlink("directory", pstore.join("linked-dir"))?;
        symlink("absent", pstore.join("broken"))?;
        symlink("a-crash", pstore.join("linked-file"))?;
    }
    let launch = output.join("launch_log");
    if !enabled(config, "missing_launch") {
        fs::write(&launch, bytes_config(config, "launch", b"launch\0log\n")?)?;
    }
    let commands = output.join("commands");
    fs::create_dir(&commands)?;
    let helper = std::env::current_exe()?;
    symlink(&helper, commands.join("df"))?;
    if !enabled(config, "missing_journal") {
        let stdout = output.join("journal.stdout");
        fs::write(
            &stdout,
            bytes_config(config, "journal", b"journal fixture\n")?,
        )?;
        let status = match config.get("journal_exit") {
            None => "0".into(),
            Some(Value::Bool(v)) => {
                if *v {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            Some(Value::Null) => "None".into(),
            Some(Value::String(v)) => v.clone(),
            Some(value) => openpilot_log_format::JsonValue::parse(&value.to_string())?.to_json()?,
        };
        fs::write(output.join("journal.exit"), status)?;
        if enabled(config, "journal_nonexec") {
            // Never executed: preserve the original -x rejection with a data file.
            fs::write(commands.join("journalctl"), b"non-executable fixture\n")?;
        } else {
            symlink(&helper, commands.join("journalctl"))?;
        }
    }
    let cwd = output.join("basedir/openpilot/system/loggerd");
    fs::create_dir_all(&cwd)?;
    fs::write(cwd.join("../../git_src_commit"), b"source-commit\n")?;
    fs::write(cwd.join("../../git_src_commit_date"), b"source-date\n")?;
    let logs = output.join("logs");
    if enabled(config, "logs_file") {
        fs::write(&logs, b"not directory")?;
    }
    let mut environment = vec![
        (
            "PARAMS_ROOT".into(),
            params.parent().ok_or("params parent")?.as_os_str().into(),
        ),
        ("OPENPILOT_PREFIX".into(), OsString::from(&prefix)),
        ("LOG_ROOT".into(), logs.as_os_str().into()),
        (
            "PARAMS_COPY_PATH".into(),
            copy.parent().ok_or("copy parent")?.as_os_str().into(),
        ),
        ("PATH".into(), commands.as_os_str().into()),
        ("BOOTLOG_PSTORE_PATH".into(), pstore.as_os_str().into()),
        ("BOOTLOG_LAUNCH_PATH".into(), launch.as_os_str().into()),
        ("CLEAN".into(), "1".into()),
    ];
    let fault = config.get("output_fault").and_then(Value::as_str);
    if let Some(fault) = fault {
        let library = if let Some(text) = config["fault_library"].as_str() {
            OsString::from(text)
        } else {
            OsString::from_vec(unhex(
                config["fault_library"]["os_hex"]
                    .as_str()
                    .ok_or("fault_library required")?,
            )?)
        };
        environment.extend([
            ("LD_PRELOAD".into(), library),
            ("BOOTLOG_OUTPUT_FAULT".into(), fault.into()),
        ]);
    }
    let mut argv = if native { runner.to_vec() } else { vec![] };
    argv.push(binary.as_os_str().into());
    if native {
        argv.extend([
            "--pstore".into(),
            pstore.as_os_str().into(),
            "--launch-log".into(),
            launch.as_os_str().into(),
        ]);
    }
    let mut spec = CommandSpec::new_os(argv.clone(), Duration::from_secs(120));
    spec.cwd = Some(cwd.clone());
    let mut child_env = environment
        .iter()
        .cloned()
        .collect::<std::collections::BTreeMap<OsString, OsString>>();
    child_env.insert(
        "BOOTLOG_ARTIFACT_FIXTURE_ROOT".into(),
        output.as_os_str().into(),
    );
    spec.set_os_env(child_env);
    spec.disable_core_dump = true;
    spec.inherit_stdin = true;
    let started = now()?;
    let captured = process.run(&spec)?;
    let ended = now()?;
    fs::write(
        output.join("stderr.log"),
        if captured.stderr.is_empty() {
            b"<empty>\n"
        } else {
            captured.stderr.as_slice()
        },
    )?;
    write_invocation(
        &output.join("invocation.json"),
        &argv,
        &cwd,
        &environment,
        captured.code,
        (started, ended),
        binary,
    )?;
    let counter = if params.join("BootCount").is_dir() {
        "<directory>".into()
    } else {
        hex(&fs::read(params.join("BootCount"))?)
    };
    let current_exists = params.join("CurrentBootlog").exists();
    let mut result =
        json!({"success":captured.code==0,"counter":counter,"current_exists":current_exists});
    if let Some(fault) = fault {
        let success = fault == "full" && !enabled(config, "large");
        if (captured.code == 0) != success || current_exists != (fault == "close" || success) {
            return Err(format!("output fault contract: {result}").into());
        }
    }
    if (captured.code == 0 && fault.is_none()) || fault == Some("close") {
        let artifacts = fs::read_dir(logs.join("boot"))?
            .filter_map(|r| r.ok().map(|r| r.path()))
            .filter(|p| p.extension().is_some_and(|s| s == "zst"))
            .collect::<Vec<_>>();
        if artifacts.len() != 1 {
            return Err(format!("expected one boot artifact, observed {}", artifacts.len()).into());
        }
        let artifact = &artifacts[0];
        let current = artifact
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or("invalid boot artifact name")?;
        if current.len() != 20
            || &current.as_bytes()[8..10] != b"--"
            || !current.as_bytes()[..8]
                .iter()
                .copied()
                .chain(current.as_bytes()[10..].iter().copied())
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(format!("invalid boot identifier: {current}").into());
        }
        if params.join("CurrentBootlog").is_file()
            && fs::read_to_string(params.join("CurrentBootlog"))? != current
        {
            return Err("CurrentBootlog does not name artifact".into());
        }
        if !artifact.is_file() {
            return Err("boot artifact is not a file".into());
        }
        let raw = zstd::stream::decode_all(Cursor::new(fs::read(artifact)?))?;
        fs::write(output.join("artifact.capnp"), &raw)?;
        let mut events = decode_raw(&raw)?;
        let rows = events.as_array().ok_or("event list")?;
        if rows.len() != 2 || rows[0].get("initData").is_none() || rows[1].get("boot").is_none() {
            return Err("expected initData then boot".into());
        }
        write_json(&output.join("decoded.json"), &events)?;
        let rows = events.as_array_mut().ok_or("event list")?;
        for row in rows.iter_mut() {
            if row["valid"] != true
                || row
                    .as_object_mut()
                    .ok_or("event object")?
                    .remove("logMonoTime")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    == 0
            {
                return Err("invalid event envelope".into());
            }
            let body = if row.get("initData").is_some() {
                &mut row["initData"]
            } else {
                &mut row["boot"]
            };
            let wall = body
                .as_object_mut()
                .ok_or("event body")?
                .remove("wallTimeNanos")
                .and_then(|v| v.as_u64())
                .ok_or("missing wall timestamp")?;
            if !(started..=ended).contains(&wall) {
                return Err("artifact wall timestamp outside process interval".into());
            }
        }
        let entries = rows[0]["initData"]["commands"]["entries"]
            .as_array_mut()
            .ok_or("initData commands entries")?;
        entries.retain(|r| !r["key"].as_str().is_some_and(|k| k.starts_with("loggerd ")));
        let selected = next_upload(&logs, false, None)?.ok_or("uploader selected no artifact")?;
        if selected.name != artifact.file_name().ok_or("artifact file name")?
            || selected.key != Path::new("boot").join(artifact.file_name().ok_or("artifact name")?)
            || selected.path != *artifact
        {
            return Err("uploader did not select boot artifact".into());
        }
        result["identifier_prefix"] = json!(&current[..8]);
        result["events"] = events;
        result["upload_key"] = json!(format!("boot/{}--RANDOM.zst", &current[..8]));
    }
    write_json(&output.join("result.json"), &result)?;
    Ok(result)
}

pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let options = scan_os_with_positionals(
        &args,
        &[
            OptionSpec::value("--fault-library", false),
            OptionSpec::value("--case", false),
            OptionSpec::append("--runner"),
        ],
        3,
    )?;
    if options.help {
        println!("Usage: check-bootlog-artifacts BINARY ORIGINAL OUTPUT [--fault-library PATH] [--case NAME] [--runner ARG]");
        return Ok(());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()?;
    let sources = validate_source(&root)?;
    let binary = PathBuf::from(&options.positionals[0]);
    let original = PathBuf::from(&options.positionals[1]);
    let output = crate::path_resolve::resolve_non_strict(Path::new(&options.positionals[2]))?;
    let fault = options
        .values
        .get("--fault-library")
        .map(|p| crate::path_resolve::resolve_non_strict(Path::new(p)))
        .transpose()?;
    let runner = options
        .append_values
        .get("--runner")
        .cloned()
        .unwrap_or_default();
    let case = options.values.get("--case");
    let mut rows = Vec::new();
    for scenario in scenarios(fault.as_deref()) {
        if case.is_some_and(|name| name != OsStr::new(&scenario.name)) {
            continue;
        }
        let expected = run_case(
            &crate::path_resolve::resolve_non_strict(&original)?,
            &output.join(&scenario.name).join("source"),
            &scenario.config,
            false,
            &[],
            &mut NativeRunner,
        )?;
        let actual = run_case(
            &crate::path_resolve::resolve_non_strict(&binary)?,
            &output.join(&scenario.name).join("native"),
            &scenario.config,
            true,
            &runner,
            &mut NativeRunner,
        )?;
        if actual != expected {
            return Err(format!(
                "bootlog mismatch {}: expected {expected}, actual {actual}",
                scenario.name
            )
            .into());
        }
        rows.push(json!({"scenario":scenario.name,"result":"PASS"}));
        println!("{} PASS", scenario.name);
    }
    write_json(
        &output.join("result.json"),
        &json!({"result":"PASS","cases":rows}),
    )?;
    let executed = !rows.is_empty();
    let mut native = Map::new();
    for name in [
        "rust/crates/dev-tools/src/bootlog_artifact_check.rs",
        "rust/crates/dev-tools/src/bin/check_bootlog_artifacts.rs",
    ] {
        native.insert(name.into(), json!(digest(&root.join(name))?));
    }
    write_json(
        &output.join("provenance.json"),
        &json!({"sources":sources,"native_sources":native,"checker_sha256":digest(&std::env::current_exe()?)?,"original_sha256":if executed {Some(digest(&original)?)} else {None},"binary_sha256":if executed {Some(digest(&binary)?)} else {None},"fault_library_sha256":if executed {fault.as_deref().map(digest).transpose()?} else {None}}),
    )?;
    Ok(())
}
