//! Live source-method codec comparison, retaining partial artifacts on failure.
use crate::{
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub output: PathBuf,
    pub binary: PathBuf,
    pub native: PathBuf,
    pub jpeg: PathBuf,
    pub library_path: Option<OsString>,
}
fn quote(value: &str) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
fn path(value: &Path) -> Result<String> {
    crate::python_fs::json_string(value.as_os_str())
}
fn object(rows: Vec<(String, String)>) -> String {
    format!(
        "{{{}}}",
        rows.into_iter()
            .map(|(k, v)| format!("{k}:{v}"))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn write_json(path: &Path, value: &str) -> Result<()> {
    let value = crate::control_tools_policy_check::json::parse_text(value)?;
    fs::write(
        path,
        crate::control_tools_policy_check::json::pretty(&value, 0)? + "\n",
    )?;
    Ok(())
}
fn read_text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn spec(options: &Options, argv: Vec<OsString>, timeout: Duration) -> CommandSpec {
    let library = options.native.join("usr/lib/x86_64-linux-gnu");
    let mut ld = library.as_os_str().to_owned();
    if let Some(value) = options.library_path.as_ref().filter(|v| !v.is_empty()) {
        ld.push(":");
        ld.push(value);
    }
    let mut command = CommandSpec::new_os(argv, timeout);
    command.set_os_env(
        [
            (
                "PKG_CONFIG_LIBDIR".into(),
                library.join("pkgconfig").into_os_string(),
            ),
            (
                "PKG_CONFIG_SYSROOT_DIR".into(),
                options.native.clone().into_os_string(),
            ),
            ("LD_LIBRARY_PATH".into(), ld),
        ]
        .into(),
    );
    command.inherit_stdin = true;
    command.stdout_limit = usize::MAX;
    command.stderr_limit = usize::MAX;
    command
}
fn run(options: &Options, runner: &mut dyn Runner, argv: Vec<OsString>, log: &str) -> Result<()> {
    let command = spec(options, argv, Duration::from_secs(120));
    let result = runner.run_to_file(&command, &options.output.join(log))?;
    if result.code != 0 {
        return Err(format!(
            "codec child exited {}: {:?}",
            result.code,
            command.effective_argv()
        )
        .into());
    }
    Ok(())
}
pub fn execute(
    options: &Options,
    runner: &mut dyn Runner,
    free: &mut dyn FnMut(&Path) -> Result<u64>,
    stdout: &mut dyn Write,
) -> Result<()> {
    let mut options = options.clone();
    options.output = crate::path_resolve::resolve_non_strict(&options.output)?;
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(&options.output)?;
    let fixture = options.root.join("rust/tools/encoder_oracle");
    let mut pieces = vec![read_text(&fixture.join("adapter.h"))?];
    let mut sources = vec![];
    for (name, marker) in [
        ("ffmpeg_encoder.h", "class FfmpegEncoder"),
        ("jpeg_encoder.h", "class JpegEncoder"),
        ("ffmpeg_encoder.cc", "FfmpegEncoder::FfmpegEncoder"),
        ("jpeg_encoder.cc", "JpegEncoder::JpegEncoder"),
    ] {
        let relative = format!("openpilot/system/loggerd/encoder/{name}");
        let source = options.root.join(&relative);
        let text = read_text(&source)?;
        let index = text.find(marker).ok_or("encoder source marker missing")?;
        pieces.push(text[index..].to_owned());
        sources.push((quote(&relative)?, quote(&hash(&source)?)?));
    }
    pieces.push(read_text(&fixture.join("main.cc"))?);
    let generated = options.output.join("source-oracle.cc");
    fs::write(&generated, pieces.join("\n"))?;
    options.native = crate::path_resolve::resolve_non_strict(&options.native)?;
    let mut flags = spec(
        &options,
        [
            "pkg-config",
            "--cflags",
            "--libs",
            "libavcodec",
            "libavformat",
            "libavutil",
        ]
        .map(OsString::from)
        .to_vec(),
        Duration::MAX,
    );
    flags.inherit_stderr = true;
    let result = runner.run(&flags)?;
    if result.code != 0 {
        return Err(format!("pkg-config exited {}", result.code).into());
    }
    let flags = crate::camera_sensor_check::split_runner(&String::from_utf8(result.stdout)?)?;
    options.jpeg = crate::path_resolve::resolve_non_strict(&options.jpeg)?;
    let library = options.native.join("usr/lib/x86_64-linux-gnu");
    let oracle = options.output.join("source-oracle");
    let mut argv: Vec<OsString> = ["g++", "-std=c++17", "-O1", "-g0"]
        .map(OsString::from)
        .to_vec();
    argv.push(generated.clone().into_os_string());
    for include in [
        options.native.join("usr/include"),
        options.root.join("rust/crates/jpeg/native/vendor/src"),
        options.jpeg.clone(),
    ] {
        let mut value = OsString::from("-I");
        value.push(include);
        argv.push(value);
    }
    argv.extend(flags.into_iter().map(OsString::from));
    argv.extend([
        library.join("libyuv.a").into_os_string(),
        options.jpeg.join("libjpeg.a").into_os_string(),
        "-o".into(),
        oracle.clone().into_os_string(),
    ]);
    let available = free(&options.output)?;
    let growth = 128 * 1024 * 1024u64;
    let floor = 0u64;
    write_json(
        &options.output.join("space.json"),
        &format!("{{\"free\":{available},\"growth\":{growth},\"floor\":{floor}}}"),
    )?;
    if available < floor + growth {
        return Err("insufficient disk headroom".into());
    }
    write_json(
        &options.output.join("build-command.json"),
        &format!(
            "[{}]",
            argv.iter()
                .map(|v| crate::python_fs::json_string(v))
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
    )?;
    run(&options, runner, argv, "build.log")?;
    let mut results = vec![];
    let mut packets = 0;
    for (index, (codec, numbers)) in [
        ("lossless", [128, 80, 128, 80, 160, 8]),
        ("lossless", [256, 160, 128, 80, 288, 8]),
        ("h264", [256, 160, 128, 80, 288, 80]),
        ("jpeg", [1928, 1208, 482, 302, 2048, 3]),
        ("jpeg", [1280, 720, 320, 180, 1344, 3]),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("case-{index}-{codec}");
        let expected = options.output.join(format!("{name}-source"));
        let actual = options.output.join(format!("{name}-rust"));
        for (binary, output, suffix) in [
            (&oracle, &expected, "source"),
            (&options.binary, &actual, "rust"),
        ] {
            let binary = if suffix == "rust" {
                crate::path_resolve::resolve_non_strict(binary)?
            } else {
                binary.clone()
            };
            let mut args = vec![
                binary.as_os_str().to_owned(),
                output.as_os_str().to_owned(),
                codec.into(),
            ];
            args.extend(numbers.map(|v| OsString::from(v.to_string())));
            run(&options, runner, args, &format!("{name}-{suffix}.log"))?;
        }
        if read_text(&expected.join("trace.tsv"))? != read_text(&actual.join("trace.tsv"))? {
            return Err(format!("{name}: packet metadata or encode returns differ").into());
        }
        let mut files = crate::python_glob::glob(&options.root, &expected, "packet-*.bin")?;
        files.sort();
        if files.is_empty()
            || files.len()
                != crate::python_glob::glob(&options.root, &actual, "packet-*.bin")?.len()
        {
            return Err(format!("{name}: missing or unexpected packets").into());
        }
        let mut hashes = vec![];
        for packet in &files {
            let file = packet.file_name().ok_or("packet filename missing")?;
            let digest = hash(packet)?;
            if digest != hash(&actual.join(file))? {
                return Err(format!("{name}: compressed bytes differ").into());
            }
            hashes.push((crate::python_fs::json_string(file)?, quote(&digest)?));
        }
        packets += files.len();
        results.push(format!(
            "{{\"case\":[{},{}],\"packets\":{},\"trace_sha256\":{},\"packet_sha256\":{}}}",
            quote(codec)?,
            numbers.map(|v| v.to_string()).join(","),
            files.len(),
            quote(&hash(&expected.join("trace.tsv"))?)?,
            object(hashes)
        ));
    }
    let mut native = vec![];
    for file in [
        library.join("libavcodec.so.60.31.102"),
        library.join("libavutil.so.58.29.100"),
        library.join("libyuv.a"),
        options.jpeg.join("libjpeg.a"),
    ] {
        native.push((path(&file)?, quote(&hash(&file)?)?));
    }
    write_json(
        &options.output.join("receipt.json"),
        &format!(
            "{{\"source_sha256\":{},\"rust_binary_sha256\":{},\"oracle_sha256\":{},\"generated_sha256\":{},\"native_sha256\":{},\"results\":[{}],\"status\":\"PASS\",\"scope\":\"host source-method codec bytes and metadata; publication transport and hardware are separate\"}}",
            object(sources),
            quote(&hash(&crate::path_resolve::resolve_non_strict(&options.binary)?)?)?,
            quote(&hash(&oracle)?)?,
            quote(&hash(&generated)?)?,
            object(native),
            results.join(",")
        ),
    )?;
    writeln!(
        stdout,
        "{{\"status\": \"PASS\", \"cases\": {}, \"packets\": {packets}}}",
        results.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = scan_os(
        &args,
        &[
            OptionSpec::value("--rust", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--jpeg-build", true),
            OptionSpec::value("--native", true),
        ],
    )?;
    if parsed.help {
        println!(
            "usage: check-encoder-codecs --rust BINARY --output DIRECTORY --jpeg-build DIRECTORY --native DIRECTORY\nCompare unchanged source FFmpeg/JPEG method bodies with Rust codec output"
        );
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let options = Options {
        root: crate::path_resolve::resolve_non_strict(&root)?,
        output: PathBuf::from(&parsed.values["--output"]),
        binary: PathBuf::from(&parsed.values["--rust"]),
        native: PathBuf::from(&parsed.values["--native"]),
        jpeg: PathBuf::from(&parsed.values["--jpeg-build"]),
        library_path: std::env::var_os("LD_LIBRARY_PATH"),
    };
    execute(
        &options,
        &mut crate::qa_process::NativeRunner,
        &mut |p| Ok(crate::disk_space::inspect(p, 0)?.free_bytes),
        &mut std::io::stdout(),
    )
}
