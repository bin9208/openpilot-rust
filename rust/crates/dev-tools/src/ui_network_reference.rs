//! Independent source network composition; no candidate widget imports.
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas},
    ui_forms_reference::{Raw, ReferenceEnvironment as Environment},
};
use openpilot_log_format::JsonValue;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
#[path = "ui_network_controls.rs"]
mod controls;
#[path = "ui_network_keyboard.rs"]
mod keyboard;
#[path = "ui_network_model.rs"]
mod model;
#[path = "ui_network_screen.rs"]
mod screen;
struct Surface<'a> {
    inner: &'a mut dyn Canvas,
    scale: f64,
    root: &'a Path,
    emoji_font: Option<Vec<u8>>,
    emoji_textures: std::collections::BTreeMap<String, usize>,
}
impl Canvas for Surface<'_> {
    fn pixel_scale(&self) -> f64 {
        self.scale
    }
    fn measure(&mut self, text: &str, size: f64) -> Result<f64> {
        self.inner.measure(text, size)
    }
    fn measure_vector(&mut self, text: &str, size: f64) -> Result<(f64, f64)> {
        self.inner.measure_vector(text, size)
    }
    fn color_emoji(&mut self, text: &str, x: f64, y: f64, size: f64, rgba: u32) -> Result<()> {
        let id = if let Some(&id) = self.emoji_textures.get(text) {
            id
        } else {
            let font = match &self.emoji_font {
                Some(font) => font,
                None => {
                    self.emoji_font = Some(fs::read(
                        self.root
                            .join("openpilot/selfdrive/assets/fonts/NotoColorEmoji.ttf"),
                    )?);
                    self.emoji_font.as_ref().unwrap()
                }
            };
            let pixels = crate::ui_forms_reference::emoji_raster(font, text)?;
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(&pixels)?;
            file.flush()?;
            let id = self.emoji_textures.len();
            self.inner.call(&format!(
                "emoji-load {id} {}",
                hex(file.path().to_str().ok_or("Forms emoji path UTF8")?)
            ))?;
            self.emoji_textures.insert(text.into(), id);
            id
        };
        self.inner
            .call(&format!("emoji {id} {x} {y} {} {rgba}", size / 128.0))
    }
    fn call(&mut self, command: &str) -> Result<()> {
        if self.scale != 1.0 {
            if let Some(rect) = command.strip_prefix("clip ") {
                let values = rect
                    .split_whitespace()
                    .map(str::parse::<f64>)
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                if values.len() != 4 {
                    return Err("forms clip dimensions missing".into());
                }
                return self.inner.call(&format!(
                    "clip {} {} {} {}",
                    (values[0] * self.scale).trunc(),
                    (values[1] * self.scale).trunc(),
                    (values[2] * self.scale).ceil(),
                    (values[3] * self.scale).ceil()
                ));
            }
        }
        self.inner.call(command)
    }
}

