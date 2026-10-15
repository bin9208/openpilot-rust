use super::*;
use serde_json::json;
pub struct DynamicOptions {
    pub root: PathBuf,
    pub sdk: PathBuf,
    pub agnos: PathBuf,
    pub qemu: PathBuf,
    pub probe: PathBuf,
    pub output: PathBuf,
}
pub fn dynamic(options: &DynamicOptions, runner: &mut impl Runner) -> Result<Value> {
    fs::create_dir_all(&options.output)?;
    let libraries = vec![
        options.sdk.join("usr/lib/aarch64-linux-gnu"),
        options.agnos.clone(),
    ];
    let specifications: &[(&str, &[&str])] = &[
        ("libportaudio.so.2", &["portaudio/src/native.rs"]),
        (
            "libusb-1.0.so.0",
            &["panda-usb/src/api.rs", "usbgpu/src/native_usb/api.rs"],
        ),
    ];
    let roots: Vec<_> = specifications
        .iter()
        .map(|(name, _)| libraries[0].join(name))
        .collect();
    let mut value = crate::elf_audit::audit(&roots, &libraries, 183)?;
    value["runs"] = json!([]);
    report(&options.output, &value)?;
    for ((name, sources), library) in specifications.iter().zip(roots) {
        let mut names = BTreeSet::new();
        let mut hashes = BTreeMap::new();
        for source in *sources {
            let path = options.root.join("rust/crates").join(source);
            names.extend(symbols(
                &fs::read_to_string(&path)?,
                r#"b"([A-Za-z][A-Za-z0-9_]*)\\0""#,
            )?);
            hashes.insert(*source, digest(&path)?);
        }
        require(
            !names.is_empty(),
            format!("no current source symbols for {name}"),
        )?;
        let mut command = provider_prefix(
            &options.qemu,
            &options.agnos.join("ld-linux-aarch64.so.1"),
            &libraries,
        )?;
        command.extend([text(&options.probe)?, text(&library)?, "symbols".into()]);
        command.extend(names.iter().cloned());
        let result = runner.run(&CommandSpec::new(command.clone(), Duration::from_secs(30)))?;
        value["runs"].as_array_mut().ok_or("missing native runs")?.push(json!({"name":name,"sources":hashes,"symbols":names,"command":command,"exit_code":result.code,"stdout":stdout_text(&result)?,"stderr":stderr_text(&result)?}));
        report(&options.output, &value)?;
    }
    value["passed"] = json!(
        value["compatible"] == true
            && value["runs"]
                .as_array()
                .ok_or("missing runs")?
                .iter()
                .all(|v| v["exit_code"] == 0)
    );
    value["execution_scope"]="AGNOS native provider dlopen/dlsym with QEMU; no USB, audio stream or network initialization".into();
    report(&options.output, &value)?;
    require(
        value["passed"] == true,
        "native dynamic provider validation failed",
    )?;
    Ok(
        json!({"passed":true,"objects":value["objects"].as_array().ok_or("missing objects")?.len(),"symbols":value["runs"].as_array().ok_or("missing runs")?.iter().map(|r|Ok((r["name"].as_str().ok_or("missing provider name")?.to_owned(),r["symbols"].as_array().ok_or("missing symbols")?.len()))).collect::<Result<BTreeMap<_,_>>>()?}),
    )
}
