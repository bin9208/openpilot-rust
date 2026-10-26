use super::*;
use serde_json::json;
pub struct GraphicsOptions {
    pub source_root: PathBuf,
    pub cc: PathBuf,
    pub qemu: PathBuf,
    pub loader: PathBuf,
    pub plugin: PathBuf,
    pub sdk: PathBuf,
    pub agnos_libraries: PathBuf,
    pub output: PathBuf,
}
pub fn graphics(options: &GraphicsOptions, runner: &mut impl Runner) -> Result<Value> {
    graphics_with_space(options, runner, &mut crate::disk_space::inspect)
}
pub(super) fn graphics_with_space(
    options: &GraphicsOptions,
    runner: &mut impl Runner,
    inspect: &mut impl FnMut(&Path, u64) -> Result<crate::disk_space::Space>,
) -> Result<Value> {
    inspect(&options.output, 1024 * 1024 * 1024)?.require()?;
    fs::create_dir_all(&options.output)?;
    let probe = options.output.join("library-probe.elf");
    let command = build_probe(
        &options.cc,
        &fixture("runtime_library_probe.c"),
        &probe,
        &[],
        runner,
        inspect,
    )?;
    let source = options.source_root.join("rust/crates/startup-ui/native");
    let raylib = fs::read_to_string(source.join("raylib_loader.cc"))?;
    let gles = fs::read_to_string(source.join("gles.cc"))?;
    let egl = fs::read_to_string(source.join("egl.cc"))?;
    let required = raylib
        .split_once("const char *required[] =")
        .ok_or("raylib required symbol list absent")?
        .1
        .split_once("};")
        .ok_or("raylib required symbol list unterminated")?
        .0;
    let mut raylib_names = symbols(required, r#""(\w+)""#)?;
    raylib_names.extend(symbols(&raylib, r"(?m)^FORWARD\([^,]+,\s*(\w+),")?);
    let mut gles_names = symbols(&gles, r#"LOAD\(\w+,\s*"(\w+)"\)"#)?;
    gles_names.extend(symbols(&egl, r#"gles.symbol<.*?>\("(\w+)"\)"#)?);
    let egl_names = symbols(&egl, r#"egl.symbol<.*?>\("(\w+)"\)"#)?;
    let extensions = symbols(&egl, r#"extension<.*?>\("(\w+)""#)?;
    require(
        [&raylib_names, &gles_names, &egl_names, &extensions]
            .iter()
            .all(|s| !s.is_empty()),
        "missing current graphics source symbols",
    )?;
    let libraries = vec![
        options
            .plugin
            .parent()
            .ok_or("plugin parent absent")?
            .into(),
        options.agnos_libraries.clone(),
        options.sdk.join("usr/lib/aarch64-linux-gnu"),
    ];
    let mut value =
        crate::elf_audit::audit(&[probe.clone(), options.plugin.clone()], &libraries, 183)?;
    value["build_command"] = json!(command);
    value["sources"] = json!(["raylib_loader.cc", "gles.cc", "egl.cc"]
        .into_iter()
        .map(|name| Ok((name, digest(&source.join(name))?)))
        .collect::<Result<BTreeMap<_, _>>>()?);
    value["runs"] = json!([]);
    report(&options.output, &value)?;
    require(value["compatible"] == true, "graphics ABI closure failed")?;
    let egl_library = libraries[2].join("libEGL.so");
    let gles_library = libraries[2].join("libGLESv2.so");
    let mut contract = vec!["comma-deps-raylib-6.0.0.1.post103".into()];
    contract.extend(raylib_names);
    for (name, library, mode, names) in [
        ("raylib", options.plugin.clone(), "contract", contract),
        (
            "egl",
            egl_library.clone(),
            "symbols",
            egl_names.into_iter().collect(),
        ),
        (
            "gles",
            gles_library,
            "symbols",
            gles_names.into_iter().collect(),
        ),
        (
            "extensions",
            egl_library,
            "extensions",
            extensions.into_iter().collect(),
        ),
    ] {
        let mut command = provider_prefix(&options.qemu, &options.loader, &libraries)?;
        command.extend([text(&probe)?, text(&library)?, mode.into()]);
        command.extend(names);
        let result = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(30)))?;
        fs::write(
            options.output.join(format!("{name}.stdout")),
            stdout_text(&result)?,
        )?;
        fs::write(
            options.output.join(format!("{name}.stderr")),
            stderr_text(&result)?,
        )?;
        value["runs"].as_array_mut().ok_or("missing runs")?.push(json!({"name":name,"command":command,"exit_code":result.code,"output":stdout_text(&result)?,"error":stderr_text(&result)?}));
        report(&options.output, &value)?;
        checked(&result, name)?;
    }
    value["passed"] = true.into();
    value["execution_scope"]="AArch64 AGNOS dlopen, raylib contract and current symbol/extension lookup; no display/GPU initialization".into();
    report(&options.output, &value)?;
    Ok(
        json!({"objects":value["objects"].as_array().ok_or("missing objects")?.len(),"runs":value["runs"].as_array().ok_or("missing runs")?.iter().map(|v|json!({"name":v["name"],"output":v["output"]})).collect::<Vec<_>>()}),
    )
}
