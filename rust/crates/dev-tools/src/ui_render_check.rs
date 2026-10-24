//! Shared UI render comparator and process contract, independently captured.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    control_tools_policy_check::json,
    image_array, image_array_math,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ffi::OsStr, fs, io::Write, path::Path, time::Duration};
const SCENES: &str = include_str!("../tests/fixtures/ui-render/scenes.json");
const SCENES_SHA: &str = "26748c74257ad0ba55f9e54a2656268dc8cb9da120799e93117b34bf403f252e";
pub fn validate_source(root: &Path) -> Result<()> {
    for (name, hash) in [
        (
            "rust/tools/check_ui_render.py",
            "b47f1d18f069aa4e57952b162947e4ca03f2ceff25f1158872a21df142b83501",
        ),
        (
            "rust/tools/ui_framework_source_render.py",
            "257dfe56f49f0c2ae1d007305126e69ab858d14ccf8e586cfeea30d29431be0c",
        ),
    ] {
        match fs::read(root.join(name)) {
            Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != hash => {
                return Err(format!("{name} changed; independently recapture render policy").into())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    if format!("{:x}", Sha256::digest(SCENES.as_bytes())) != SCENES_SHA {
        return Err("render scene inputs changed; independently recapture".into());
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn check(
    root: &Path,
    binary: &Path,
    reference: &Path,
    output: &Path,
    display: &OsStr,
    runner: &mut dyn Runner,
    printed: &mut dyn Write,
) -> Result<()> {
    fs::create_dir_all(output)?;
    validate_source(root)?;
    let scenes = JsonValue::parse(SCENES)?;
    let JsonView::Object(scenes) = scenes.view() else {
        return Err("render scene list missing".into());
    };
    let mut results = Vec::new();
    let mut accepted = true;
    for (name, scene) in scenes {
        let name = name
            .iter()
            .map(|v| char::from_u32(*v).unwrap())
            .collect::<String>();
        let path = output.join(format!("{name}.json"));
        fs::write(&path, scene.to_json_utf8()?)?;
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
                return Err(format!("{lane} render {name} exited {}", result.code).into());
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
        let equal = json::equal(&states[0], &states[1]);
        let row=format!("{{\"scene\": {}, \"different_pixels\": {}, \"max_channel_difference\": {}, \"state_equal\": {equal}}}",JsonValue::text(&name).to_json()?,delta.different_pixels,delta.max_channel_difference);
        writeln!(printed, "{row}")?;
        printed.flush()?;
        accepted &= delta.different_pixels == 0 && equal;
        results.push(row);
    }
    fs::write(
        output.join("render-results.json"),
        json::pretty_with_limit(
            &JsonValue::parse(&format!("[{}]", results.join(", ")))?,
            0,
            996,
        )?,
    )?;
    if !accepted {
        return Err("source/native shared UI render differs".into());
    }
    writeln!(
        printed,
        "PASS: six actual-source/native shared widget scenes and state snapshots"
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
        println!("usage: check-ui-render --binary BINARY --output OUTPUT --display DISPLAY");
        return Ok(());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    let current = std::env::current_exe()?;
    let reference = current
        .parent()
        .ok_or("render executable parent missing")?
        .join("ui-render-reference");
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
