//! Dynamic source/native diagnostic raster gate; source policy lives in the
//! independent native reference and all admitted image arrays stay unnormalized.
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
    path::Path,
    time::Duration,
};
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
    crate::ui_diagnostic_reference::validate_source(root)?;
    let mut results = vec![];
    let mut accepted = true;
    for mode in ["burn-in", "grid", "touches"] {
        let mut images = vec![];
        for (lane, exe) in [("source", reference), ("native", binary)] {
            let image = output.join(format!("{lane}-{mode}.png"));
            let mut argv = vec![exe.as_os_str().to_owned()];
            if lane == "native" {
                argv.push(root.as_os_str().to_owned());
            }
            argv.extend([OsString::from(mode), image.as_os_str().to_owned()]);
            let mut spec = CommandSpec::new_os(argv, Duration::MAX);
            spec.inherit_stdin = true;
            spec.stdout_limit = usize::MAX;
            spec.stderr_limit = usize::MAX;
            spec.set_os_env(BTreeMap::from([
                ("DISPLAY".into(), display.to_owned()),
                ("OFFSCREEN".into(), "1".into()),
                ("PYTHONPATH".into(), root.as_os_str().to_owned()),
            ]));
            let result = runner.run_to_file(&spec, &output.join(format!("{lane}-{mode}.log")))?;
            if result.code != 0 {
                return Err(
                    format!("{lane} diagnostic {mode} failed with exit {}", result.code).into(),
                );
            }
            images.push(image_array::read(&image)?);
        }
        let difference = image_array_math::compare(
            &images[0].shape,
            &images[0].values,
            &images[1].shape,
            &images[1].values,
        )?;
        let row = format!(
            "{{\"mode\": \"{mode}\", \"different_pixels\": {}, \"max_channel_difference\": {}}}",
            difference.different_pixels, difference.max_channel_difference
        );
        writeln!(printed, "{row}")?;
        printed.flush()?;
        accepted &= difference.different_pixels == 0;
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
        return Err("source/native diagnostic pixels differ".into());
    }
    writeln!(
        printed,
        "PASS: unchanged-source/native burn-in shader, grid and touch trail final-frame pixels"
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
        println!("usage: check-ui-diagnostics --binary BINARY --output OUTPUT --display DISPLAY");
        return Ok(());
    }
    let root = crate::ui_diagnostic_reference::source_root()?;
    let executable = std::env::current_exe()?;
    let reference = executable
        .parent()
        .ok_or("diagnostic executable parent missing")?
        .join("ui-diagnostic-reference");
    check(
        &resolve_non_strict(&root)?,
        Path::new(&args.values["--binary"]),
        &reference,
        Path::new(&args.values["--output"]),
        &args.values["--display"],
        &mut NativeRunner,
        &mut std::io::stdout().lock(),
    )
}
