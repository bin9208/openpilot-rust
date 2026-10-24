//! Network UI comparator and process contract, independently captured.
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
const SCENES: &str = include_str!("../tests/fixtures/ui-network/scenes.json");
const SCENES_SHA: &str = "2d31033f448d0d840a9c67c11e37b577be4827b6ba11411cb4642086f5feb13a";
pub fn validate_source(root: &Path) -> Result<()> {
    for (name, hash) in [
        (
            "rust/tools/check_ui_network.py",
            "f6225784cc275e6c69ecd1b58f7a15ccec42bbea261285d53c24b0802e53f379",
        ),
        (
            "rust/tools/ui_network_source.py",
            "00fb81b7a01eb5b515d9af98c03a7a5a1b7ab25f52f9ffba4d335034e03163b9",
        ),
    ] {
        match fs::read(root.join(name)) {
            Ok(bytes) if format!("{:x}", Sha256::digest(&bytes)) != hash => {
                return Err(
                    format!("{name} changed; independently recapture network policy").into(),
                )
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
    }
    if format!("{:x}", Sha256::digest(SCENES.as_bytes())) != SCENES_SHA {
        return Err("network scene inputs changed; independently recapture".into());
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
        return Err("network scene list missing".into());
    };
    let mut results = Vec::new();
    let mut accepted = true;
    for (name, scene) in scenes {
        let name = name
            .iter()
            .map(|v| char::from_u32(*v).unwrap())
            .collect::<String>();
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
            ]));
            let result = runner.run_to_file(&spec, &output.join(format!("{lane}-{name}.log")))?;
            if result.code != 0 {
                return Err(format!("{lane} network {name} exited {}", result.code).into());
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
        let frames =
            json::sequence(&scene.get("frames").ok_or("network frame list missing")?)?.len();
        let row=format!("{{\"scene\": {}, \"frames\": {frames}, \"different_pixels\": {}, \"max_channel_difference\": {}, \"state_equal\": {equal}}}",JsonValue::text(&name).to_json()?,delta.different_pixels,delta.max_channel_difference);
        writeln!(printed, "{row}")?;
        printed.flush()?;
        if !equal {
            let source = json::sequence(&states[0])?;
            let native = json::sequence(&states[1])?;
            for index in 0..source.len().max(native.len()) {
                let (Some(source), Some(native)) = (source.get(index), native.get(index)) else {
                    return Err("zip() arguments have different lengths".into());
                };
                if !json::equal(source, native) {
                    writeln!(
                        printed,
                        "{{\"frame\": {index}, \"source\": {}, \"native\": {}}}",
                        source.to_json()?,
                        native.to_json()?
                    )?;
                    printed.flush()?;
                    break;
                }
            }
        }
        accepted &= delta.different_pixels == 0 && equal;
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
        return Err("source/native network screens differ".into());
    }
    writeln!(
        printed,
        "PASS: {} source/native network screens",
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
        ],
    )?;
    if args.help {
        println!("usage: check-ui-network --binary BINARY --output OUTPUT --display DISPLAY");
        return Ok(());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    let current = std::env::current_exe()?;
    let reference = current
        .parent()
        .ok_or("network executable parent missing")?
        .join("ui-network-reference");
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