pub fn validate_source(root: &Path) -> Result<()> {
    let source_matches = |name: &str, expected: &str, bytes: &[u8]| {
        // #585: #557의 정확한 font 구간만 기존 application 역투영 경계로 검사한다.
        if name == "openpilot/system/ui/lib/application.py"
            && expected == "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00"
        {
            crate::ui_application_logic_check::application_source_matches(bytes)
        } else {
            format!("{:x}", Sha256::digest(bytes)) == expected
        }
    };
    let manifest = include_str!("../tests/fixtures/ui-network/provenance.json");
    if format!("{:x}", Sha256::digest(manifest.as_bytes()))
        != "4967ad887a8cee21ae050ceb592ab1b432cceb98861c9d2df55105a82d559c5e"
    {
        return Err("network provenance manifest changed; independently recapture".into());
    }
    let pins: serde_json::Value = serde_json::from_str(manifest)?;
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("network provenance missing sources")?
    {
        let retired = match name.as_str() {
            "rust/tools/check_ui_network.py" => {
                Some("f6225784cc275e6c69ecd1b58f7a15ccec42bbea261285d53c24b0802e53f379")
            }
            "rust/tools/ui_network_source.py" => {
                Some("00fb81b7a01eb5b515d9af98c03a7a5a1b7ab25f52f9ffba4d335034e03163b9")
            }
            _ => None,
        };
        if retired.is_some_and(|expected| hash.as_str() != Some(expected)) {
            return Err("network original capture hash changed".into());
        }
        match fs::read(root.join(name)) {
            Ok(bytes) if !source_matches(name, hash.as_str().unwrap_or_default(), &bytes) => {
                return Err(format!("{name} changed; independently recapture network").into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired.is_some() => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("network provenance missing absence")?
    {
        if root
            .join(name.as_str().ok_or("render absence path")?)
            .try_exists()?
        {
            return Err("network font namespace changed; independently recapture".into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock["package"]
        .as_array()
        .ok_or("network dependency lock missing")?
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
        return Err("network dependency lock changed; independently recapture".into());
    }
    Ok(())
}
fn fonts(api: &mut dyn Canvas, root: &Path) -> Result<()> {
    let points = (32..127)
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    for (id, name) in [
        "Inter-Regular",
        "Inter-Bold",
        "Inter-Medium",
        "Inter-SemiBold",
    ]
    .iter()
    .enumerate()
    {
        let path = root
            .join("openpilot/selfdrive/assets/fonts")
            .join(format!("{name}.ttf"));
        let command = if id == 0 {
            "font".into()
        } else {
            format!("font-load {id}")
        };
        api.call(&format!(
            "{command} {} 200 95 {points}",
            hex(path.to_str().ok_or("network font path UTF8")?)
        ))?;
    }
    Ok(())
}
pub fn render(
    root: &Path,
    scene: &[u8],
    output: &Path,
    helper: &Path,
    library: &Path,
) -> Result<Option<JsonValue>> {
    let scene = JsonValue::parse(std::str::from_utf8(scene)?)?;
    let environment = Environment::read()?;
    let mut raw = Raw::open(helper, library)?;
    raw.call(if environment.vsync {
        "flags 96"
    } else {
        "flags 32"
    })?;
    raw.call(&format!(
        "window 2160 1080 {}",
        hex("Source network widgets")
    ))?;
    // Application initializes these resources, but the source helper's manual
    // begin/end loop never enters render textures, shaders or recorder submission.
    if environment.burn_in || environment.record {
        raw.call("target 2160 1080")?;
    }
    let _recorder = environment.start_recording(2160, 1080)?;
    fonts(&mut raw, root)?;
    if environment.burn_in {
        raw.call(&format!(
            "shader {} {}",
            hex(include_str!("../tests/fixtures/ui-diagnostics/vertex.glsl")),
            hex(include_str!(
                "../tests/fixtures/ui-diagnostics/fragment.glsl"
            ))
        ))?;
    }
    if environment.startup_complete() {
        raw.close()?;
        return Ok(None);
    }
    crate::ui_forms_reference::controls::set_debug(
        std::env::var_os("SHOW_TOUCHES").as_deref() == Some(std::ffi::OsStr::new("1")),
    );
    let frames =
        crate::control_tools_policy_check::json::sequence(&model::field(&scene, "frames")?)?;
    let mut api = Surface {
        inner: &mut raw,
        scale: 1.,
        root,
        emoji_font: None,
        emoji_textures: std::collections::BTreeMap::new(),
    };
    let mut screen = screen::Screen::new(
        root,
        model::text(&model::field(&scene, "kind")?)?,
        model::field(&scene, "snapshot")?,
        &mut api,
    )?;
    let mut rows = Vec::new();
    for (index, frame) in frames.iter().enumerate() {
        rows.push(screen.frame(&mut api, frame, index as f64 / 20.)?);
        if index + 1 == frames.len() {
            api.call(&format!(
                "export {}",
                hex(output.to_str().ok_or("network output path UTF8")?)
            ))?;
        }
        api.call("end")?;
    }
    raw.close()?;
    Ok(Some(crate::ui_forms_input::json_array(&rows)?))
}
pub fn cli() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() < 2 {
        return Err("ui-network-reference SCENE OUTPUT".into());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    validate_source(&root)?;
    let helper = std::env::var_os("UI_NETWORK_RAW_HELPER")
        .or_else(|| std::env::var_os("UI_FORMS_RAW_HELPER"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("ui-forms-raw")
        });
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|p| PathBuf::from(p).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    let output = crate::path_resolve::resolve_non_strict(Path::new(&args[1]))?;
    if let Some(state) = render(&root, &fs::read(&args[0])?, &output, &helper, &library)? {
        fs::write(
            output.with_extension("json"),
            crate::control_tools_policy_check::json::pretty_with_limit(&state, 0, 996)?,
        )?;
    }
    Ok(())
}

pub fn states_equal(left: &JsonValue, right: &JsonValue) -> bool {
    crate::control_tools_policy_check::json::equal(left, right)
}
