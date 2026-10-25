//! Independent diagnostic scene policy, ported from the pinned original helper.
//! Raylib calls cross a narrow owned raw subprocess ABI, never candidate Renderer.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    seal::Result,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const VERTEX: &str = include_str!("../tests/fixtures/ui-diagnostics/vertex.glsl");
const FRAGMENT: &str = include_str!("../tests/fixtures/ui-diagnostics/fragment.glsl");
const CHECKER: &str = "rust/tools/check_ui_diagnostics.py";
const SOURCE: &str = "rust/tools/ui_qa/diagnostic_source.py";
const CHECKER_SHA: &str = "a1aeb0a2448230d42f42c353c338737e960807a0fa19efc5577c371d8f9bff46";
const SOURCE_SHA: &str = "fe876548a62c463debde07da0e79fa584f1ccbccfc40621d6d82ef31aa2ddb9e";
// Bind the entire independent capture manifest, including its required pin set.
const MANIFEST_SHA: &str = "a2f6226fbb81d358ad9c37c6ca514ff941728f674c6603da81102b6bc2437605";
fn provenance(text: &str) -> Result<serde_json::Value> {
    if format!("{:x}", Sha256::digest(text.as_bytes())) != MANIFEST_SHA {
        return Err("diagnostic capture manifest changed; independently recapture".into());
    }
    Ok(serde_json::from_str(text)?)
}
pub fn validate_source(root: &Path) -> Result<()> {
    let pins = provenance(include_str!(
        "../tests/fixtures/ui-diagnostics/original.json"
    ))?;
    // This captured source import falls back to English because params_pyx is
    // not built. A new importable binding can select a saved multilingual font;
    // require an independent recapture rather than silently fixing it to en.
    for entry in fs::read_dir(root.join("openpilot/common"))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if name == "params_pyx"
            || name == "params_pyx.py"
            || name == "params_pyx.pyc"
            || (name.starts_with("params_pyx.") && name.ends_with(".so"))
        {
            return Err(
                "diagnostic Params import layout changed; recapture language/font selection".into(),
            );
        }
    }
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("missing diagnostic provenance")?
    {
        if (name == CHECKER && hash != CHECKER_SHA) || (name == SOURCE && hash != SOURCE_SHA) {
            return Err("diagnostic original provenance changed".into());
        }
        match fs::read(root.join(name)) {
            Ok(bytes)
                if !(name == "openpilot/system/ui/lib/application.py"
                    && hash
                        == "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00"
                    && crate::ui_application_logic_check::application_source_matches(&bytes))
                    && Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) != hash.as_str() =>
            {
                return Err(
                    format!("{name} changed; independently recapture diagnostic behavior").into(),
                )
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && (name == CHECKER || name == SOURCE) => {}
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("missing diagnostic namespace/font layout pins")?
    {
        if root
            .join(name.as_str().ok_or("invalid diagnostic absent pin")?)
            .try_exists()?
        {
            return Err("diagnostic namespace/font layout changed; independently recapture".into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock["package"]
        .as_array()
        .ok_or("missing diagnostic locked dependencies")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|n| {
                    matches!(
                        n,
                        "numpy" | "pillow" | "comma-deps-raylib" | "cffi" | "pycparser"
                    )
                })
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(packages)? != pins["dependencies"] {
        return Err("diagnostic original dependencies changed; independently recapture".into());
    }
    Ok(())
}
pub trait Api {
    fn call(&mut self, command: &str) -> Result<()>;
}
fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".into();
    }
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn path(path: &Path) -> Result<String> {
    Ok(hex(path
        .to_str()
        .ok_or("original raylib path requires strict UTF-8")?
        .as_bytes()))
}
/// All geometry, touch colors, shader source and call order belong to Rust.
pub fn render(
    root: &Path,
    mode: &OsStr,
    output: &Path,
    burn_in: bool,
    vsync: bool,
    api: &mut dyn Api,
) -> Result<()> {
    render_recording(root, mode, output, burn_in, vsync, None, api)
}
fn render_recording(
    root: &Path,
    mode: &OsStr,
    output: &Path,
    burn_in: bool,
    vsync: bool,
    recording: Option<&Recording>,
    api: &mut dyn Api,
) -> Result<()> {
    let startup = Instant::now();
    api.call(&format!("flags {}", 32 | if vsync { 64 } else { 0 }))?;
    api.call(&format!(
        "window 536 240 {}",
        hex(b"Source diagnostic modes")
    ))?;
    let target = burn_in || recording.is_some();
    if target {
        api.call("target 536 240")?;
    }
    // The original helper never feeds frames to the recorder: it exports the
    // screen directly. Preserve launch, inherited argv and EOF/cleanup anyway.
    let _recorder = recording.map(Recording::start).transpose()?;
    let atlas = root.join("openpilot/selfdrive/assets/fonts/Inter-Medium.fnt");
    let font = if atlas.exists() {
        atlas
    } else {
        root.join("openpilot/selfdrive/assets/fonts/Inter-Medium.ttf")
    };
    let size = if font.extension().is_some_and(|v| v == "fnt") {
        0
    } else {
        200
    };
    let points = (32..127)
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    api.call(&format!("font {} {size} 2 95 {points}", path(&font)?))?;
    if burn_in {
        api.call(&format!(
            "shader {} {}",
            hex(VERTEX.as_bytes()),
            hex(FRAGMENT.as_bytes())
        ))?;
    }
    // The source stops after initialization, before drawing or export.
    // CPython profiling entries have no native runtime equivalent.
    if std::env::var_os("PROFILE_STARTUP").is_some() {
        println!("\n=== Startup profile ===");
        println!("native diagnostic initialization");
        println!(
            "\x1b[92mUI window ready in {:.1} ms\x1b[0m",
            startup.elapsed().as_secs_f64() * 1000.0
        );
        return Ok(());
    }
    if target {
        api.call("target-begin")?;
    } else {
        api.call("begin")?;
    }
    api.call("clear 4278190080")?;
    for (x, blue) in [(20, 0u32), (140, 128), (260, 255)] {
        api.call(&format!(
            "rect {x} 20 110 140 {}",
            0xff000000 | (blue << 16)
        ))?;
    }
    api.call(&format!(
        "text {} 30 175 {} 4294967295",
        hex(b"Native diagnostics"),
        32.0f64 * 1.16
    ))?;
    if target {
        api.call("target-end")?;
        api.call("begin")?;
        api.call("clear 4278190080")?;
        if !burn_in {
            return Err("diagnostic source requires a Shader, not None, when RECORD is enabled without BURN_IN".into());
        }
        api.call("shader-begin")?;
        api.call("target-draw")?;
        api.call("shader-end")?;
    }
    if mode == OsStr::new("touches") {
        api.call(&format!(
            "circle 120 90 15 {}",
            u32::from_le_bytes([230, 41, 55, 255])
        ))?;
        for (index, (x, y)) in [(70, 70), (90, 80), (120, 90)].into_iter().enumerate() {
            let perc = index as f64 / 3.0;
            let red = (255.0 * (1.5 - perc)).min(255.0) as u8;
            let green = (255.0 * (perc + 0.5)).min(255.0) as u8;
            api.call(&format!(
                "circle {x} {y} 5 {}",
                u32::from_le_bytes([red, green, 50, 255])
            ))?;
        }
    }
    if mode == OsStr::new("grid") {
        let color = u32::from_le_bytes([60, 60, 60, 255]);
        for x in (0..=536).step_by(32) {
            api.call(&format!("line {x} 0 {x} 240 {color}"))?;
        }
        for y in (0..=240).step_by(32) {
            api.call(&format!("line 0 {y} 536 {y} {color}"))?;
        }
    }
    api.call(&format!("export {}", path(output)?))?;
    api.call("end")?;
    Ok(())
}
pub struct Raw {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    closed: bool,
}
impl Raw {
    pub fn open(helper: &Path, library: &Path) -> Result<Self> {
        let mut child = Command::new(helper)
            .arg(library)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().ok_or("raw diagnostic input missing")?;
        let output = BufReader::new(child.stdout.take().ok_or("raw diagnostic output missing")?);
        let mut raw = Self {
            child,
            input,
            output,
            closed: false,
        };
        raw.reply()?;
        Ok(raw)
    }
    fn reply(&mut self) -> Result<()> {
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        if line == "OK\n" {
            Ok(())
        } else if let Some(error) = line.trim_end().strip_prefix("ERR ") {
            let bytes = (0..error.len())
                .step_by(2)
                .map(|at| {
                    u8::from_str_radix(
                        error
                            .get(at..at + 2)
                            .ok_or("invalid raw diagnostic error")?,
                        16,
                    )
                    .map_err(|e| e.to_string().into())
                })
                .collect::<Result<Vec<_>>>()?;
            Err(String::from_utf8_lossy(&bytes).into_owned().into())
        } else {
            Err(format!("raw diagnostic helper ended or invalid reply: {line:?}").into())
        }
    }
    pub fn close(&mut self) -> Result<()> {
        self.call("quit")?;
        let result = self.child.wait()?;
        self.closed = true;
        if result.success() {
            Ok(())
        } else {
            Err("raw diagnostic cleanup failed".into())
        }
    }
}
impl Api for Raw {
    fn call(&mut self, command: &str) -> Result<()> {
        writeln!(self.input, "{command}")?;
        self.input.flush()?;
        self.reply()
    }
}
impl Drop for Raw {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
pub fn source_root() -> Result<PathBuf> {
    crate::path_resolve::resolve_non_strict(
        &std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")),
    )
}
pub fn reference_cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let mode = args
        .first()
        .ok_or("diagnostic reference needs MODE OUTPUT")?;
    let output = Path::new(
        args.get(1)
            .ok_or("diagnostic reference needs MODE OUTPUT")?,
    );
    let root = source_root()?;
    validate_source(&root)?;
    // These globals are parsed by the original application at import time,
    // including when recording/profiling/grid overlays are not enabled.
    for name in ["GRID", "PROFILE_RENDER", "PROFILE_STATS"] {
        environment_integer(name, "0")?;
    }
    let record_output =
        recording_output(std::env::var_os("RECORD_OUTPUT").unwrap_or_else(|| "output".into()))?;
    let quality = environment_integer("RECORD_QUALITY", "23")?;
    let speed = environment_integer("RECORD_SPEED", "1")?;
    let recording = (std::env::var_os("RECORD").as_deref() == Some(OsStr::new("1")))
        .then(|| Recording::from_environment(quality, speed, record_output))
        .transpose()?;
    let current = std::env::current_exe()?;
    let helper = std::env::var_os("UI_DIAGNOSTIC_RAW_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| current.parent().unwrap().join("ui-diagnostic-raw"));
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|v| PathBuf::from(v).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    let mut api = Raw::open(&helper, &library)?;
    let result = render_recording(
        &root,
        mode,
        output,
        mode == "burn-in" || std::env::var_os("BURN_IN").is_some(),
        std::env::var_os("ENABLE_VSYNC").as_deref() == Some(OsStr::new("1")),
        recording.as_ref(),
        &mut api,
    );
    if result.is_ok() {
        api.close()?;
    }
    result
}
pub fn build(output: &Path, raylib: &Path) -> Result<PathBuf> {
    let space = openpilot_disk_space::inspect(output, 16 * 1024 * 1024)?;
    space.require()?;
    fs::create_dir_all(output)?;
    fs::write(
        output.join("raw-raylib.cc"),
        include_bytes!("../tests/fixtures/ui-diagnostics/raw-raylib.cc"),
    )?;
    fs::write(
        output.join("raw-raylib.h"),
        include_bytes!("../tests/fixtures/ui-diagnostics/raw-raylib.h"),
    )?;
    let binary = output.join("ui-diagnostic-raw");
    let compiler = std::env::var_os("CXX").unwrap_or_else(|| "c++".into());
    let status = Command::new(compiler)
        .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-O2"])
        .arg("-I")
        .arg(raylib.join("include"))
        .arg(output.join("raw-raylib.cc"))
        .arg("-ldl")
        .arg("-o")
        .arg(&binary)
        .status()?;
    if !status.success() {
        return Err("raw diagnostic helper compilation failed".into());
    }
    Ok(binary)
}
pub fn build_cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--raylib-root", false),
        ],
    )?;
    if args.help {
        println!("usage: build-ui-diagnostic-reference --output OUTPUT [--raylib-root ROOT]");
        return Ok(());
    }
    let raylib = args
        .values
        .get("--raylib-root")
        .cloned()
        .or_else(|| std::env::var_os("STARTUP_UI_RAYLIB_ROOT"))
        .ok_or("set STARTUP_UI_RAYLIB_ROOT")?;
    println!(
        "{}",
        build(Path::new(&args.values["--output"]), Path::new(&raylib))?.display()
    );
    Ok(())
}

