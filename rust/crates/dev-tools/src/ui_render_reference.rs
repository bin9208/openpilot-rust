//! Independent shared UI scene reference. Policy never imports candidate widgets.
use crate::ui_forms_reference::ReferenceEnvironment as Environment;
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
#[path = "ui_render_layout.rs"]
mod layout;
#[path = "ui_render_scene.rs"]
mod scene_json;
struct SceneCanvas {
    raw: Raw,
    root: PathBuf,
    scale: f64,
    font: Option<Vec<u8>>,
    emoji: std::collections::BTreeMap<String, usize>,
}
impl SceneCanvas {
    fn call(&mut self, command: &str) -> Result<()> {
        layout::Api::call(self, command)
    }
}
impl layout::Api for SceneCanvas {
    fn call(&mut self, command: &str) -> Result<()> {
        if self.scale != 1.0 {
            if let Some(v) = command.strip_prefix("clip ") {
                let v = v
                    .split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                if v.len() != 4 {
                    return Err("render clip dimensions".into());
                }
                return Canvas::call(
                    &mut self.raw,
                    &format!(
                        "clip {} {} {} {}",
                        (v[0] * self.scale).trunc(),
                        (v[1] * self.scale).trunc(),
                        (v[2] * self.scale).ceil(),
                        (v[3] * self.scale).ceil()
                    ),
                );
            }
        }
        Canvas::call(&mut self.raw, command)
    }
    fn measure(&mut self, text: &str, size: f64, spacing: f64) -> Result<(f64, f64)> {
        let answer = self
            .raw
            .request(&format!("measure-spaced {} {size} {spacing}", hex(text)))?;
        let v = answer
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if v.len() != 2 {
            return Err("render invalid measure reply".into());
        }
        Ok((f64::from(v[0]), f64::from(v[1])))
    }
    fn emoji(&mut self, text: &str, x: f64, y: f64, size: f64, rgba: u32) -> Result<()> {
        let id = if let Some(&id) = self.emoji.get(text) {
            id
        } else {
            if self.font.is_none() {
                self.font = Some(fs::read(
                    self.root
                        .join("openpilot/selfdrive/assets/fonts/NotoColorEmoji.ttf"),
                )?);
            }
            let pixels =
                crate::ui_forms_reference::emoji_raster(self.font.as_ref().unwrap(), text)?;
            let mut f = tempfile::NamedTempFile::new()?;
            f.write_all(&pixels)?;
            f.flush()?;
            let id = self.emoji.len();
            layout::Api::call(
                self,
                &format!(
                    "emoji-load {id} {}",
                    hex(f.path().to_str().ok_or("render emoji path UTF8")?)
                ),
            )?;
            self.emoji.insert(text.into(), id);
            id
        };
        layout::Api::call(self, &format!("emoji {id} {x} {y} {} {rgba}", size / 128.0))
    }
}
impl Canvas for SceneCanvas {
    fn pixel_scale(&self) -> f64 {
        self.scale
    }
    fn call(&mut self, command: &str) -> Result<()> {
        layout::Api::call(self, command)
    }
    fn measure(&mut self, text: &str, size: f64) -> Result<f64> {
        Ok(layout::Api::measure(self, text, size, 0.0)?.0)
    }
    fn measure_vector(&mut self, text: &str, size: f64) -> Result<(f64, f64)> {
        layout::Api::measure(self, text, size, 0.0)
    }
    fn color_emoji(&mut self, text: &str, x: f64, y: f64, size: f64, rgba: u32) -> Result<()> {
        layout::Api::emoji(self, text, x, y, size, rgba)
    }
}

