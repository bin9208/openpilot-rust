//! Dynamic independent polygon reference and candidate raster/state comparison.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    image_array, image_array_math,
    path_resolve::resolve_non_strict,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::JsonValue;
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
const SCENES: [&str; 7] = [
    "odd-gradient",
    "solid",
    "color",
    "clipped-stops",
    "empty-gradient",
    "capped-gradient",
    "empty",
];
fn validate_checker(root: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let original = JsonValue::parse(include_str!(
        "../tests/fixtures/ui-polygon/checker-original.json"
    ))?;
    let source = root.join("rust/tools/check_ui_polygon.py");
    const SHA: &str = "d794d8c8a4c1dcf9a11fb2139872a9b5a2ebabed284c17f017bc530ef2755478";
    if !original
        .get("script_sha256")
        .is_some_and(|v| v.text_eq(SHA))
    {
        return Err("polygon checker provenance changed".into());
    }
    match fs::read(source) {
        Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != SHA => {
            return Err("reintroduced polygon checker changed".into());
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        _ => {}
    }
    Ok(())
}
fn parse(text: &str) -> Result<JsonValue> {
    let value = JsonValue::parse(text)?;
    // Same original Python3.12 decoder boundary as the independent renderer.
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
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
                    if depth > 9997 {
                        return Err("source JSON recursion limit exceeded".into());
                    }
                }
                b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
    }
    Ok(value)
}
fn lexical(path: &Path) -> PathBuf {
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
        .filter(|v| !v.is_empty() && *v != b".")
    {
        if !result.is_empty() && !result.ends_with(b"/") {
            result.push(b'/');
        }
        result.extend_from_slice(part);
    }
    if result.is_empty() {
        result.push(b'.');
    }
    OsString::from_vec(result).into()
}
fn input(name: &str) -> Result<String> {
    let text = match name {
        "odd-gradient" => include_str!("../tests/fixtures/ui-polygon/input/odd-gradient.json"),
        "solid" => include_str!("../tests/fixtures/ui-polygon/input/solid.json"),
        "color" => include_str!("../tests/fixtures/ui-polygon/input/color.json"),
        "clipped-stops" => include_str!("../tests/fixtures/ui-polygon/input/clipped-stops.json"),
        "empty-gradient" => include_str!("../tests/fixtures/ui-polygon/input/empty-gradient.json"),
        "capped-gradient" => {
            include_str!("../tests/fixtures/ui-polygon/input/capped-gradient.json")
        }
        "empty" => include_str!("../tests/fixtures/ui-polygon/input/empty.json"),
        _ => return Err("unknown polygon scene".into()),
    };
    Ok(parse(text)?.to_json()?)
}
/// Preserve the original seven live scenes, raw decoded-array arithmetic,
/// Python JSON equality and diagnostic/partial-artifact behavior.
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    display: &OsStr,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    let output = lexical(output);
    fs::create_dir_all(&output)?;
    let root = resolve_non_strict(root)?;
    validate_checker(&root)?;
    let mut results = vec![];
    let mut accepted = true;
    for name in SCENES {
        let scene = output.join(format!("{name}.json"));
        fs::write(&scene, input(name)?)?;
        let mut images = vec![];
        let mut states = vec![];
        for (lane, executable) in [("source", reference), ("native", binary)] {
            let image = output.join(format!("{lane}-{name}.png"));
            let mut argv = vec![lexical(executable).into_os_string()];
            if lane == "native" {
                argv.push(root.clone().into_os_string());
            }
            argv.push(scene.clone().into_os_string());
            argv.push(image.clone().into_os_string());
            let mut spec = CommandSpec::new_os(argv, Duration::MAX);
            spec.inherit_stdin = true;
            spec.stdout_limit = usize::MAX;
            spec.stderr_limit = usize::MAX;
            let mut env = BTreeMap::new();
            env.insert(OsString::from("DISPLAY"), display.to_owned());
            env.insert(OsString::from("OFFSCREEN"), "1".into());
            env.insert(OsString::from("PYTHONPATH"), root.as_os_str().to_owned());
            spec.set_os_env(env);
            let result = runner.run_to_file(&spec, &output.join(format!("{lane}-{name}.log")))?;
            if result.code != 0 {
                return Err(format!("polygon {name}/{lane} exited {}", result.code).into());
            }
            images.push(image_array::read(&image)?);
            let text = String::from_utf8(fs::read(image.with_extension("json"))?)?
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            states.push(parse(&text)?);
        }
        let difference = image_array_math::compare(
            &images[0].shape,
            &images[0].values,
            &images[1].shape,
            &images[1].values,
        )?;
        let state_equal = json::equal(&states[0], &states[1]);
        let row=format!("{{\"scene\": \"{name}\", \"different_pixels\": {}, \"max_channel_difference\": {}, \"state_equal\": {state_equal}}}",difference.different_pixels,difference.max_channel_difference);
        printed.write_all(row.as_bytes())?;
        printed.write_all(b"\n")?;
        printed.flush()?;
        accepted &= difference.different_pixels == 0 && state_equal;
        results.push(row);
    }
    let value = parse(&format!("[{}]", results.join(", ")))?;
    fs::write(
        output.join("results.json"),
        json::pretty_with_limit(&value, 0, 996)?,
    )?;
    if !accepted {
        return Err("source/native polygon pixels or states differ".into());
    }
    printed.write_all(
        b"PASS: 7 actual-source/native polygon shader, gradient and triangulation scenes\n",
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
        ],
    )?;
    if args.help {
        println!("usage: check-ui-polygon --binary BINARY --output OUTPUT --display DISPLAY");
        return Ok(());
    }
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let reference = std::env::current_exe()?
        .parent()
        .ok_or("polygon checker has no executable parent")?
        .join("ui-polygon-reference");
    check(
        &root,
        Path::new(&args.values["--binary"]),
        &reference,
        Path::new(&args.values["--output"]),
        &args.values["--display"],
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
