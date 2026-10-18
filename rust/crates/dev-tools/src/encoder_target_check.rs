//! Frozen ARM encoder artifact comparison under an explicit AGNOS loader.
//! This host tool neither downloads dependencies nor accesses a physical device.
use crate::{
    control_tools_policy_check::json,
    path_resolve::resolve_non_strict,
    python_fs,
    qa_process::{CaptureError, CommandSpec, NativeRunner, OutputFileError, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
pub type TargetCase = (&'static str, u32, u32, u32, u32, u32, u32);
pub const CASES: [TargetCase; 5] = [
    ("lossless", 128, 80, 128, 80, 160, 8),
    ("lossless", 256, 160, 128, 80, 288, 8),
    ("h264", 256, 160, 128, 80, 288, 80),
    ("jpeg", 1928, 1208, 482, 302, 2048, 3),
    ("jpeg", 1280, 720, 320, 180, 1344, 3),
];
pub const PACKET_COUNTS: [usize; 5] = [16, 16, 68, 6, 6];
const LIBRARIES: [&str; 9] = [
    "avformat",
    "avcodec",
    "avutil",
    "swresample",
    "x264",
    "z",
    "va",
    "va-drm",
    "drm",
];
const VERSION_LINE: &str = "encoder-target b08d7969 4002660 3999588 3876708 1922";
const GROWTH: u64 = 256 * 1024 * 1024;
#[derive(Debug)]
pub struct TargetFailure(pub String);
impl fmt::Display for TargetFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for TargetFailure {}
#[derive(Debug)]
struct Classified {
    kind: &'static str,
    detail: String,
}
impl fmt::Display for Classified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}
impl std::error::Error for Classified {}
fn require(ok: bool, detail: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(TargetFailure(detail.into()).into())
    }
}
fn classified(kind: &'static str, detail: impl Into<String>) -> Box<dyn std::error::Error> {
    Box::new(Classified {
        kind,
        detail: detail.into(),
    })
}
#[derive(Clone, Debug)]
pub struct Options {
    pub rust: PathBuf,
    pub rust_sha256: OsString,
    pub jpeg_build: PathBuf,
    pub jpeg_archive: PathBuf,
    pub jpeg_sha256: OsString,
    pub ffmpeg: PathBuf,
    pub ffmpeg_receipt: PathBuf,
    pub libyuv: PathBuf,
    pub libyuv_receipt: PathBuf,
    pub compiler: PathBuf,
    pub compiler_host_libs: PathBuf,
    pub qemu: PathBuf,
    pub agnos_libs: PathBuf,
    pub output: PathBuf,
    pub pin_receipt: Vec<PathBuf>,
}
#[derive(Clone, Debug, Serialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}
pub fn identify(path: &Path) -> Result<FileRecord> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(FileRecord {
        path: path.into(),
        sha256: format!("{:x}", hash.finalize()),
        bytes: fs::metadata(path)?.len(),
    })
}
fn string(value: impl AsRef<str>) -> Result<String> {
    Ok(JsonValue::text(value.as_ref()).to_json()?)
}
fn path_json(path: &Path) -> Result<String> {
    python_fs::json_string(path.as_os_str())
}
fn os(value: impl AsRef<OsStr>) -> OsString {
    value.as_ref().to_owned()
}
fn pretty(path: &Path, encoded: &str) -> Result<()> {
    fs::write(path, json::pretty(&json::parse_text(encoded)?, 0)? + "\n")?;
    Ok(())
}
fn records_json(records: &[FileRecord]) -> Result<String> {
    Ok(format!(
        "[{}]",
        records
            .iter()
            .map(|r| Ok(format!(
                "{{\"path\":{},\"sha256\":{},\"bytes\":{}}}",
                path_json(&r.path)?,
                string(&r.sha256)?,
                r.bytes
            )))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
}
fn monotonic() -> u64 {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (t.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(t.tv_nsec as u64)
}
fn read_text(path: &Path) -> Result<String> {
    Ok(String::from_utf8(fs::read(path)?)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn get(value: &JsonValue, key: &str) -> Result<JsonValue> {
    if !matches!(value.view(), JsonView::Object(_)) {
        return Err(classified("TypeError", "receipt is not an object"));
    }
    value
        .get(key)
        .ok_or_else(|| classified("KeyError", format!("'{key}'")))
}
fn parse_json(path: &Path) -> Result<JsonValue> {
    let source = read_text(path)?;
    json::parse_text(&source).map_err(|error| classified("JSONDecodeError", error.to_string()))
}
fn utf8(value: &JsonValue) -> Result<String> {
    value
        .to_utf8()
        .ok_or_else(|| classified("TypeError", "expected text"))
}
fn list(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(v) => Ok(v),
        _ => Err(classified("TypeError", "expected list")),
    }
}
pub fn arm_elf(path: &Path) -> Result<()> {
    let mut bytes = [0; 20];
    let mut file = File::open(path)?;
    let mut read = 0;
    while read < bytes.len() {
        let n = file.read(&mut bytes[read..])?;
        if n == 0 {
            break;
        }
        read += n;
    }
    require(
        read == 20
            && bytes[..6] == *b"\x7fELF\x02\x01"
            && u16::from_le_bytes([bytes[18], bytes[19]]) == 183,
        format!("not a little-endian AArch64 ELF: {}", path.display()),
    )
}
pub fn disk_guard(directory: &Path, growth: u64) -> Result<()> {
    let free = crate::disk_space::inspect(directory, 0)?.free_bytes;
    writeln!(OpenOptions::new().create(true).append(true).open(directory.join("disk-guards.jsonl"))?,"{{\"free_bytes\": {free}, \"growth_bytes\": {growth}, \"floor_bytes\": {}, \"monotonic_ns\": {}}}",25u64*1024*1024*1024,monotonic())?;
    require(
        free >= 25u64 * 1024 * 1024 * 1024 + growth,
        "insufficient disk space; recover at least 35 GiB before resuming",
    )
}
fn argv_json(argv: &[OsString]) -> Result<String> {
    Ok(format!(
        "[{}]",
        argv.iter()
            .map(|v| python_fs::json_string(v))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))
}
/// Retain binary logs and final command status even if process creation or its
/// owned deadline fails. Parent stdin is inherited, with core files disabled.
pub fn command(
    root: &Path,
    directory: &Path,
    argv: Vec<OsString>,
    environment: BTreeMap<OsString, OsString>,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
) -> Result<Vec<u8>> {
    fs::create_dir(directory)?;
    space(directory, GROWTH)?;
    let overrides = ["LC_ALL", "DEBUG_ENCODER", "LD_LIBRARY_PATH", "LD_PRELOAD"]
        .into_iter()
        .map(|k| {
            Ok(format!(
                "{}:{}",
                string(k)?,
                environment
                    .get(OsStr::new(k))
                    .map(|v| python_fs::json_string(v))
                    .transpose()?
                    .unwrap_or_else(|| "null".into())
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let started = monotonic();
    let receipt = directory.join("command.json");
    let base=format!("\"argv\":{},\"cwd\":{},\"started_monotonic_ns\":{started},\"timeout_seconds\":300,\"environment_overrides\":{{{overrides}}}",argv_json(&argv)?,path_json(root)?);
    pretty(
        &receipt,
        &format!("{{{base},\"returncode\":null,\"status\":\"RUNNING\"}}"),
    )?;
    let executable = argv.first().cloned();
    let outcome = {
        let mut spec = CommandSpec::new_os(argv, Duration::from_secs(300));
        spec.cwd = Some(root.into());
        spec.inherit_stdin = true;
        spec.disable_core_dump = true;
        spec.stdout_limit = usize::MAX;
        spec.stderr_limit = usize::MAX;
        // subprocess.run(env=...) replaces the inherited environment. Remove
        // everything absent from the supplied map, including scrubbed QEMU_*.
        spec.env_remove = std::env::vars_os()
            .filter_map(|(k, _)| (!environment.contains_key(&k)).then_some(k))
            .collect();
        spec.set_os_env(environment);
        runner.run_to_files(
            &spec,
            &directory.join("stdout.bin"),
            &directory.join("stderr.bin"),
        )
    };
    let (status, code, detail) = match &outcome {
        Ok(v) => (
            if v.code == 0 { "PASS" } else { "FAIL" },
            Some(v.code),
            None,
        ),
        Err(e)
            if matches!(
                e.downcast_ref::<CaptureError>(),
                Some(CaptureError::Timeout { .. })
            ) =>
        {
            ("TIMEOUT", None, None)
        }
        Err(e) if e.is::<std::io::Error>() || e.is::<OutputFileError>() => {
            let (io, path) = if let Some(file) = e.downcast_ref::<OutputFileError>() {
                (&file.error, Some(file.path.as_os_str()))
            } else {
                (
                    e.downcast_ref::<std::io::Error>().unwrap(),
                    executable.as_deref(),
                )
            };
            let detail = if let (Some(errno), Some(path)) = (io.raw_os_error(), path) {
                let description = io.to_string();
                let description = description.split(" (os error").next().unwrap();
                format!(
                    "[Errno {errno}] {description}: '{}'",
                    path.to_string_lossy()
                        .replace('\\', "\\\\")
                        .replace('\'', "\\'")
                )
            } else {
                io.to_string()
            };
            ("EXEC_ERROR", None, Some(detail))
        }
        Err(_) => ("EXEC_ERROR", None, None),
    };
    let detail = detail
        .map(|v| Ok::<_, Box<dyn std::error::Error>>(format!(",\"error\":{}", string(v)?)))
        .transpose()?
        .unwrap_or_default();
    pretty(
        &receipt,
        &format!(
            "{{{base},\"returncode\":{},\"status\":{},\"ended_monotonic_ns\":{}{detail}}}",
            code.map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
            string(status)?,
            monotonic()
        ),
    )?;
    let result = outcome?;
    require(
        result.code == 0,
        format!("command failed: {}", receipt.display()),
    )?;
    Ok(fs::read(directory.join("stdout.bin"))?)
}
pub fn source_oracle(root: &Path, output: &Path) -> Result<Vec<FileRecord>> {
    let source = root.join("openpilot/system/loggerd/encoder");
    let fixture = root.join("rust/tools/encoder_oracle");
    let mut inputs = vec![fixture.join("adapter.h"), fixture.join("main.cc")];
    let mut pieces = vec![read_text(&inputs[0])?];
    for (name, marker) in [
        ("ffmpeg_encoder.h", "class FfmpegEncoder"),
        ("jpeg_encoder.h", "class JpegEncoder"),
        ("ffmpeg_encoder.cc", "FfmpegEncoder::FfmpegEncoder"),
        ("jpeg_encoder.cc", "JpegEncoder::JpegEncoder"),
    ] {
        let path = source.join(name);
        let text = read_text(&path)?;
        let index = text
            .find(marker)
            .ok_or_else(|| classified("ValueError", "substring not found"))?;
        pieces.push(text[index..].into());
        inputs.push(path);
    }
    pieces.push("#include <libyuv/version.h>\nstatic const bool version_recorded = [] {\n  fprintf(stderr, \"encoder-target %s %u %u %u %d\\n\", av_version_info(), avcodec_version(),\n          avformat_version(), avutil_version(), LIBYUV_VERSION);\n  return true;\n}();\n".into());
    pieces.push(read_text(&inputs[1])?);
    let generated = output.join("source-oracle.cc");
    fs::write(&generated, pieces.join("\n"))?;
    inputs.push(generated);
    inputs.iter().map(|p| identify(p)).collect()
}
pub fn loaded_libraries(output: &[u8], root: &Path) -> Result<Vec<FileRecord>> {
    let text = std::str::from_utf8(output)?;
    let regex = regex::Regex::new(r"(/[^\s]+)\s+\(")?;
    let paths = regex
        .captures_iter(text)
        .map(|v| resolve_non_strict(Path::new(v.get(1).unwrap().as_str())))
        .collect::<Result<BTreeSet<_>>>()?;
    require(
        !paths.is_empty(),
        "AGNOS loader did not report resolved library paths",
    )?;
    let root = resolve_non_strict(root)?;
    require(
        paths.iter().all(|p| p.starts_with(&root)),
        "target loader resolved a library outside the supplied AGNOS tree",
    )?;
    paths.iter().map(|p| identify(p)).collect()
}
pub fn verify_packages(options: &Options, records: &[FileRecord]) -> Result<()> {
    let ff = parse_json(&options.ffmpeg_receipt)?;
    let yuv = parse_json(&options.libyuv_receipt)?;
    require(
        get(&ff, "wheel_sha256")?.to_utf8().as_deref()
            == Some("ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4"),
        "FFmpeg release pin differs",
    )?;
    require(
        get(&yuv, "sha256")?.to_utf8().as_deref()
            == Some("1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475"),
        "libyuv release pin differs",
    )?;
    let ff_files = list(&get(&ff, "files")?)?
        .into_iter()
        .map(|r| Ok((utf8(&get(&r, "path")?)?, get(&r, "sha256")?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    for record in records {
        if let Ok(relative) = record.path.strip_prefix(&options.ffmpeg) {
            let key = format!(
                "ffmpeg-7.1.0.data/purelib/ffmpeg/install/{}",
                relative.display()
            );
            require(
                ff_files.get(&key).and_then(JsonValue::to_utf8).as_deref()
                    == Some(record.sha256.as_str()),
                format!(
                    "FFmpeg payload differs from pinned receipt: {}",
                    record.path.display()
                ),
            )?;
        }
        if let Ok(relative) = record.path.strip_prefix(&options.libyuv) {
            let key = format!(
                "extracted/libyuv-1922.0.data/purelib/libyuv/install/{}",
                relative.display()
            );
            require(
                get(&yuv, "files")?
                    .get(&key)
                    .and_then(|v| v.to_utf8())
                    .as_deref()
                    == Some(record.sha256.as_str()),
                format!(
                    "libyuv payload differs from pinned receipt: {}",
                    record.path.display()
                ),
            )?;
        }
    }
    Ok(())
}
fn paths(directory: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut result = vec![];
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        result.push(path.clone());
        if recursive && entry.file_type()?.is_dir() {
            result.extend(paths(&path, true)?);
        }
    }
    Ok(result)
}
fn packets(directory: &Path) -> Result<BTreeSet<OsString>> {
    Ok(paths(directory, false)?
        .into_iter()
        .filter_map(|p| {
            p.file_name()
                .filter(|v| {
                    v.as_encoded_bytes().starts_with(b"packet-")
                        && v.as_encoded_bytes().ends_with(b".bin")
                })
                .map(OsStr::to_owned)
        })
        .collect())
}
pub fn compare(case: &Path, expected_count: usize) -> Result<Vec<FileRecord>> {
    let source = case.join("source");
    let native = case.join("rust");
    let st = source.join("trace.tsv");
    let nt = native.join("trace.tsv");
    require(
        fs::read(&st)? == fs::read(&nt)?,
        format!("metadata/return values differ: {}", case.display()),
    )?;
    let expected = packets(&source)?;
    let actual = packets(&native)?;
    require(
        expected == actual && expected.len() == expected_count,
        format!(
            "missing/extra packet files: {}: {}/{}",
            case.display(),
            expected.len(),
            actual.len()
        ),
    )?;
    let mut records = vec![identify(&st)?, identify(&nt)?];
    for name in expected {
        let left = source.join(&name);
        let right = native.join(&name);
        require(
            fs::read(&left)? == fs::read(&right)?,
            format!(
                "compressed bytes differ: {}/{}",
                case.display(),
                name.to_string_lossy()
            ),
        )?;
        records.push(identify(&left)?);
        records.push(identify(&right)?);
    }
    Ok(records)
}
fn options_json(options: &Options) -> Result<String> {
    let mut fields = vec![];
    for (name, path) in [
        ("rust", &options.rust),
        ("jpeg_build", &options.jpeg_build),
        ("jpeg_archive", &options.jpeg_archive),
        ("ffmpeg", &options.ffmpeg),
        ("ffmpeg_receipt", &options.ffmpeg_receipt),
        ("libyuv", &options.libyuv),
        ("libyuv_receipt", &options.libyuv_receipt),
        ("compiler", &options.compiler),
        ("compiler_host_libs", &options.compiler_host_libs),
        ("qemu", &options.qemu),
        ("agnos_libs", &options.agnos_libs),
        ("output", &options.output),
    ] {
        fields.push(format!("{}:{}", string(name)?, path_json(path)?));
    }
    fields.push(format!(
        "\"rust_sha256\":{},\"jpeg_sha256\":{}",
        python_fs::json_string(&options.rust_sha256)?,
        python_fs::json_string(&options.jpeg_sha256)?
    ));
    fields.push(format!(
        "\"pin_receipt\":[{}]",
        options
            .pin_receipt
            .iter()
            .map(|p| path_json(p))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ));
    Ok(format!("{{{}}}", fields.join(",")))
}
fn error_kind(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if let Some(e) = error.downcast_ref::<Classified>() {
        return e.kind;
    }
    if error.is::<TargetFailure>() {
        return "TargetFailure";
    }
    if let Some(e) = error
        .downcast_ref::<std::io::Error>()
        .or_else(|| error.downcast_ref::<OutputFileError>().map(|e| &e.error))
    {
        return match e.kind() {
            std::io::ErrorKind::NotFound => "FileNotFoundError",
            std::io::ErrorKind::PermissionDenied => "PermissionError",
            std::io::ErrorKind::AlreadyExists => "FileExistsError",
            std::io::ErrorKind::IsADirectory => "IsADirectoryError",
            std::io::ErrorKind::NotADirectory => "NotADirectoryError",
            _ => "OSError",
        };
    }
    if matches!(
        error.downcast_ref::<CaptureError>(),
        Some(CaptureError::Timeout { .. })
    ) {
        return "TimeoutExpired";
    }
    if error.is::<std::str::Utf8Error>() || error.is::<std::string::FromUtf8Error>() {
        return "UnicodeDecodeError";
    }
    "TargetFailure"
}
struct Session<'a> {
    root: &'a Path,
    options: &'a Options,
    runner: &'a mut dyn Runner,
    space: &'a mut dyn FnMut(&Path, u64) -> Result<()>,
    environment: &'a BTreeMap<OsString, OsString>,
    records: &'a mut Vec<FileRecord>,
    completed: &'a mut Vec<String>,
}
impl Session<'_> {
    fn run(&mut self) -> Result<()> {
        let o = self.options;
        (self.space)(&o.output, 512 * 1024 * 1024)?;
        let mut inputs = vec![
            o.rust.clone(),
            o.jpeg_archive.clone(),
            o.compiler.clone(),
            o.qemu.clone(),
            o.ffmpeg_receipt.clone(),
            o.libyuv_receipt.clone(),
            self.root
                .join("rust/crates/dev-tools/src/encoder_target_check.rs"),
            self.root
                .join("rust/crates/dev-tools/src/bin/check_encoder_target.rs"),
        ];
        // The running ELF fingerprints all compiled checker policy, including
        // shared process, argument, path and JSON helpers. Keep source records
        // above for review alongside this executable identity.
        inputs.push(std::env::current_exe()?);
        inputs.extend(o.pin_receipt.clone());
        for path in inputs {
            self.records.push(identify(&path)?);
        }
        require(
            OsStr::new(&self.records[0].sha256) == o.rust_sha256,
            "frozen Rust ELF hash differs",
        )?;
        require(
            OsStr::new(&self.records[1].sha256) == o.jpeg_sha256,
            "frozen target JPEG archive hash differs",
        )?;
        arm_elf(&o.rust)?;
        let loader = o.agnos_libs.join("ld-linux-aarch64.so.1");
        arm_elf(&loader)?;
        self.records.push(identify(&loader)?);
        self.records.extend(source_oracle(self.root, &o.output)?);
        let mut archives = LIBRARIES
            .map(|n| o.ffmpeg.join(format!("lib/lib{n}.a")))
            .to_vec();
        archives.extend([o.libyuv.join("lib/libyuv.a"), o.jpeg_archive.clone()]);
        for path in &archives {
            self.records.push(identify(path)?);
        }
        let includes = [
            o.ffmpeg.join("include"),
            o.libyuv.join("include"),
            self.root.join("rust/crates/jpeg/native/vendor/src"),
            o.jpeg_build.clone(),
        ];
        for folder in &includes {
            require(
                folder.is_dir(),
                format!("missing explicit include directory: {}", folder.display()),
            )?;
            let mut headers = paths(folder, true)?
                .into_iter()
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|v| v.as_encoded_bytes().ends_with(b".h"))
                })
                .collect::<Vec<_>>();
            headers.sort();
            for path in headers {
                self.records.push(identify(&path)?);
            }
        }
        verify_packages(o, self.records)?;
        let mut environment = self
            .environment
            .iter()
            .filter(|(k, _)| {
                *k != OsStr::new("LD_PRELOAD")
                    && *k != OsStr::new("LD_LIBRARY_PATH")
                    && !k.as_encoded_bytes().starts_with(b"QEMU_")
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>();
        environment.insert("LC_ALL".into(), "C".into());
        environment.insert("DEBUG_ENCODER".into(), "0".into());
        let mut build = environment.clone();
        build.insert("LD_LIBRARY_PATH".into(), os(&o.compiler_host_libs));
        let oracle = o.output.join("source-oracle");
        let mut argv = vec![
            os(&o.compiler),
            "-std=c++17".into(),
            "-O1".into(),
            "-g0".into(),
            "-pthread".into(),
            os(o.output.join("source-oracle.cc")),
        ];
        for include in includes {
            let mut value = OsString::from("-I");
            value.push(include);
            argv.push(value);
        }
        argv.push("-Wl,--start-group".into());
        argv.extend(archives.iter().map(os));
        argv.extend([
            "-Wl,--end-group".into(),
            "-lm".into(),
            "-ldl".into(),
            "-o".into(),
            os(&oracle),
        ]);
        command(
            self.root,
            &o.output.join("build"),
            argv,
            build,
            self.runner,
            self.space,
        )?;
        arm_elf(&oracle)?;
        self.records.push(identify(&oracle)?);
        let runner = [
            os(&o.qemu),
            os(&loader),
            "--library-path".into(),
            os(&o.agnos_libs),
        ];
        for (label, binary) in [("source", &oracle), ("rust", &o.rust)] {
            let mut argv = runner.to_vec();
            argv.extend(["--list".into(), os(binary)]);
            let listing = command(
                self.root,
                &o.output.join(format!("loader-{label}")),
                argv,
                environment.clone(),
                self.runner,
                self.space,
            )?;
            self.records
                .extend(loaded_libraries(&listing, &o.agnos_libs)?);
        }
        for (number, (case, count)) in CASES.iter().zip(PACKET_COUNTS).enumerate() {
            let directory = o.output.join(format!("case-{number}-{}", case.0));
            fs::create_dir(&directory)?;
            for (label, binary) in [("source", &oracle), ("rust", &o.rust)] {
                let mut argv = runner.to_vec();
                argv.extend([os(binary), os(directory.join(label)), case.0.into()]);
                argv.extend(
                    [case.1, case.2, case.3, case.4, case.5, case.6].map(|v| v.to_string().into()),
                );
                command(
                    self.root,
                    &directory.join(format!("run-{label}")),
                    argv,
                    environment.clone(),
                    self.runner,
                    self.space,
                )?;
            }
            require(
                read_text(&directory.join("run-source/stderr.bin"))?
                    .split([
                        '\n', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}',
                        '\u{2029}',
                    ])
                    .any(|s| s == VERSION_LINE),
                format!(
                    "pinned codec runtime versions differ: {}",
                    directory.display()
                ),
            )?;
            self.records.extend(compare(&directory, count)?);
            self.completed.push(
                directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        require(
            PACKET_COUNTS.iter().sum::<usize>() == 112 && self.completed.len() == 5,
            "incomplete required target matrix",
        )?;
        require(
            OsStr::new(&identify(&o.rust)?.sha256) == o.rust_sha256,
            "frozen Rust ELF changed during comparison",
        )
    }
}
/// Finalize evidence on validation/process failures. Exclusive mkdir occurs
/// outside this owner, matching the original's refusal to touch an existing run.
pub fn execute(
    root: &Path,
    options: &Options,
    runner: &mut dyn Runner,
    space: &mut dyn FnMut(&Path, u64) -> Result<()>,
    environment: &BTreeMap<OsString, OsString>,
    printed: &mut dyn Write,
) -> Result<()> {
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let mut records = vec![];
    let mut completed = vec![];
    let result = Session {
        root,
        options,
        runner,
        space,
        environment,
        records: &mut records,
        completed: &mut completed,
    }
    .run();
    if let Err(error) = &result {
        pretty(
            &options.output.join("failure.json"),
            &format!(
                "{{\"type\":{},\"error\":{}}}",
                string(error_kind(error.as_ref()))?,
                string(error.to_string())?
            ),
        )?;
    }
    // Preserve per-pattern grouping, including repeated identities from earlier
    // successful comparison stages. Never overwrite the original artifacts.
    let artifacts = paths(&options.output, true)?;
    for pattern in [
        "command.json",
        "stdout.bin",
        "stderr.bin",
        "disk-guards.jsonl",
        "trace.tsv",
        "packet-*.bin",
        "*.nv12",
        "failure.json",
    ] {
        for path in &artifacts {
            let name = path.file_name().unwrap().as_encoded_bytes();
            let matches = if pattern == "packet-*.bin" {
                name.starts_with(b"packet-") && name.ends_with(b".bin")
            } else if pattern == "*.nv12" {
                name.ends_with(b".nv12")
            } else {
                name == pattern.as_bytes()
            };
            if matches {
                records.push(identify(path)?);
            }
        }
    }
    let status = if result.is_ok() { "PASS" } else { "FAIL" };
    let cases = CASES
        .iter()
        .map(|c| {
            format!(
                "[{},{},{},{},{},{},{}]",
                string(c.0).unwrap(),
                c.1,
                c.2,
                c.3,
                c.4,
                c.5,
                c.6
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    pretty(&options.output.join("receipt.json"),&format!("{{\"status\":{},\"scope\":\"exact source-method ARM codec comparison under the supplied AGNOS loader; no device/performance claim\",\"options\":{},\"completed_cases\":{},\"required_cases\":[{cases}],\"required_packet_counts\":{},\"artifacts\":{}}}",string(status)?,options_json(options)?,serde_json::to_string(&completed)?,serde_json::to_string(&PACKET_COUNTS)?,records_json(&records)?))?;
    result?;
    writeln!(
        printed,
        "{{\"status\": \"PASS\", \"cases\": {}, \"packets\": 112, \"receipt\": {}}}",
        completed.len(),
        path_json(&options.output.join("receipt.json"))?
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let names = [
        "--rust",
        "--jpeg-build",
        "--jpeg-archive",
        "--ffmpeg",
        "--ffmpeg-receipt",
        "--libyuv",
        "--libyuv-receipt",
        "--compiler",
        "--compiler-host-libs",
        "--qemu",
        "--agnos-libs",
        "--output",
    ];
    let mut specs = names.map(|n| OptionSpec::value(n, true)).to_vec();
    specs.extend([
        OptionSpec::value("--rust-sha256", true),
        OptionSpec::value("--jpeg-sha256", true),
        OptionSpec::append("--pin-receipt"),
    ]);
    let args = scan_os(&std::env::args_os().skip(1).collect::<Vec<_>>(), &specs)?;
    if args.help {
        println!("usage: check-encoder-target --rust ELF --rust-sha256 SHA256 --jpeg-build DIRECTORY --jpeg-archive ARCHIVE --jpeg-sha256 SHA256 --ffmpeg PREFIX --ffmpeg-receipt JSON --libyuv PREFIX --libyuv-receipt JSON --compiler EXECUTABLE --compiler-host-libs DIRECTORY --qemu EXECUTABLE --agnos-libs DIRECTORY --output DIRECTORY --pin-receipt JSON [--pin-receipt JSON ...]\nExact pinned ARM source/Rust encoder codec QA through the AGNOS loader");
        return Ok(());
    }
    let pins = args
        .append_values
        .get("--pin-receipt")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            crate::argparse_scan::ArgumentError(
                "the following arguments are required: --pin-receipt".into(),
            )
        })?;
    let path = |name: &str| resolve_non_strict(Path::new(&args.values[name]));
    let o = Options {
        rust: path("--rust")?,
        rust_sha256: args.values["--rust-sha256"].clone(),
        jpeg_build: path("--jpeg-build")?,
        jpeg_archive: path("--jpeg-archive")?,
        jpeg_sha256: args.values["--jpeg-sha256"].clone(),
        ffmpeg: path("--ffmpeg")?,
        ffmpeg_receipt: path("--ffmpeg-receipt")?,
        libyuv: path("--libyuv")?,
        libyuv_receipt: path("--libyuv-receipt")?,
        compiler: path("--compiler")?,
        compiler_host_libs: path("--compiler-host-libs")?,
        qemu: path("--qemu")?,
        agnos_libs: path("--agnos-libs")?,
        output: path("--output")?,
        pin_receipt: pins
            .iter()
            .map(|p| resolve_non_strict(Path::new(p)))
            .collect::<Result<_>>()?,
    };
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    execute(
        &resolve_non_strict(&root)?,
        &o,
        &mut NativeRunner,
        &mut disk_guard,
        &std::env::vars_os().collect(),
        &mut std::io::stdout(),
    )
}
