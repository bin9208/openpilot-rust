//! Full forms comparator and fixed scene selection, independently captured from
//! the original checker. Its native reference remains a separate dependency.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    image_array, image_array_math,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::Path,
    time::Duration,
};
const SCENES: &str = include_str!("../tests/fixtures/ui-forms/scenes.json");
const CHECKER: &str = "rust/tools/check_ui_forms.py";
const HELPER: &str = "rust/tools/ui_forms_source.py";
const CHECKER_SHA: &str = "a681ddf7f1abf25219d5e89c113c624733dcbf6d08459e20e2382c6dbd2d0900";
const HELPER_SHA: &str = "2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365";
const SCENES_SHA: &str = "41475105d918bc376bddb132a0fa14ad75421f57953316392763787508685d99";
pub fn validate_source(root: &Path) -> Result<()> {
    for (name, hash) in [(CHECKER, CHECKER_SHA), (HELPER, HELPER_SHA)] {
        match fs::read(root.join(name)) {
            Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != hash => {
                return Err(format!("{name} changed; recapture forms comparator policy").into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if format!("{:x}", Sha256::digest(SCENES.as_bytes())) != SCENES_SHA {
        return Err("forms scene list changed; independently recapture".into());
    }
    Ok(())
}
fn repr_text(points: &[u32]) -> String {
    static NONPRINTABLE: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let nonprintable = NONPRINTABLE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../tests/fixtures/ui-forms/repr-nonprintable.json"
        ))
        .expect("captured Python Unicode table")
    });
    let quote = if points.contains(&39) && !points.contains(&34) {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for &point in points {
        if point == u32::from(quote) || point == 92 {
            out.push('\\');
            out.push(char::from_u32(point).unwrap());
        } else {
            match point {
                9 => out.push_str("\\t"),
                10 => out.push_str("\\n"),
                13 => out.push_str("\\r"),
                32..=126 => out.push(char::from_u32(point).unwrap()),
                _ if nonprintable
                    .iter()
                    .any(|&[lo, hi]| (lo..=hi).contains(&point)) =>
                {
                    if point <= 255 {
                        out.push_str(&format!("\\x{point:02x}"));
                    } else if point <= 0xffff {
                        out.push_str(&format!("\\u{point:04x}"));
                    } else {
                        out.push_str(&format!("\\U{point:08x}"));
                    }
                }
                _ => out.push(char::from_u32(point).unwrap_or('\u{fffd}')),
            }
        }
    }
    out.push(quote);
    out
}
pub(crate) fn repr(value: &JsonValue) -> String {
    enum Part {
        Value(JsonValue),
        Text(String),
    }
    let mut out = String::new();
    let mut pending = vec![Part::Value(value.clone())];
    while let Some(part) = pending.pop() {
        let Part::Value(value) = part else {
            if let Part::Text(text) = part {
                out.push_str(&text);
            }
            continue;
        };
        match value.view() {
            JsonView::Null => out.push_str("None"),
            JsonView::Bool(v) => out.push_str(if v { "True" } else { "False" }),
            JsonView::Integer(v) => out.push_str(v),
            JsonView::Float(v) if v.is_nan() => out.push_str("nan"),
            JsonView::Float(v) if v.is_infinite() => {
                out.push_str(if v > 0.0 { "inf" } else { "-inf" });
            }
            JsonView::Float(_) => out.push_str(&value.to_json().unwrap()),
            JsonView::Text(v) => out.push_str(&repr_text(v)),
            JsonView::Array(values) => {
                out.push('[');
                pending.push(Part::Text("]".into()));
                for (index, value) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    if index != 0 {
                        pending.push(Part::Text(", ".into()));
                    }
                }
            }
            JsonView::Object(values) => {
                out.push('{');
                pending.push(Part::Text("}".into()));
                for (index, (key, value)) in values.into_iter().enumerate().rev() {
                    pending.push(Part::Value(value));
                    pending.push(Part::Text(format!("{}: ", repr_text(key))));
                    if index != 0 {
                        pending.push(Part::Text(", ".into()));
                    }
                }
            }
        }
    }
    out
}

