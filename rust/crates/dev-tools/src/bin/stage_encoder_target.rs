use openpilot_dev_tools::encoder_stage::{stage, NativeRunner, Options, Toolchain};
use std::{collections::BTreeMap, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut values = BTreeMap::new();
    let mut prepare_wheels = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let key = arg.to_str().ok_or("invalid argument")?;
        match key {
            "--prepare-wheels" if !prepare_wheels => prepare_wheels = true,
            "--output"
            | "--root"
            | "--archive-directory"
            | "--swscale-archive"
            | "--cc"
            | "--ar"
            | "--ranlib" => {
                let value = args.next().ok_or("missing argument value")?;
                if values.insert(key.to_owned(), value).is_some() {
                    return Err("duplicate argument".into());
                }
            }
            "--cache" | "--store" => {
                let value = args.next().ok_or("missing argument value")?;
                if values.insert(key.to_owned(), value).is_some() {
                    return Err("duplicate argument".into());
                }
            }
            "--help" | "-h" => {
                println!("Usage: stage-encoder-target --output NEW_DIRECTORY [--root ROOT] [--archive-directory DIRECTORY] [--swscale-archive FILE] [--cc CC --ar AR --ranlib RANLIB]\n       stage-encoder-target --prepare-wheels --cache ASSET_CACHE --store PREFIX_STORE [--root ROOT]\nPrepare only unchanged pinned ARM wheel inputs; this mode does not build libswscale or activate a runtime.");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {key}").into()),
        }
    }
    if prepare_wheels {
        if values
            .keys()
            .any(|key| !["--root", "--cache", "--store"].contains(&key.as_str()))
        {
            return Err(
                "wheel preparation cannot be combined with legacy staging/build arguments".into(),
            );
        }
        let root = values.remove("--root").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        );
        let cache = PathBuf::from(values.remove("--cache").ok_or("--cache required")?);
        let store = PathBuf::from(values.remove("--store").ok_or("--store required")?);
        let report = openpilot_dev_tools::native_codec_inputs::prepare(
            &root,
            &cache,
            &store,
            &openpilot_runtime_build::assets::Curl,
        )?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    if values.contains_key("--cache") || values.contains_key("--store") {
        return Err("--cache and --store require --prepare-wheels".into());
    }
    let mut tools = Toolchain::default();
    for (key, target) in [
        ("--cc", &mut tools.cc),
        ("--ar", &mut tools.ar),
        ("--ranlib", &mut tools.ranlib),
    ] {
        if let Some(value) = values.remove(key) {
            *target = value
                .into_string()
                .map_err(|_| "non-UTF8 toolchain command")?;
        }
    }
    let options = Options {
        root: values.remove("--root").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
            PathBuf::from,
        ),
        output: values.remove("--output").ok_or("--output required")?.into(),
        archive_directory: values.remove("--archive-directory").map(PathBuf::from),
        swscale_archive: values.remove("--swscale-archive").map(PathBuf::from),
        tools,
    };
    let result = stage(&options, &mut NativeRunner)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("encoder target staging: {e}");
        std::process::exit(1);
    }
}
