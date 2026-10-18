//! Independent Forms reference for all widget families and raw drawing boundaries.
use crate::{
    seal::Result,
    ui_forms_input::{hex, Canvas, Input, InputFrame, Rect},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
#[path = "ui_forms_assets.rs"]
pub(crate) mod assets;
#[path = "ui_forms_controls.rs"]
pub(crate) mod controls;
#[path = "ui_forms_emoji.rs"]
mod emoji;
#[path = "ui_forms_environment.rs"]
mod environment;
pub use emoji::raster as emoji_raster;
pub(crate) use environment::Environment as ReferenceEnvironment;
#[path = "ui_forms_html.rs"]
pub(crate) mod html;
#[path = "ui_forms_keyboard.rs"]
mod keyboard;
#[path = "ui_forms_list.rs"]
mod list;
#[path = "ui_forms_mici.rs"]
mod mici;
#[path = "ui_forms_scene.rs"]
mod scene_json;
#[path = "ui_forms_slider.rs"]
mod slider;
fn truth(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(v) => *v,
        serde_json::Value::Number(v) => v.as_f64() != Some(0.0),
        serde_json::Value::String(v) => !v.is_empty(),
        serde_json::Value::Array(v) => !v.is_empty(),
        serde_json::Value::Object(v) => !v.is_empty(),
    }
}
fn slice_limit(value: &serde_json::Value) -> Result<i64> {
    match value {
        serde_json::Value::Bool(v) => Ok(i64::from(*v)),
        serde_json::Value::Number(v) if v.is_i64() => Ok(v.as_i64().unwrap()),
        serde_json::Value::Number(v) if v.is_u64() => {
            Ok(v.as_u64().unwrap().min(i64::MAX as u64) as i64)
        }
        _ => Err("Forms slice limit must integer".into()),
    }
}
fn frame_count(value: &serde_json::Value) -> Result<usize> {
    Ok(slice_limit(value)?.max(0) as usize)
}
fn numeric_key(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Bool(v) => Some(i64::from(*v)),
        serde_json::Value::Number(v) => v.as_i64().or_else(|| {
            let v = v.as_f64()?;
            (v.is_finite() && v.fract() == 0.0 && v >= i64::MIN as f64 && v < i64::MAX as f64)
                .then_some(v as i64)
        }),
        _ => None,
    }
}
#[derive(Deserialize)]
struct Scene {
    config: Config,
    kind: String,
    rect: Rect,
    #[serde(default)]
    text: String,
    frames: serde_json::Value,
    #[serde(default = "empty_props")]
    props: serde_json::Value,
    #[serde(default = "english")]
    language: String,
}
struct FrameSnapshot {
    state: openpilot_log_format::JsonValue,
    pops: usize,
    confirmed: usize,
    // Python snapshots retain a list object. Later in-place Mici edits must
    // remain visible, while every other field is frozen at this frame.
    text: Option<std::rc::Rc<std::cell::RefCell<openpilot_log_format::JsonValue>>>,
}
fn empty_props() -> serde_json::Value {
    serde_json::json!({})
}
fn english() -> String {
    "en".into()
}
#[derive(Deserialize)]
struct Config {
    big: serde_json::Value,
    scale: f64,
    #[serde(rename = "large_viewport")]
    _large_viewport: serde_json::Value,
}
pub fn validate_source(root: &Path) -> Result<()> {
    let manifest = include_str!("../tests/fixtures/ui-forms/input-provenance.json");
    if format!("{:x}", Sha256::digest(manifest.as_bytes()))
        != "e72c683f958ec93f1a780284f024d4a30dc0e6496f491f8f44d293efc870c540"
    {
        return Err("forms input provenance manifest changed; independently recapture".into());
    }
    let pins: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-forms/input-provenance.json"
    ))?;
    for entry in fs::read_dir(root.join("openpilot/common"))? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if name == "params_pyx"
            || name == "params_pyx.py"
            || name == "params_pyx.pyc"
            || (name.starts_with("params_pyx.") && name.ends_with(".so"))
        {
            return Err("Forms Params import layout changed; independently recapture".into());
        }
    }
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("forms provenance missing")?
    {
        let retired = match name.as_str() {
            "rust/tools/check_ui_forms.py" => {
                Some("a681ddf7f1abf25219d5e89c113c624733dcbf6d08459e20e2382c6dbd2d0900")
            }
            "rust/tools/ui_forms_source.py" => {
                Some("2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365")
            }
            _ => None,
        };
        if retired.is_some_and(|expected| hash.as_str() != Some(expected)) {
            return Err("Forms original capture hash changed".into());
        }
        match fs::read(root.join(name)) {
            Ok(bytes)
                if Some(format!("{:x}", Sha256::digest(&bytes)).as_str()) != hash.as_str() =>
            {
                return Err(format!("{name} changed; independently recapture Forms").into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && retired.is_some() => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("forms font absence missing")?
    {
        if root
            .join(name.as_str().ok_or("invalid forms font path")?)
            .try_exists()?
        {
            return Err("forms font layout changed; independently recapture".into());
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock["package"]
        .as_array()
        .ok_or("forms locked dependencies missing")?
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
        return Err("forms input dependencies changed; independently recapture".into());
    }
    Ok(())
}
// Snapshot dictionary order controls the original checker's first mismatch.
fn object_in_order(
    value: &openpilot_log_format::JsonValue,
    names: &[&str],
) -> Result<openpilot_log_format::JsonValue> {
    let mut fields = Vec::new();
    for &name in names {
        fields.push(format!(
            "{}:{}",
            serde_json::to_string(name)?,
            value
                .get(name)
                .ok_or("Forms snapshot field missing")?
                .to_json()?
        ));
    }
    Ok(openpilot_log_format::JsonValue::parse(&format!(
        "{{{}}}",
        fields.join(",")
    ))?)
}
fn snapshot_json(value: serde_json::Value, kind: &str) -> Result<openpilot_log_format::JsonValue> {
    let value = openpilot_log_format::JsonValue::parse(&serde_json::to_string(&value)?)?;
    match kind {
        "slider" => object_in_order(
            &value,
            &["confirmed", "percentage", "position", "scale", "dragging"],
        ),
        "list" => {
            let right = object_in_order(
                &value.get("right").ok_or("Forms list right missing")?,
                &["x", "y", "width", "height"],
            )?;
            Ok(openpilot_log_format::JsonValue::parse(&format!(
                "{{\"description_visible\":{},\"height\":{},\"right\":{}}}",
                value.get("description_visible").unwrap().to_json()?,
                value.get("height").unwrap().to_json()?,
                right.to_json()?
            ))?)
        }
        _ => Ok(value),
    }
}
pub fn render_input(
    root: &Path,
    scene_bytes: &[u8],
    output: &Path,
    api: &mut dyn Canvas,
) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(
        &render_input_json(root, scene_bytes, output, api)?.to_json()?,
    )?)
}
pub fn render_input_json(
    root: &Path,
    scene_bytes: &[u8],
    output: &Path,
    api: &mut dyn Canvas,
) -> Result<openpilot_log_format::JsonValue> {
    let document = scene_json::Document::parse(scene_bytes)?;
    let scene: Scene = serde_json::from_value(document.bootstrap()?)?;
    let environment = environment::Environment::read()?;
    let (width, height) = if truth(&scene.config.big) {
        (2160, 1080)
    } else {
        (536, 240)
    };
    let pixel_scale = scene.config.scale;
    if !pixel_scale.is_finite() || pixel_scale <= 0.0 {
        return Err("forms scale must be positive and finite".into());
    }
    let mut surface = Surface {
        inner: api,
        scale: pixel_scale,
        root,
        emoji_font: None,
        emoji_textures: std::collections::BTreeMap::new(),
    };
    let api: &mut dyn Canvas = &mut surface;
    let scaled = |v: i32| {
        let v = (f64::from(v) * pixel_scale) as i32;
        v + v.rem_euclid(2)
    };
    let (pixel_width, pixel_height) = (scaled(width), scaled(height));
    let target = pixel_scale != 1.0 || environment.burn_in || environment.record;
    let scale = if truth(&scene.config.big) {
        1.242
    } else {
        1.16
    };
    api.call(if environment.vsync {
        "flags 96"
    } else {
        "flags 32"
    })?;
    api.call(&format!(
        "window {pixel_width} {pixel_height} {}",
        hex("Source forms")
    ))?;
    if target {
        api.call(&format!("target {pixel_width} {pixel_height}"))?;
    }
    let recorder = environment.start_recording(pixel_width, pixel_height)?;
    let fallback = matches!(
        scene.language.as_str(),
        "th" | "zh-CHT" | "zh-CHS" | "ko" | "ja"
    );
    let font = root
        .join("openpilot/selfdrive/assets/fonts")
        .join(if fallback {
            "KaiGenGothicKR-Bold.ttf"
        } else if truth(&scene.config.big) {
            "Inter-Regular.ttf"
        } else {
            "Inter-Medium.ttf"
        });
    let codepoints = if fallback {
        (32..127)
            .chain(0x3400..0x4dc0)
            .chain(0x4e00..0xa000)
            .chain(0xac00..0xd7a4)
            .collect::<Vec<_>>()
    } else {
        (32..127).collect::<Vec<_>>()
    };
    let points = codepoints
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    api.call(&format!(
        "font {} {} {} {points}",
        hex(font.to_str().ok_or("forms font path UTF8")?),
        if fallback { 48 } else { 200 },
        codepoints.len()
    ))?;
    if fallback {
        api.call("font-fallback")?;
    } else if scene.kind != "input" {
        let bold = root.join("openpilot/selfdrive/assets/fonts/Inter-Bold.ttf");
        api.call(&format!(
            "font-load 1 {} 200 95 {points}",
            hex(bold.to_str().ok_or("forms font path UTF8")?)
        ))?;
        let medium = root.join("openpilot/selfdrive/assets/fonts/Inter-Medium.ttf");
        api.call(&format!(
            "font-load 2 {} 200 95 {points}",
            hex(medium.to_str().ok_or("forms font path UTF8")?)
        ))?;
        let semi = root.join("openpilot/selfdrive/assets/fonts/Inter-SemiBold.ttf");
        api.call(&format!(
            "font-load 3 {} 200 95 {points}",
            hex(semi.to_str().ok_or("forms font path UTF8")?)
        ))?;
    }
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
        return Ok(openpilot_log_format::JsonValue::parse("null")?);
    }
    let mut scene: Scene = serde_json::from_value(document.prepare()?)?;
    if let Some(props) = scene.props.as_object_mut() {
        for name in [
            "password",
            "toggle",
            "show_touches",
            "center",
            "rich",
            "green",
        ] {
            if let Some(value) = props.get_mut(name) {
                *value = serde_json::Value::Bool(truth(value));
            }
        }
    }
    if !matches!(
        scene.kind.as_str(),
        "input" | "html" | "confirm" | "options" | "list" | "keyboard" | "mici" | "slider"
    ) {
        return Err("original Forms helper rejects unknown widget kind".into());
    }
    if !scene.props.is_object() {
        return Err("Forms props must a mapping".into());
    }
    let text_value = document.text_value();
    let text_codepoints = if matches!(scene.kind.as_str(), "html" | "confirm") {
        document.text_codepoints()?
    } else {
        Vec::new()
    };
    let mut html = if scene.kind == "html" {
        Some(html::Html::new_codepoints(
            &text_codepoints,
            scene
                .props
                .get("size")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(48.0),
            scene.props["center"] == true,
        )?)
    } else {
        None
    };
    let mut confirm = if scene.kind == "confirm" {
        Some(controls::Confirm::new_codepoints(
            &text_codepoints,
            &"Cancel".chars().map(u32::from).collect::<Vec<_>>(),
            scene.props["rich"] == true,
        )?)
    } else {
        None
    };
    let mut options = if scene.kind == "options" {
        Some(controls::Options::new_codepoints(&text_codepoints))
    } else {
        None
    };
    let mut list = if scene.kind == "list" {
        Some(list::List::new_codepoints(
            &text_codepoints,
            &document.description_codepoints()?,
            &scene.props,
            scene.rect.cast(),
        )?)
    } else {
        None
    };
    let mut keyboard = if scene.kind == "keyboard" {
        Some(keyboard::Keyboard::new(
            root,
            &scene.text,
            &scene.props,
            api,
            scale,
        )?)
    } else {
        None
    };
    let mut mici = if scene.kind == "mici" {
        Some(mici::Mici::new_codepoints(root, &text_codepoints, "", api)?)
    } else {
        None
    };
    let mut slider = if scene.kind == "slider" {
        Some(slider::Slider::new_codepoints(
            root,
            &text_codepoints,
            scene.props["green"] == true,
            api,
        )?)
    } else {
        None
    };
    if let Some(mici) = &mut mici {
        mici.set_value(text_value.clone());
        if let Some(auto) = document.property("auto_return") {
            mici.set_auto_return(auto);
        }
    }
    if let Some(options) = &mut options {
        options.set_value(text_value.clone());
    }
    if let Some(list) = &mut list {
        list.set_value(text_value.clone());
        if let Some(visible) = document.property("description_visible") {
            list.set_description_visible(visible);
        }
    }
    if let Some(slider) = &mut slider {
        slider.set_value(text_value.clone())?;
    }
    if let Some(confirm) = &mut confirm {
        if let Some(cancel) = document.property("cancel") {
            confirm.set_cancel_value(cancel)?;
        }
    }
    if let Some(keyboard) = &mut keyboard {
        keyboard.set_value(text_value.clone(), api, 0.0, scale)?;
    }
    // Only InputBox reads max/password at this level. Other widgets must not
    // construct an unused InputBox or validate its unrelated properties.
    let mut input = Input::new(255, false);
    let debug = scene.props.get("show_touches").is_some_and(truth);
    controls::set_debug(debug);
    if scene.kind == "input" {
        let max = scene.props.get("max");
        let limit = match max {
            Some(value) if value.is_null() => 255,
            Some(value) => slice_limit(value)?,
            None => 255,
        };
        let password = scene.props.get("password").is_some_and(truth);
        input = Input::new(limit, password);
        if max.is_some_and(serde_json::Value::is_null) {
            input.none_limit();
        }
        input.set_value(text_value, api, 0.0, scale)?;
    }
    let frames = frame_count(&scene.frames)?;
    if frames == 0 {
        api.call("export-idle")?;
        api.call(&format!(
            "export {}",
            hex(output.to_str().ok_or("Forms output path UTF8")?)
        ))?;
        api.call("end")?;
        return Ok(openpilot_log_format::JsonValue::parse("[]")?);
    }
    let mut rows = Vec::new();
    let mut pops = 0;
    let mut outer_touch = controls::Touch::default();
    let actions = document.actions()?;
    for index in 0..frames {
        let now = index as f64 / 20.0;
        api.call(if target { "target-begin" } else { "begin" })?;
        if pixel_scale != 1.0 {
            api.call(&format!("matrix-scale {pixel_scale}"))?;
        }
        api.call("clear 4278190080")?;
        let mut keys = vec![];
        let mut down = vec![];
        let mut characters = vec![];
        let mut events = vec![];
        let mut raw_events = Vec::new();
        for raw_action in &actions {
            if !scene_json::Document::frame_matches(raw_action, index)? {
                continue;
            }
            let projected = scene_json::Document::action(raw_action, &scene.kind)?;
            raw_events.extend(projected.events);
            let text_codepoints = projected.text_codepoints;
            let text_value = projected.text_value;
            let selection = projected.selection;
            let action = &projected.value;
            if let Some(key) = action.get("key").filter(|v| truth(v)) {
                // Keys are compared with integer constants by the original;
                // unknown values can be ignored, while numeric bool/float keys
                // retain Python equality with those constants.
                if let Some(key) = numeric_key(key) {
                    keys.push(key);
                }
            }
            if let Some(value) = action.get("down") {
                match value {
                    serde_json::Value::Array(values) => {
                        for value in values {
                            if let Some(value) = numeric_key(value) {
                                down.push(value);
                            }
                        }
                    }
                    serde_json::Value::String(_) | serde_json::Value::Object(_) => {}
                    _ => return Err("Forms down must iterable".into()),
                }
            }
            match action["operation"].as_str().unwrap_or("") {
                "character" => {
                    characters.extend(text_codepoints.ok_or("Forms character codepoints missing")?)
                }
                "text" => {
                    if let Some(mici) = &mut mici {
                        mici.set_value(text_value.ok_or("Forms Mici text missing")?);
                    } else if let Some(keyboard) = &mut keyboard {
                        keyboard.set_value(
                            text_value.ok_or("Forms keyboard text missing")?,
                            api,
                            now,
                            scale,
                        )?;
                    } else if let Some(confirm) = &mut confirm {
                        confirm.set_value(
                            text_value.ok_or("Forms confirm text missing")?,
                            api,
                            scale,
                        )?;
                    } else if let Some(html) = &mut html {
                        html.parse_codepoints(
                            text_codepoints
                                .as_deref()
                                .ok_or("Forms HTML codepoints missing")?,
                        )?;
                    } else if scene.kind == "input" {
                        input.set_value(
                            text_value.ok_or("Forms Input text missing")?,
                            api,
                            now,
                            scale,
                        )?;
                    } else {
                        return Err("original Forms widget has no set_text method".into());
                    }
                }
                "cursor" if scene.kind == "input" => {
                    input.cursor_value(
                        action.get("value").ok_or("Forms cursor value missing")?,
                        api,
                        now,
                        scale,
                    )?;
                }
                "clear" => {
                    if let Some(keyboard) = &mut keyboard {
                        keyboard.clear();
                    } else if scene.kind == "input" {
                        input.clear();
                    } else {
                        return Err("original Forms widget has no clear method".into());
                    }
                }
                "key" if keyboard.is_some() => keyboard.as_mut().unwrap().key_value(
                    text_value.ok_or("Forms keyboard key value missing")?,
                    api,
                    now,
                    scale,
                )?,

                "selection" => {
                    if let Some(options) = &mut options {
                        options.set_selection(selection.ok_or("Forms selection missing")?);
                    }
                }
                // Original ignores unrecognized operation labels; methods unsupported by
                // InputBox fail if explicitly requested rather than silently succeeding.
                "backspace" if mici.is_some() => mici.as_mut().unwrap().backspace()?,
                "space" if mici.is_some() => mici.as_mut().unwrap().space()?,
                "key" | "backspace" | "space" | "cursor" => {
                    return Err("operation not supported by original InputBox".into());
                }
                _ => {}
            }
        }
        for raw_event in raw_events {
            let mut event = scene_json::Document::event(&raw_event)?;
            // Python bool is a valid list index; preserve float index type.
            if let Some(slot) = event["slot"].as_bool() {
                event["slot"] = i64::from(slot).into();
            }
            events.push(event);
        }
        if let Some(mici) = &mut mici {
            mici.render(api, scene.rect.cast(), scale, now, &events)?;
        } else if let Some(slider) = &mut slider {
            slider.render(api, scene.rect.cast(), scale, now, &events)?;
        } else if let Some(keyboard) = &mut keyboard {
            pops += keyboard.render(
                api,
                scene.rect.cast(),
                scale,
                now,
                &keys,
                &down,
                &characters,
                &events,
            )?;
        } else if let Some(list) = &mut list {
            list.render(api, scale, &events)?;
        } else if let Some(options) = &mut options {
            pops += options.render(api, scene.rect.cast(), scale, &events)?;
        } else if let Some(confirm) = &mut confirm {
            pops += confirm.render(
                api,
                f64::from(width),
                f64::from(height),
                scale,
                &keys,
                &events,
            )?;
        } else if let Some(html) = &html {
            html.render(api, scene.rect.cast(), scale)?;
            if debug {
                api.call(&format!(
                    "outline {} {} {} {} 4281805286",
                    scene.rect.x.trunc(),
                    scene.rect.y.trunc(),
                    scene.rect.width.trunc().max(1.0),
                    scene.rect.height.trunc().max(1.0)
                ))?;
            }
        } else {
            input.frame(
                api,
                InputFrame {
                    rect: scene.rect.cast(),
                    now,
                    scale,
                    key: keys.first().copied().unwrap_or(0),
                    down: &down,
                    character: characters.first().copied(),
                    events: &events,
                    debug,
                },
            )?;
        }
        if scene.kind != "input" && scene.kind != "html" {
            controls::debug_rect(api, scene.rect.cast())?;
        }
        if matches!(
            scene.kind.as_str(),
            "html" | "confirm" | "keyboard" | "options"
        ) {
            // The original outer Widget still tracks its own slots after its
            // child/render callbacks, even when that tracking has no callback.
            outer_touch.update(scene.rect.cast(), &events)?;
        }
        let ordinary = if let Some(slider) = &slider {
            slider.snapshot()
        } else {
            serde_json::Value::Null
        };
        let state = if let Some(mici) = &mici {
            mici.snapshot_json()?
        } else if let Some(html) = &html {
            html.snapshot_json(api, scene.rect.width, scale)?
        } else if let Some(options) = &options {
            options.snapshot_json()?
        } else if let Some(list) = &list {
            list.snapshot_json(api, scale)?
        } else if let Some(keyboard) = &keyboard {
            keyboard.snapshot_json(now)?
        } else if scene.kind == "input" {
            input.snapshot_json(now)?
        } else {
            snapshot_json(ordinary, &scene.kind)?
        };
        rows.push(FrameSnapshot {
            state,
            pops,
            confirmed: slider.as_ref().map_or(0, |s| s.count),
            text: mici.as_ref().map(mici::Mici::text_handle),
        });
        if index + 1 == frames {
            api.call(&format!(
                "{} {}",
                if target { "export-target" } else { "export" },
                hex(output.to_str().ok_or("forms output requires UTF8")?)
            ))?;
        }
        if pixel_scale != 1.0 {
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
        if index + 1 < frames {
            if environment.show_fps {
                api.call("fps")?;
            }
            if !environment.grid.starts_with('-') && environment.grid != "0" {
                if let Ok(grid) = environment.grid.parse::<i32>() {
                    for x in (0..=pixel_width).step_by(grid as usize) {
                        api.call(&format!("line {x} 0 {x} {pixel_height} 4282137660"))?;
                    }
                    for y in (0..=pixel_height).step_by(grid as usize) {
                        api.call(&format!("line 0 {y} {pixel_width} {y} 4282137660"))?;
                    }
                }
            }
        }
        api.call("end")?;
        if index + 1 < frames {
            if let Some(recorder) = &recorder {
                if (index + 1).is_multiple_of(3) {
                    recorder.submit(api)?;
                }
            }
            if environment.strict {
                api.call("strict-fps")?;
            }
            if environment.render_complete(index + 1) {
                return Ok(openpilot_log_format::JsonValue::parse("null")?);
            }
        }
    }
    let mut output_json = vec![91];
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            output_json.push(44);
        }
        let state = if let Some(text) = row.text {
            crate::ui_forms_input::snapshot_object(&[
                ("text", text.borrow().clone()),
                ("candidate", row.state.get("candidate").unwrap()),
                ("caps", row.state.get("caps").unwrap()),
                ("layer", row.state.get("layer").unwrap()),
                ("selected_at", row.state.get("selected_at").unwrap()),
                ("unselect_at", row.state.get("unselect_at").unwrap()),
                ("dragging", row.state.get("dragging").unwrap()),
                ("keys", row.state.get("keys").unwrap()),
            ])?
        } else {
            row.state
        };
        output_json.extend("{\"state\":".chars().map(u32::from));
        output_json.extend(crate::ui_forms_input::json_codepoints(&state)?);
        output_json.extend(
            format!(",\"pops\":{},\"confirmed\":{}}}", row.pops, row.confirmed)
                .chars()
                .map(u32::from),
        );
    }
    output_json.push(93);
    Ok(openpilot_log_format::JsonValue::parse_codepoints(
        &output_json,
    )?)
}
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
            let pixels = emoji::raster(font, text)?;
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
        let input = child.stdin.take().ok_or("forms raw stdin missing")?;
        let output = BufReader::new(child.stdout.take().ok_or("forms raw stdout missing")?);
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
                        v.get(at..at + 2).ok_or("malformed forms reply")?,
                        16,
                    )?)
                })
                .collect::<Result<Vec<_>>>()?;
            return Err(String::from_utf8_lossy(&bytes).into_owned().into());
        }
        Err(format!("forms raw helper ended or invalid reply {line:?}").into())
    }
    fn request(&mut self, command: &str) -> Result<String> {
        // CFFI admits embedded NUL in text buffers; raylib observes the C
        // prefix while the widget snapshot retains every original character.
        let mut command = command.to_owned();
        if command.starts_with("text ") || command.starts_with("measure ") {
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
            Err("forms raw cleanup failed".into())
        }
    }
}
impl Canvas for Raw {
    fn call(&mut self, command: &str) -> Result<()> {
        let reply = self.request(command)?;
        if !reply.is_empty() {
            return Err("unexpected forms raw payload".into());
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
            return Err("invalid forms measure reply".into());
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
        return Err("ui-forms-reference SCENE OUTPUT".into());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    validate_source(&root)?;
    let current = std::env::current_exe()?;
    let helper = std::env::var_os("UI_FORMS_RAW_HELPER")
        .map(PathBuf::from)
        .unwrap_or_else(|| current.parent().unwrap().join("ui-forms-raw"));
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|v| PathBuf::from(v).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| "libopenpilot-raylib.so".into());
    let scene_bytes = fs::read(&args[0])?;
    let output = crate::path_resolve::resolve_non_strict(Path::new(&args[1]))?;
    let mut api = Raw::open(&helper, &library)?;
    let state = render_input_json(&root, &scene_bytes, &output, &mut api)?;
    if !matches!(state.view(), openpilot_log_format::JsonView::Null) {
        fs::write(
            output.with_extension("json"),
            crate::control_tools_policy_check::json::pretty_with_limit(&state, 0, 996)?,
        )?;
    }
    api.close()
}
pub fn build(output: &Path, raylib: &Path) -> Result<PathBuf> {
    openpilot_disk_space::inspect(output, 16 * 1024 * 1024)?.require()?;
    fs::create_dir_all(output)?;
    fs::write(
        output.join("raw-raylib.cc"),
        include_bytes!("../tests/fixtures/ui-forms/raw-raylib.cc"),
    )?;
    let binary = output.join("ui-forms-raw");
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
        return Err("forms raw boundary build failed".into());
    }
    Ok(binary)
}