fn environment_integer(name: &str, default: &str) -> Result<String> {
    let value = std::env::var_os(name).unwrap_or_else(|| default.into());
    let value = value
        .to_str()
        .ok_or_else(|| format!("invalid diagnostic {name} integer encoding"))?;
    let value = value.trim();
    let (negative, body) = if let Some(body) = value.strip_prefix('-') {
        (true, body)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let mut digits = String::new();
    let mut previous = false;
    for ch in body.chars() {
        if ch == '_' && previous {
            previous = false;
            continue;
        }
        let digit = crate::argparse_scan::decimal_value(ch)
            .ok_or_else(|| format!("invalid diagnostic {name} integer"))?;
        digits.push(char::from(b'0' + digit));
        previous = true;
    }
    if !previous || digits.len() > 4300 {
        return Err(format!("invalid or overlong diagnostic {name} integer").into());
    }
    let magnitude = digits.trim_start_matches('0');
    Ok(if magnitude.is_empty() {
        "0".into()
    } else if negative {
        format!("-{magnitude}")
    } else {
        magnitude.into()
    })
}
struct Recording {
    arguments: Vec<std::ffi::OsString>,
}
fn recording_output(value: std::ffi::OsString) -> Result<PathBuf> {
    let mut output = PathBuf::from(value);
    if output.components().next_back() == Some(std::path::Component::ParentDir) {
        output.pop();
        output.push("...mp4");
    } else {
        let mut name = output
            .file_name()
            .ok_or("diagnostic RECORD_OUTPUT has an empty name")?
            .to_owned();
        // Python 3.12 pathlib treats a terminal dot as part of the stem.
        if output.extension().is_some_and(OsStr::is_empty) {
            name.push(".mp4");
            output.set_file_name(name);
        } else {
            output.set_extension("mp4");
        }
    }
    Ok(output)
}
impl Recording {
    fn from_environment(quality: String, speed: String, output: PathBuf) -> Result<Self> {
        // Python ints are unbounded; multiply the canonical decimal by 20
        // without limiting an inherited speed to a machine integer.
        let mut carry = 0u32;
        let mut digits = Vec::new();
        for ch in speed.trim_start_matches('-').bytes().rev() {
            let n = u32::from(ch - b'0') * 20 + carry;
            digits.push((b'0' + (n % 10) as u8) as char);
            carry = n / 10;
        }
        while carry != 0 {
            digits.push((b'0' + (carry % 10) as u8) as char);
            carry /= 10;
        }
        digits.reverse();
        let fps = format!(
            "{}{}",
            if speed.starts_with('-') { "-" } else { "" },
            digits.into_iter().collect::<String>()
        );
        if fps.trim_start_matches('-').len() > 4300 {
            return Err("diagnostic RECORD_SPEED result exceeds decimal conversion limit".into());
        }
        let mut args = [
            "-v",
            "warning",
            "-nostats",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            "536x240",
            "-r",
            "20",
            "-i",
            "pipe:0",
            "-vf",
            "vflip,format=yuv420p",
            "-r",
        ]
        .into_iter()
        .map(Into::into)
        .collect::<Vec<std::ffi::OsString>>();
        args.push(fps.into());
        args.extend(["-c:v", "libx264", "-preset", "veryfast", "-crf"].map(Into::into));
        args.push(quality.into());
        if let Some(bitrate) = std::env::var_os("RECORD_BITRATE").filter(|v| !v.is_empty()) {
            for option in ["-b:v", "-maxrate", "-bufsize"] {
                args.push(option.into());
                args.push(bitrate.clone());
            }
        }
        args.extend(["-y", "-f", "mp4"].map(Into::into));
        args.push(output.into_os_string());
        Ok(Self { arguments: args })
    }
    fn start(&self) -> Result<Recorder> {
        Ok(Recorder(
            Command::new("ffmpeg")
                .args(&self.arguments)
                .stdin(Stdio::piped())
                .spawn()?,
        ))
    }
}
struct Recorder(Child);
impl Drop for Recorder {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Err(_) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn original_pathlib_recording_suffix_examples() {
        for (input, expected) in [
            ("out.", "out..mp4"),
            ("x.tar.gz", "x.tar.mp4"),
            (".foo", ".foo.mp4"),
            ("..", "...mp4"),
            ("dir/foo/", "dir/foo.mp4"),
            ("dir/foo..", "dir/foo...mp4"),
        ] {
            assert_eq!(
                super::recording_output(input.into()).unwrap(),
                std::path::Path::new(expected)
            );
        }
        for input in ["", ".", "/"] {
            assert!(super::recording_output(input.into()).is_err());
        }
    }
    #[test]
    fn removing_or_changing_any_provenance_entry_cannot_silently_pass() {
        let text = include_str!("../tests/fixtures/ui-diagnostics/original.json");
        super::provenance(text).unwrap();
        let pins: serde_json::Value = serde_json::from_str(text).unwrap();
        for name in pins["sources"].as_object().unwrap().keys() {
            for remove in [true, false] {
                let mut edited = pins.clone();
                if remove {
                    edited["sources"].as_object_mut().unwrap().remove(name);
                } else {
                    edited["sources"][name] = "changed".into();
                }
                assert!(
                    super::provenance(&serde_json::to_string(&edited).unwrap()).is_err(),
                    "{name}"
                );
            }
        }
        for name in ["absent", "dependencies"] {
            let mut edited = pins.clone();
            edited.as_object_mut().unwrap().remove(name);
            assert!(super::provenance(&serde_json::to_string(&edited).unwrap()).is_err());
        }
    }
}