fn points(cjk: bool) -> String {
    let mut v = (32..127).collect::<Vec<_>>();
    if cjk {
        v.extend(0x3400..0x4dc0);
        v.extend(0x4e00..0xa000);
        v.extend(0xac00..0xd7a4);
    }
    v.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
fn load_fonts(api: &mut SceneCanvas, big: bool, language: &str) -> Result<()> {
    let fallback = matches!(language, "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja");
    let weights = [
        if big { "Inter-Regular" } else { "Inter-Medium" },
        "Inter-Medium",
        "Inter-Bold",
        "Inter-SemiBold",
        "KaiGenGothicKR-Bold",
        "Inter-Regular",
        "Pretendard-SemiBold",
        "unifont",
    ];
    for (id, name) in weights.iter().enumerate() {
        let folder = api.root.join("openpilot/selfdrive/assets/fonts");
        let atlas = folder.join(format!("{name}.fnt"));
        let path = if atlas.exists() {
            atlas
        } else {
            let ttf = folder.join(format!("{name}.ttf"));
            if ttf.exists() {
                ttf
            } else {
                folder.join(format!("{name}.otf"))
            }
        };
        let cjk = id == 4 || id == 7;
        let size = if id == 7 {
            -1
        } else if id == 4 {
            48
        } else {
            200
        };
        let points = points(cjk);
        let count = if cjk { 38851 } else { 95 };
        let op = if path.extension().is_some_and(|v| v == "fnt") {
            "font-atlas"
        } else if id == 0 {
            "font"
        } else {
            "font-load"
        };
        let identifier = if op == "font" {
            String::new()
        } else {
            format!("{id} ")
        };
        api.call(&format!(
            "{op} {identifier}{} {size} {count} {points}",
            hex(path.to_str().ok_or("render font path UTF8")?)
        ))?;
    }
    if fallback {
        api.call("font-fallback-id 4")?;
    }
    api.call("font-select 0")
}
pub fn validate_source(root: &Path) -> Result<()> {
    let source_matches = |name: &str, expected: &str, bytes: &[u8]| {
        if name == "openpilot/system/ui/lib/application.py"
            && expected == "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00"
        {
            crate::ui_application_logic_check::application_source_matches(bytes)
        } else {
            format!("{:x}", Sha256::digest(bytes)) == expected
        }
    };
    let manifest = include_str!("../tests/fixtures/ui-render/provenance.json");
    if format!("{:x}", Sha256::digest(manifest.as_bytes()))
        != "e2c03cc3d440ae44a81490ffd3153739e6c00accedc4ef0d4f5009738799cfe6"
    {
        return Err("render provenance manifest changed; independently recapture".into());
    }
    let pins: serde_json::Value = serde_json::from_str(manifest)?;
    for entry in fs::read_dir(root.join("openpilot/common"))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if name == "params_pyx"
            || name == "params_pyx.py"
            || name == "params_pyx.pyc"
            || (name.starts_with("params_pyx.") && name.ends_with(".so"))
        {
            return Err("render Params import layout changed; independently recapture".into());
        }
    }
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("render provenance missing sources")?
    {
        let retired = match name.as_str() {
            "rust/tools/check_ui_render.py" => {
                Some("b47f1d18f069aa4e57952b162947e4ca03f2ceff25f1158872a21df142b83501")
            }
            "rust/tools/ui_framework_source_render.py" => {
                Some("257dfe56f49f0c2ae1d007305126e69ab858d14ccf8e586cfeea30d29431be0c")
            }
            _ => None,
        };
        if retired.is_some_and(|expected| hash.as_str() != Some(expected)) {
            return Err("render original capture hash changed".into());
        }
        match fs::read(root.join(name)) {
            Ok(bytes)
                if !hash
                    .as_str()
                    .is_some_and(|expected| source_matches(name, expected, &bytes)) =>
            {
                return Err(format!("{name} changed; independently recapture render").into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired.is_some() => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("render provenance missing absence")?
    {
        if root
            .join(name.as_str().ok_or("render absence path")?)
            .try_exists()?
        {
            return Err("render font namespace changed; independently recapture".into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock["package"]
        .as_array()
        .ok_or("render dependency lock missing")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| {
                    matches!(
                        name,
                        "numpy" | "pillow" | "comma-deps-raylib" | "cffi" | "pycparser"
                    )
                })
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(packages)? != pins["dependencies"] {
        return Err("render dependency lock changed; independently recapture".into());
    }
    Ok(())
}
pub fn states_equal(
    left: &openpilot_log_format::JsonValue,
    right: &openpilot_log_format::JsonValue,
) -> bool {
    crate::control_tools_policy_check::json::equal(left, right)
}
pub fn render_lossless(
    root: &Path,
    scene: &[u8],
    output: &Path,
    helper: &Path,
    library: &Path,
) -> Result<Option<openpilot_log_format::JsonValue>> {
    let document = scene_json::Document::parse(scene)?;
    let scene = document.bootstrap();
    let config = scene.get("config").ok_or("render config missing")?;
    let big = layout::truth(config.get("big").ok_or("render big missing")?);
    let _viewport = config
        .get("large_viewport")
        .ok_or("render large_viewport missing")?;
    let scale = config
        .get("scale")
        .and_then(serde_json::Value::as_f64)
        .ok_or("render scale must numeric")?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err("render window scale invalid".into());
    }
    let environment = Environment::read()?;
    let mut api = SceneCanvas {
        raw: Raw::open(helper, library)?,
        root: root.into(),
        scale,
        font: None,
        emoji: std::collections::BTreeMap::new(),
    };
    let (width, height) = if big { (2160, 1080) } else { (536, 240) };
    let even = |v: i32| {
        let n = (f64::from(v) * scale) as i32;
        n + n.rem_euclid(2)
    };
    let (w, h) = (even(width), even(height));
    api.call(if environment.vsync {
        "flags 96"
    } else {
        "flags 32"
    })?;
    api.call(&format!("window {w} {h} {}", hex("Source shared UI")))?;
    let target = scale != 1.0 || environment.burn_in || environment.record;
    if target {
        api.call(&format!("target {w} {h}"))?;
    }
    let recorder = environment.start_recording(w, h)?;
    let language = scene
        .get("language")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("en");
    load_fonts(&mut api, big, language)?;
    if environment.burn_in {
        api.call(&format!(
            "shader {} {}",
            hex(include_str!("../tests/fixtures/ui-diagnostics/vertex.glsl")),
            hex(include_str!(
                "../tests/fixtures/ui-diagnostics/fragment.glsl"
            ))
        ))?;
    }
    if environment.startup_complete() {
        api.raw.close()?;
        return Ok(None);
    }
    let elements = document.elements()?;
    let mut widgets = Vec::new();
    for element in &elements {
        let mut widget =
            layout::Widget::new(&element.kind, "", element.rect, element.props.clone())?;
        widget.set_raw_values(element.text.clone(), element.value.clone())?;
        widget.initialize(&mut api, if big { 1.242 } else { 1.16 })?;
        widgets.push(widget);
    }
    let count = document.frame_count()?;
    let global = if big { 1.242 } else { 1.16 };
    for frame in 0..count {
        api.call(if target { "target-begin" } else { "begin" })?;
        api.call("clear 4278190080")?;
        if scale != 1.0 {
            api.call(&format!("matrix-scale {scale}"))?;
        }
        for (widget, e) in widgets.iter_mut().zip(&elements) {
            let events = if frame == 0 { e.events()? } else { Vec::new() };
            widget.render(&mut api, global, frame as f64 / 20.0, &events)?;
        }
        if frame + 1 == count {
            api.call(&format!(
                "{} {}",
                if target { "export-target" } else { "export" },
                hex(output.to_str().ok_or("render output path UTF8")?)
            ))?;
        }
        if scale != 1.0 {
            api.call("matrix-end")?;
        }
        if target {
            api.call("target-end")?;
            api.call("begin")?;
            api.call("clear 4278190080")?;
            if environment.burn_in {
                api.call("shader-begin")?;
            }
            api.call("target-draw")?;
            if environment.burn_in {
                api.call("shader-end")?;
            }
        }
        if frame + 1 < count {
            if environment.show_fps {
                api.call("fps")?;
            }
            if !environment.grid.starts_with('-') && environment.grid != "0" {
                if let Ok(grid) = environment.grid.parse::<i32>() {
                    for x in (0..=w).step_by(grid as usize) {
                        api.call(&format!("line {x} 0 {x} {h} 4282137660"))?;
                    }
                    for y in (0..=h).step_by(grid as usize) {
                        api.call(&format!("line 0 {y} {w} {y} 4282137660"))?;
                    }
                }
            }
        }
        api.call("end")?;
        if frame + 1 < count {
            if let Some(recorder) = &recorder {
                if (frame + 1).is_multiple_of(3) {
                    recorder.submit(&mut api)?;
                }
            }
            if environment.strict {
                api.call("strict-fps")?;
            }
            if environment.render_complete(frame + 1) {
                api.raw.close()?;
                return Ok(None);
            }
        }
    }
    if count == 0 {
        api.call("export-idle")?;
        api.call(&format!(
            "export {}",
            hex(output.to_str().ok_or("render output UTF8")?)
        ))?;
        api.call("end")?;
    }
    let states = widgets
        .iter()
        .map(layout::Widget::state_json)
        .collect::<Result<Vec<_>>>()?;
    api.raw.close()?;
    Ok(Some(crate::ui_forms_input::json_array(&states)?))
}
pub fn render(
    root: &Path,
    scene: &[u8],
    output: &Path,
    helper: &Path,
    library: &Path,
) -> Result<Option<serde_json::Value>> {
    render_lossless(root, scene, output, helper, library)?
        .map(|value| Ok(serde_json::from_str(&value.to_json()?)?))
        .transpose()
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
        let input = child.stdin.take().ok_or("render raw stdin missing")?;
        let output = BufReader::new(child.stdout.take().ok_or("render raw stdout missing")?);
        let mut raw = Self {
            child,
            input,
            output,
            closed: false,
        };
        raw.reply()?;
        Ok(raw)
    }
    fn reply(&mut self) -> Result<String> {
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        if line == "OK\n" {
            return Ok(String::new());
        }
        if let Some(v) = line.strip_prefix("OK ") {
            return Ok(v.trim_end().into());
        }
        if let Some(v) = line.trim_end().strip_prefix("ERR ") {
            let bytes = (0..v.len())
                .step_by(2)
                .map(|at| {
                    Ok(u8::from_str_radix(
                        v.get(at..at + 2).ok_or("malformed render reply")?,
                        16,
                    )?)
                })
                .collect::<Result<Vec<_>>>()?;
            return Err(String::from_utf8_lossy(&bytes).into_owned().into());
        }
        Err(format!("render raw helper ended or invalid reply {line:?}").into())
    }
    fn request(&mut self, command: &str) -> Result<String> {
        // CFFI admits embedded NUL in text buffers; raylib observes the C
        // prefix while the widget snapshot retains every original character.
        let mut command = command.to_owned();
        if ["text ", "text-spaced ", "measure ", "measure-spaced "]
            .iter()
            .any(|prefix| command.starts_with(prefix))
        {
            let start = command.find(' ').unwrap() + 1;
            if let Some(end) = command[start..].find(' ').map(|v| v + start) {
                if let Some(nul) = command.as_bytes()[start..end]
                    .chunks_exact(2)
                    .position(|v| v == b"00")
                {
                    let prefix = &command[start..start + nul * 2];
                    let prefix = if prefix.is_empty() { "-" } else { prefix };
                    command = format!("{}{}{}", &command[..start], prefix, &command[end..]);
                }
            }
        }
        writeln!(self.input, "{command}")?;
        self.input.flush()?;
        self.reply()
    }
    pub fn close(&mut self) -> Result<()> {
        self.request("quit")?;
        let code = self.child.wait()?;
        self.closed = true;
        if code.success() {
            Ok(())
        } else {
            Err("render raw cleanup failed".into())
        }
    }
}
impl Canvas for Raw {
    fn call(&mut self, command: &str) -> Result<()> {
        let reply = self.request(command)?;
        if !reply.is_empty() {
            return Err("unexpected render raw payload".into());
        }
        Ok(())
    }
    fn measure(&mut self, text: &str, size: f64) -> Result<f64> {
        Ok(self.measure_vector(text, size)?.0)
    }
    fn measure_vector(&mut self, text: &str, size: f64) -> Result<(f64, f64)> {
        let reply = self.request(&format!("measure {} {size}", hex(text)))?;
        let values = reply
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if values.len() != 2 {
            return Err("invalid render measure reply".into());
        }
        Ok((f64::from(values[0]), f64::from(values[1])))
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
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("ui-render-reference SCENE OUTPUT".into());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    validate_source(&root)?;
    let exe = std::env::current_exe()?;
    let helper = std::env::var_os("UI_RENDER_RAW_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| exe.parent().unwrap().join("ui-render-raw"));
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|v| PathBuf::from(v).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    let resolved_output = crate::path_resolve::resolve_non_strict(Path::new(&args[1]))?;
    if let Some(states) = render_lossless(
        &root,
        &fs::read(&args[0])?,
        &resolved_output,
        &helper,
        &library,
    )? {
        fs::write(
            resolved_output.with_extension("json"),
            crate::control_tools_policy_check::json::pretty_with_limit(&states, 0, 996)?,
        )?;
    }
    Ok(())
}

pub fn build(output: &Path, raylib: &Path) -> Result<PathBuf> {
    openpilot_disk_space::inspect(output, 16 * 1024 * 1024)?.require()?;
    fs::create_dir_all(output)?;
    fs::write(
        output.join("raw-raylib.cc"),
        include_bytes!("../tests/fixtures/ui-render/raw-raylib.cc"),
    )?;
    let binary = output.join("ui-render-raw");
    let status = Command::new(std::env::var_os("CXX").unwrap_or_else(|| "c++".into()))
        .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-O2"])
        .arg("-I")
        .arg(raylib.join("include"))
        .arg(output.join("raw-raylib.cc"))
        .arg("-ldl")
        .arg("-o")
        .arg(&binary)
        .status()?;
    if !status.success() {
        return Err("render raw boundary build failed".into());
    }
    Ok(binary)
}
