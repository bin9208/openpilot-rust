//! Independent source polygon policy; candidate renderer is not used.
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use std::{io::Write, path::Path};
/// Scene policy calls, implemented by a checked raw raylib ABI in production.
pub trait PolygonApi {
    fn initialize(&mut self) -> Result<()>;
    fn overflow_warning(&mut self) -> Result<()>;
    fn begin(&mut self) -> Result<()>;
    fn clear(&mut self) -> Result<()>;
    fn load_shader(&mut self, vertex: &str, fragment: &str) -> Result<()>;
    fn location(&mut self, name: &str) -> Result<()>;
    fn integer(&mut self, name: &str, value: i32) -> Result<()>;
    fn floats(&mut self, name: &str, values: &[f32], components: usize) -> Result<()>;
    fn matrix(&mut self, values: &[f32]) -> Result<()>;
    fn begin_shader(&mut self) -> Result<()>;
    fn strip(&mut self, points: &[f32], color: u32) -> Result<()>;
    fn end_shader(&mut self) -> Result<()>;
    fn capture(&mut self, output: &Path) -> Result<()>;
    fn end(&mut self) -> Result<()>;
    fn unload_shader(&mut self) -> Result<()>;
    fn close(&mut self) -> Result<()>;
}
/// Only one NumPy array cast occurs in this isolated CLI. Python's default,
/// always, module and once actions therefore emit the same single diagnostic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WarningAction {
    #[default]
    Emit,
    Ignore,
    Error,
}
pub fn warning_action(value: Option<&std::ffi::OsStr>) -> WarningAction {
    use std::os::unix::ffi::OsStrExt;
    let Some(value) = value else {
        return WarningAction::Emit;
    };
    for option in value.as_bytes().split(|b| *b == b',').rev() {
        let Ok(option) = std::str::from_utf8(option) else {
            continue;
        };
        let fields = option.split(':').map(str::trim).collect::<Vec<_>>();
        if fields.len() > 5 {
            continue;
        }
        let action = fields[0];
        let action = if action.is_empty() {
            "default"
        } else if action == "all" {
            "always"
        } else {
            let Some(action) = ["default", "always", "ignore", "module", "once", "error"]
                .into_iter()
                .find(|name| name.starts_with(action))
            else {
                continue;
            };
            action
        };
        let category = fields.get(2).copied().unwrap_or("");
        if !matches!(
            category,
            "" | "Warning" | "RuntimeWarning" | "builtins.Warning" | "builtins.RuntimeWarning"
        ) {
            continue;
        }
        let message = fields.get(1).copied().unwrap_or("");
        let insensitive = message
            .chars()
            .map(|c| match c {
                'İ' | 'ı' => 'i',
                'ſ' => 's',
                'K' => 'k',
                _ => c.to_ascii_lowercase(),
            })
            .collect::<String>();
        if !"overflow encountered in cast".starts_with(&insensitive) {
            continue;
        }
        if !matches!(fields.get(3).copied().unwrap_or(""), "" | "__main__") {
            continue;
        }
        if let Some(line) = fields.get(4).filter(|line| !line.is_empty()) {
            let Ok(line) = color_integer(&JsonValue::text(line)) else {
                continue;
            };
            if line != 0 && line != 19 {
                continue;
            }
        }
        return match action {
            "ignore" => WarningAction::Ignore,
            "error" => WarningAction::Error,
            _ => WarningAction::Emit,
        };
    }
    WarningAction::Emit
}
pub fn write_overflow_warning(
    action: WarningAction,
    source: &Path,
    stderr: &mut dyn Write,
) -> Result<()> {
    match action {
        WarningAction::Ignore => Ok(()),
        WarningAction::Error => Err("source RuntimeWarning: overflow encountered in cast".into()),
        WarningAction::Emit => {
            let mut filename = String::new();
            for point in crate::python_fs::codepoints(source.as_os_str()) {
                if let Some(character) = char::from_u32(point) {
                    filename.push(character);
                } else {
                    filename.push_str(&format!("\\u{point:04x}"));
                }
            }
            write!(stderr,"{filename}:19: RuntimeWarning: overflow encountered in cast\n  points = np.asarray([[point['x'], point['y']] for point in scene['points']], dtype=np.float32)\n")?;
            Ok(())
        }
    }
}
fn python_float_text(points: &[u32]) -> Result<f64> {
    let text = points
        .iter()
        .map(|p| char::from_u32(*p).ok_or("non-Unicode source number"))
        .collect::<std::result::Result<String, _>>()?;
    let normalized = text
        .trim()
        .chars()
        .map(|c| crate::argparse_scan::decimal_value(c).map_or(c, |d| char::from(b'0' + d)))
        .collect::<Vec<_>>();
    let mut numeric = String::new();
    for (index, character) in normalized.iter().enumerate() {
        if *character == '_' {
            if !index
                .checked_sub(1)
                .and_then(|i| normalized.get(i))
                .is_some_and(char::is_ascii_digit)
                || !normalized.get(index + 1).is_some_and(char::is_ascii_digit)
            {
                return Err("invalid source float separator".into());
            }
        } else {
            numeric.push(*character);
        }
    }
    match numeric.to_ascii_lowercase().as_str() {
        "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
        "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
        "nan" | "+nan" => Ok(f64::NAN),
        "-nan" => Ok(-f64::NAN),
        _ => Ok(numeric.parse()?),
    }
}
fn number(value: &JsonValue, strings: bool) -> Result<f64> {
    match value.view() {
        JsonView::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
        JsonView::Integer(v) => {
            let number = v.parse::<f64>()?;
            if !number.is_finite() {
                return Err("source integer cannot convert to float".into());
            }
            Ok(number)
        }
        JsonView::Float(v) => Ok(v),
        JsonView::Null if strings => Ok(f64::NAN),
        JsonView::Text(v) if strings => python_float_text(v),
        _ => Err("source requires a real number".into()),
    }
}
fn color_integer(value: &JsonValue) -> Result<u32> {
    if let JsonView::Text(points) = value.view() {
        let text = points
            .iter()
            .map(|p| char::from_u32(*p).ok_or("source color has a surrogate"))
            .collect::<std::result::Result<String, _>>()?;
        let text = text.trim();
        let (negative, digits) = if let Some(v) = text.strip_prefix('-') {
            (true, v)
        } else {
            (false, text.strip_prefix('+').unwrap_or(text))
        };
        let mut previous = false;
        let mut result = 0u32;
        let mut count = 0usize;
        for character in digits.chars() {
            if character == '_' {
                if !previous {
                    return Err("invalid source integer separator".into());
                }
                previous = false;
                continue;
            }
            let digit = crate::argparse_scan::decimal_value(character)
                .ok_or("invalid source integer digit")?;
            // CPython's default decimal conversion limit counts all digits,
            // including leading zeros and Unicode Nd, but not signs or '_'.
            count += 1;
            if count > 4300 {
                return Err("source color exceeds the 4300-digit integer conversion limit".into());
            }
            result = result
                .checked_mul(10)
                .and_then(|n| n.checked_add(u32::from(digit)))
                .ok_or("source color exceeds four bytes")?;
            previous = true;
        }
        if !previous || (negative && result != 0) {
            return Err("invalid unsigned source color".into());
        }
        return Ok(result);
    }
    let number = number(value, true)?;
    if !number.is_finite() || number.trunc() < 0.0 || number.trunc() > f64::from(u32::MAX) {
        return Err("source color is outside unsigned four-byte range".into());
    }
    Ok(number.trunc() as u32)
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing source scene field {key}").into())
}
fn sequence(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(v) => Ok(v),
        JsonView::Text(v) => v
            .iter()
            .map(|v| JsonValue::codepoints(vec![*v]).ok_or_else(|| "invalid Unicode value".into()))
            .collect(),
        JsonView::Object(v) => v
            .into_iter()
            .map(|(key, _)| {
                JsonValue::codepoints(key.to_vec()).ok_or_else(|| "invalid Unicode value".into())
            })
            .collect(),
        _ => Err("source requires an iterable".into()),
    }
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(v) => v,
        JsonView::Integer(v) => v.parse::<f64>().unwrap_or(1.0) != 0.0,
        JsonView::Float(v) => v != 0.0,
        JsonView::Text(v) => !v.is_empty(),
        JsonView::Array(v) => !v.is_empty(),
        JsonView::Object(v) => !v.is_empty(),
    }
}
fn mode_is(value: &JsonValue, text: &str) -> bool {
    matches!(value.view(),JsonView::Text(v) if v.iter().copied().eq(text.chars().map(u32::from)))
}
fn values_json(values: &[JsonValue]) -> Result<String> {
    Ok(format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(", ")
    ))
}
fn float_json(value: f64) -> Result<String> {
    JsonValue::parse(&if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else {
        format!("{value:?}")
    })?
    .to_json()
    .map_err(Into::into)
}
struct Coordinate {
    values: Vec<f32>,
    shape: Vec<usize>,
}
fn coordinate_shape(value: &JsonValue, depth: usize) -> Result<Vec<usize>> {
    if depth > 62 {
        return Err("source coordinate array exceeds NumPy dimensions".into());
    }
    if let JsonView::Array(children) = value.view() {
        let children = children
            .iter()
            .map(|v| coordinate_shape(v, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        let first = children.first().cloned().unwrap_or_default();
        if children.iter().any(|v| *v != first) {
            return Err("source coordinate array is ragged".into());
        }
        let mut shape = vec![children.len()];
        shape.extend(first);
        Ok(shape)
    } else {
        Ok(vec![])
    }
}
fn coordinate(value: &JsonValue, overflow: &mut bool) -> Result<Coordinate> {
    if let JsonView::Array(children) = value.view() {
        let length = children.len();
        let children = children
            .iter()
            .map(|v| coordinate(v, overflow))
            .collect::<Result<Vec<_>>>()?;
        let child_shape = children
            .first()
            .map(|v| v.shape.clone())
            .unwrap_or_default();
        let mut shape = vec![length];
        shape.extend(child_shape);
        Ok(Coordinate {
            values: children.into_iter().flat_map(|v| v.values).collect(),
            shape,
        })
    } else {
        let number = number(value, true)?;
        let cast = number as f32;
        *overflow |= number.is_finite() && cast.is_infinite();
        Ok(Coordinate {
            values: vec![cast],
            shape: vec![],
        })
    }
}
fn coordinate_json(value: &Coordinate) -> Result<String> {
    fn write(shape: &[usize], values: &mut std::slice::Iter<'_, f32>) -> Result<String> {
        if let Some((length, child)) = shape.split_first() {
            Ok(format!(
                "[{}]",
                (0..*length)
                    .map(|_| write(child, values))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        } else {
            float_json(f64::from(
                *values.next().ok_or("missing source coordinate")?,
            ))
        }
    }
    write(&value.shape, &mut value.values.iter())
}
// pathlib.Path collapses empty and dot components, but keeps symlink-sensitive
// parent components. In particular, a trailing slash on a file is discarded.
fn python_path(path: &Path) -> std::path::PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
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
        .filter(|part| !part.is_empty() && *part != b".")
    {
        if !result.is_empty() && !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    if result.is_empty() {
        result.push(b'.');
    }
    std::ffi::OsString::from_vec(result).into()
}
#[cfg(feature = "polygon-reference")]
fn source_root(root: &Path) -> Result<std::path::PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let bytes = root.as_os_str().as_bytes();
    let mut parts = vec![];
    for part in bytes.split(|b| *b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut result = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        b"//".to_vec()
    } else {
        b"/".to_vec()
    };
    for part in parts {
        if !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    Ok(std::ffi::OsString::from_vec(result).into())
}
pub fn render(
    scene: &Path,
    output: &Path,
    api: &mut dyn PolygonApi,
    printed: &mut dyn Write,
) -> Result<()> {
    let scene = python_path(scene);
    let output = python_path(output);
    let mut warnings = String::new();
    let result = render_inner(&scene, &output, api, &mut warnings);
    printed.write_all(warnings.as_bytes())?;
    result
}
fn render_inner(
    scene: &Path,
    output: &Path,
    api: &mut dyn PolygonApi,
    warnings: &mut String,
) -> Result<()> {
    let text = String::from_utf8(std::fs::read(scene)?)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let scene = JsonValue::parse(&text)?;
    source_json_depth(&text)?;
    api.initialize()?;
    // The source list comprehension and NumPy shape discovery finish before
    // converting any scalar. Casting can warn even if a later scalar fails.
    let fields = sequence(&field(&scene, "points")?)?
        .iter()
        .map(|point| Ok([field(point, "x")?, field(point, "y")?]))
        .collect::<Result<Vec<_>>>()?;
    let mut shapes = fields
        .iter()
        .flat_map(|p| p.iter())
        .map(|v| coordinate_shape(v, 0));
    let shape = shapes.next().transpose()?.unwrap_or_default();
    for candidate in shapes {
        if candidate? != shape {
            return Err("source coordinate array is ragged".into());
        }
    }
    let dimensions = if fields.is_empty() {
        1
    } else {
        2 + shape.len()
    };
    if dimensions > 64 {
        return Err("source array exceeds NumPy dimensions".into());
    }
    let mut overflow = false;
    let points = fields
        .iter()
        .map(|point| {
            Ok([
                coordinate(&point[0], &mut overflow)?,
                coordinate(&point[1], &mut overflow)?,
            ])
        })
        .collect::<Result<Vec<_>>>();
    if overflow {
        api.overflow_warning()?;
    }
    let points = points?;
    let origin = field(&scene, "origin")?;
    let origin = ["x", "y", "width", "height"]
        .map(|k| {
            field(&origin, k)
                .and_then(|v| number(&v, false))
                .map(|v| v as f32)
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let mut colors = sequence(&field(&scene, "colors")?)?
        .iter()
        .map(color_integer)
        .collect::<Result<Vec<_>>>()?;
    let start = field(&scene, "start")?;
    let end = field(&scene, "end")?;
    let mut stops = field(&scene, "stops")?;
    if colors.len() > 20 {
        colors.truncate(20);
        warnings.push_str("Warning: Gradient colors truncated to 20 entries\n");
    }
    let mut stop_items = sequence(&stops)?;
    if stop_items.len() > 20 {
        stop_items.truncate(20);
        stops = match stops.view() {
            JsonView::Array(_) => JsonValue::parse(&values_json(&stop_items)?)?,
            JsonView::Text(v) => JsonValue::codepoints(v[..20].to_vec()).ok_or("invalid stops")?,
            _ => return Err("source stops cannot be sliced".into()),
        };
        warnings.push_str("Warning: Gradient stops truncated to 20 entries\n");
    }
    if stop_items.is_empty() {
        stop_items = (0..colors.len())
            .map(|i| {
                JsonValue::parse(&format!(
                    "{:?}",
                    i as f64 / colors.len().saturating_sub(1).max(1) as f64
                ))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        stops = JsonValue::parse(&values_json(&stop_items)?)?;
    }
    let expected_error = scene.get("expect_error").is_some_and(|v| truth(&v));
    let length = points.len() - (points.len() % 2);
    let mut interleaved = Vec::new();
    for i in 0..length / 2 {
        interleaved.push(i);
        interleaved.push(length - i - 1);
    }
    let strip = if dimensions == 2 {
        interleaved
            .iter()
            .flat_map(|i| points[*i].iter().flat_map(|v| v.values.iter().copied()))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    let mut initialized = false;
    let mut failed = false;
    for frame in 0..3 {
        api.begin()?;
        api.clear()?;
        let mode = field(&scene, "mode")?;
        if mode_is(&mode, "solid") {
            let color = *colors.first().ok_or("source solid has no color")?;
            if points.len() >= 3 {
                if dimensions != 2 {
                    return Err("source points must be (N,2)".into());
                }
                api.strip(&strip, color)?;
            }
        } else if mode_is(&mode, "gradient") || mode_is(&mode, "color") {
            let solid = if mode_is(&mode, "color") {
                Some(*colors.first().ok_or("source color has no color")?)
            } else {
                None
            };
            if points.len() >= 3 {
                if !initialized {
                    api.load_shader(
                        include_str!("../tests/fixtures/ui-polygon/vertex.glsl"),
                        include_str!("../tests/fixtures/ui-polygon/fragment.glsl"),
                    )?;
                    for name in [
                        "fillColor",
                        "useGradient",
                        "gradientStart",
                        "gradientEnd",
                        "gradientColors",
                        "gradientStops",
                        "gradientColorCount",
                        "mvp",
                    ] {
                        api.location(name)?;
                    }
                    api.matrix(&[
                        2.0 / 536.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        -2.0 / 240.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        -1.0,
                        0.0,
                        -1.0,
                        1.0,
                        -0.0,
                        1.0,
                    ])?;
                    initialized = true;
                }
                if dimensions != 2 {
                    return Err("source points must be (N,2)".into());
                }
                let gradient = solid.is_none() && !colors.is_empty();
                api.integer("useGradient", i32::from(gradient))?;
                if gradient {
                    let mut rgba = vec![];
                    for color in &colors {
                        for byte in color.to_le_bytes() {
                            rgba.push(f32::from(byte) / 255.0);
                        }
                    }
                    api.floats("gradientColors", &rgba, 4)?;
                    if matches!(stops.view(), JsonView::Object(_)) {
                        return Err("source gradient stops has no integer key".into());
                    }
                    let values = stop_items
                        .iter()
                        .map(|v| {
                            if matches!(v.view(), JsonView::Null) {
                                return Err("source float(None) is invalid".into());
                            }
                            number(v, true).map(|v| v.clamp(0.0, 1.0) as f32)
                        })
                        .collect::<Result<Vec<_>>>()?;
                    api.floats("gradientStops", &values, 1)?;
                    api.integer("gradientColorCount", colors.len() as i32)?;
                    let mut endpoints = Vec::new();
                    for value in [&start, &end] {
                        let pair = sequence(value)?;
                        let x = number(pair.first().ok_or("source gradient x missing")?, false)?;
                        let y = number(pair.get(1).ok_or("source gradient y missing")?, false)?;
                        endpoints.push([
                            (f64::from(origin[0]) + x * f64::from(origin[2])) as f32,
                            (f64::from(origin[1]) + y * f64::from(origin[3])) as f32,
                        ]);
                    }
                    for (name, values) in
                        ["gradientStart", "gradientEnd"].into_iter().zip(endpoints)
                    {
                        api.floats(name, &values, 2)?;
                    }
                } else if let Some(color) = solid {
                    api.floats(
                        "fillColor",
                        &color.to_le_bytes().map(|v| f32::from(v) / 255.0),
                        4,
                    )?;
                } else if expected_error {
                    failed = true;
                } else {
                    return Err("source WHITE tuple has no r attribute".into());
                }
                if gradient || solid.is_some() {
                    api.begin_shader()?;
                    api.strip(&strip, u32::MAX)?;
                    api.end_shader()?;
                }
            }
        }
        if frame == 2 {
            api.capture(output)?;
        }
        api.end()?;
    }
    let strip = interleaved
        .iter()
        .map(|index| {
            Ok(format!(
                "{{\"x\": {}, \"y\": {}}}",
                coordinate_json(&points[*index][0])?,
                coordinate_json(&points[*index][1])?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let colors = colors
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        output.with_extension("json"),
        format!(
            "{{\"failed\": {failed}, \"strip\": [{}], \"stops\": {}, \"colors\": [{colors}]}}",
            strip.join(", "),
            stops.to_json()?
        ),
    )?;
    if initialized {
        api.unload_shader()?;
    }
    api.close()?;
    Ok(())
}

// The pinned original Python 3.12.15 CLI accepts 9,997 containers and
// rejects the next one before initializing raylib. Scan validated JSON so
// overwritten object values count too, without recursive Rust stack growth.
fn source_json_depth(text: &str) -> Result<()> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > 9_997 {
                        return Err("source JSON exceeds the pinned decoder recursion limit".into());
                    }
                }
                b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
    }
    Ok(())
}

/// The retained original runtime remains authoritative provenance for this
/// independent policy; edits require a fresh original/reference comparison.
pub fn validate_source(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let pins: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/ui-polygon/policy-pins.json"
    ))?;
    for (name, hash) in pins["sources"]
        .as_object()
        .ok_or("missing polygon source pins")?
    {
        // This tool is replaced by this independent native reference; runtime
        // shader/import sources remain live drift guards until their own ports.
        let retired = name == "rust/tools/ui_qa/polygon_source.py";
        if retired && hash != "1be2b79db03c8d5db77ac67714b775527167b3f43e1bf7026888187304a100a8" {
            return Err("polygon helper provenance changed".into());
        }
        let bytes = match std::fs::read(root.join(name)) {
            Err(error) if retired && error.kind() == std::io::ErrorKind::NotFound => continue,
            other => other?,
        };
        let observed = format!("{:x}", Sha256::digest(bytes));
        if hash.as_str() != Some(&observed) {
            return Err(
                format!("{name} changed; independently revalidate the polygon reference").into(),
            );
        }
    }
    for name in pins["absent"]
        .as_array()
        .ok_or("missing polygon namespace pins")?
    {
        if root
            .join(name.as_str().ok_or("invalid namespace pin")?)
            .try_exists()?
        {
            return Err("polygon namespace contract changed".into());
        }
    }
    let lock: toml::Value = toml::from_str(&std::fs::read_to_string(root.join("uv.lock"))?)?;
    let records = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing polygon source dependencies")?
        .iter()
        .filter(|p| {
            p.get("name")
                .and_then(toml::Value::as_str)
                .is_some_and(|name| {
                    matches!(name, "numpy" | "comma-deps-raylib" | "cffi" | "pycparser")
                })
        })
        .collect::<Vec<_>>();
    if serde_json::to_value(records)? != pins["lock"] {
        return Err("polygon source dependencies changed; revalidate reference".into());
    }
    Ok(())
}
#[cfg(feature = "polygon-reference")]
pub struct Raylib {
    pub api: openpilot_startup_ui::polygon_reference_abi::RawApi,
    pub warning_source: std::path::PathBuf,
    pub warnings: WarningAction,
}
#[cfg(feature = "polygon-reference")]
impl PolygonApi for Raylib {
    fn overflow_warning(&mut self) -> Result<()> {
        write_overflow_warning(
            self.warnings,
            &self.warning_source,
            &mut std::io::stderr().lock(),
        )
    }

    fn initialize(&mut self) -> Result<()> {
        self.api.flags(32)?;
        self.api.initialize(536, 240, "Source ribbon polygons")?;
        self.api.fps(0)?;
        Ok(())
    }
    fn begin(&mut self) -> Result<()> {
        Ok(self.api.begin()?)
    }
    fn clear(&mut self) -> Result<()> {
        Ok(self.api.clear(0xff000000)?)
    }
    fn load_shader(&mut self, vertex: &str, fragment: &str) -> Result<()> {
        Ok(self.api.load_shader(vertex, fragment)?)
    }
    fn location(&mut self, name: &str) -> Result<()> {
        Ok(self.api.location(name)?)
    }
    fn integer(&mut self, name: &str, value: i32) -> Result<()> {
        Ok(self.api.integer(name, value)?)
    }
    fn floats(&mut self, name: &str, values: &[f32], components: usize) -> Result<()> {
        Ok(self.api.floats(
            name,
            values,
            u32::try_from(components)?,
            ["gradientColors", "gradientStops"].contains(&name),
        )?)
    }
    fn matrix(&mut self, values: &[f32]) -> Result<()> {
        Ok(self.api.matrix("mvp", values)?)
    }
    fn begin_shader(&mut self) -> Result<()> {
        Ok(self.api.begin_shader()?)
    }
    fn strip(&mut self, points: &[f32], color: u32) -> Result<()> {
        Ok(self.api.strip(points, color)?)
    }
    fn end_shader(&mut self) -> Result<()> {
        Ok(self.api.end_shader()?)
    }
    fn capture(&mut self, output: &Path) -> Result<()> {
        self.api.flush()?;
        let image = self.api.load_capture()?;
        // pyray encodes the Python path as strict UTF-8 at ExportImage, after
        // loading the capture and before calling the image export ABI.
        output
            .to_str()
            .ok_or("source image output path is not UTF-8")?;
        if !self.api.export_capture(image, output)? {
            return Err("source image export failed".into());
        }
        self.api.unload_capture(image)?;
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        Ok(self.api.end()?)
    }
    fn unload_shader(&mut self) -> Result<()> {
        Ok(self.api.unload_shader()?)
    }
    fn close(&mut self) -> Result<()> {
        Ok(self.api.close()?)
    }
}
#[cfg(feature = "polygon-reference")]
pub fn cli() -> Result<()> {
    use std::path::PathBuf;
    let args = std::env::args_os().collect::<Vec<_>>();
    let scene = Path::new(args.get(1).ok_or("polygon reference needs SCENE OUTPUT")?);
    let output = Path::new(args.get(2).ok_or("polygon reference needs SCENE OUTPUT")?);
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let root = source_root(&root)?;
    validate_source(&root)?;
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("STARTUP_UI_RAYLIB_ROOT")
                .map(|p| PathBuf::from(p).join("lib/libopenpilot-raylib.so"))
        })
        .unwrap_or_else(|| PathBuf::from("libopenpilot-raylib.so"));
    let mut api = Raylib {
        api: openpilot_startup_ui::polygon_reference_abi::RawApi::new(&library)?,
        warning_source: root.join("rust/tools/ui_qa/polygon_source.py"),
        warnings: warning_action(std::env::var_os("PYTHONWARNINGS").as_deref()),
    };
    let result = render(scene, output, &mut api, &mut std::io::stdout().lock());
    if result.is_err() {
        // The original exits without explicit cleanup on scene errors. This
        // isolated reference process relinquishes its resources at process exit.
        std::mem::forget(api);
    }
    result
}