fn numeric(v: &JsonValue) -> Option<std::result::Result<f64, std::num::ParseFloatError>> {
    match v.view() {
        JsonView::Integer(v) => Some(v.parse()),
        JsonView::Float(v) => Some(Ok(v)),
        JsonView::Bool(v) => Some(Ok(if v { 1.0 } else { 0.0 })),
        _ => None,
    }
}
fn integer(v: &JsonValue) -> Option<String> {
    match v.view() {
        JsonView::Integer(v) => Some(if v == "-0" { "0" } else { v }.into()),
        JsonView::Bool(v) => Some(if v { "1" } else { "0" }.into()),
        _ => None,
    }
}
/// The original recurses in source dictionary order and returns the first
/// assertion's tuple representation. Arithmetic errors remain fatal errors.
pub fn state_difference(a: &JsonValue, b: &JsonValue) -> Result<Option<String>> {
    let mut pending = vec![(a.clone(), b.clone(), vec![114, 111, 111, 116], 0usize)];
    while let Some((a, b, at, depth)) = pending.pop() {
        if depth > 990 {
            return Err("forms state comparison recursion limit".into());
        }
        let fail =
            |left: String, right: String| Some(format!("({}, {left}, {right})", repr_text(&at)));
        match (a.view(), b.view()) {
            (JsonView::Object(left), JsonView::Object(right)) => {
                if left.len() != right.len()
                    || left
                        .iter()
                        .any(|(k, _)| !right.iter().any(|(other, _)| other == k))
                {
                    let keys = |v: &Vec<(&[u32], JsonValue)>| {
                        format!(
                            "dict_keys([{}])",
                            v.iter()
                                .map(|(k, _)| repr_text(k))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    return Ok(fail(keys(&left), keys(&right)));
                }
                for (key, value) in left.into_iter().rev() {
                    let other = right.iter().find(|(k, _)| *k == key).unwrap().1.clone();
                    let mut path = at.clone();
                    path.push(46);
                    path.extend(key);
                    pending.push((value, other, path, depth + 1));
                }
            }
            (JsonView::Array(left), JsonView::Array(right)) => {
                if left.len() != right.len() {
                    return Ok(fail(left.len().to_string(), right.len().to_string()));
                }
                for (i, (left, right)) in left.into_iter().zip(right).enumerate().rev() {
                    let mut path = at.clone();
                    path.extend(format!("[{i}]").chars().map(u32::from));
                    pending.push((left, right, path, depth + 1));
                }
            }
            _ => {
                let equal = if let (Some(left), Some(right)) = (integer(&a), integer(&b)) {
                    left == right
                } else if let (Some(left), Some(right)) = (numeric(&a), numeric(&b)) {
                    let left = left?;
                    let right = right?;
                    if matches!(a.view(), JsonView::Integer(_)) && !left.is_finite()
                        || matches!(b.view(), JsonView::Integer(_)) && !right.is_finite()
                    {
                        return Err("forms state integer too large to convert to float".into());
                    }
                    (left - right).abs() <= 2e-5
                } else {
                    json::equal(&a, &b)
                };
                if !equal {
                    return Ok(fail(repr(&a), repr(&b)));
                }
            }
        }
    }
    Ok(None)
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    display: &OsStr,
    only: Option<&OsStr>,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    fs::create_dir_all(output)?;
    validate_source(root)?;
    let scenes = JsonValue::parse(SCENES)?;
    let JsonView::Object(scenes) = scenes.view() else {
        return Err("forms scene list missing".into());
    };
    let mut results = Vec::new();
    let mut accepted = true;
    let mut frames = 0u64;
    for (name, scene) in scenes {
        let name = name
            .iter()
            .map(|v| char::from_u32(*v).unwrap())
            .collect::<String>();
        if let Some(only) = only.filter(|v| !v.is_empty()) {
            if !only
                .as_encoded_bytes()
                .split(|b| *b == b',')
                .any(|n| n == name.as_bytes())
            {
                continue;
            }
        }
        let frame_count = scene
            .get("frames")
            .ok_or("missing forms frames")?
            .to_json()?
            .parse::<u64>()?;
        let path = output.join(format!("{name}.json"));
        fs::write(&path, scene.to_json()?)?;
        let mut images = vec![];
        let mut states = vec![];
        for (lane, exe) in [("source", reference), ("native", binary)] {
            let image = output.join(format!("{lane}-{name}.png"));
            let mut args = vec![exe.as_os_str().to_owned()];
            if lane == "native" {
                args.push(root.as_os_str().to_owned());
            }
            args.extend([path.as_os_str().to_owned(), image.as_os_str().to_owned()]);
            let mut spec = CommandSpec::new_os(args, Duration::MAX);
            spec.inherit_stdin = true;
            spec.stdout_limit = usize::MAX;
            spec.stderr_limit = usize::MAX;
            spec.set_os_env(BTreeMap::from([
                ("DISPLAY".into(), display.to_owned()),
                ("OFFSCREEN".into(), "1".into()),
                ("PYTHONPATH".into(), root.as_os_str().to_owned()),
                ("PARAMS_ROOT".into(), output.join("params").into_os_string()),
            ]));
            let result = runner.run_to_file(&spec, &output.join(format!("{lane}-{name}.log")))?;
            if result.code != 0 {
                return Err(format!("{lane} forms {name} exited {}", result.code).into());
            }
            images.push(image_array::read(&image)?);
            states.push(json::parse_text(&fs::read_to_string(
                image.with_extension("json"),
            )?)?);
        }
        let delta = image_array_math::compare(
            &images[0].shape,
            &images[0].values,
            &images[1].shape,
            &images[1].values,
        )?;
        let state_error = state_difference(&states[0], &states[1])?;
        let equal = state_error.is_none();
        let mut row=format!("{{\"scene\": {}, \"frames\": {frame_count}, \"different_pixels\": {}, \"max_channel_difference\": {}, \"state_equal\": {equal}",JsonValue::text(&name).to_json()?,delta.different_pixels,delta.max_channel_difference);
        if let Some(error) = state_error {
            row.push_str(&format!(
                ", \"state_error\": {}",
                JsonValue::text(&error).to_json()?
            ));
        }
        row.push('}');
        writeln!(printed, "{row}")?;
        printed.flush()?;
        accepted &= delta.different_pixels == 0 && equal;
        frames += frame_count;
        results.push(row);
    }
    fs::write(
        output.join("results.json"),
        json::pretty_with_limit(
            &JsonValue::parse(&format!("[{}]", results.join(", ")))?,
            0,
            996,
        )?,
    )?;
    if !accepted {
        return Err("source/native forms differ".into());
    }
    writeln!(
        printed,
        "PASS: {} actual-source/native form scenes; {frames} input/render frames",
        results.len()
    )?;
    Ok(())
}
pub fn cli() -> Result<()> {
    let args = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--binary", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--display", true),
            OptionSpec::value("--only", false),
        ],
    )?;
    if args.help {
        println!("usage: check-ui-forms --binary BINARY --output OUTPUT --display DISPLAY [--only SCENES]");
        return Ok(());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    let current = std::env::current_exe()?;
    let reference = current
        .parent()
        .ok_or("forms executable parent missing")?
        .join("ui-forms-reference");
    check(
        &root,
        Path::new(&args.values["--binary"]),
        &reference,
        Path::new(&args.values["--output"]),
        &args.values["--display"],
        args.values.get("--only").map(OsString::as_os_str),
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
