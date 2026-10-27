#[cfg(feature = "prepare-rust")]
use openpilot_runtime_build::toolchain;
use openpilot_runtime_build::{inputs::Tree, Result};
use std::{fs::File, io::Write, path::PathBuf};

fn run() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut root = None;
    let mut output = None;
    let (mut plan, mut cache, mut store) = (None, None, None);
    let (mut prepare_rust, mut prepare_capnp, mut prepare_cargo) = (false, false, false);
    let mut capture_system = false;
    let mut prepare_source_assets = false;
    let mut prepare_python = false;
    let mut prepare_assets = false;
    let mut verify_models = false;
    let mut objects = None;
    let mut lock = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--verify-models") if !verify_models => verify_models = true,
            Some("--help" | "-h") => {
                println!("Usage: openpilot-runtime-inputs --root INPUT_TREE --output RECEIPT_JSON\n       openpilot-runtime-inputs --capture-system-sdk --root VERIFIED_IMAGE_ROOT --plan SDK_ROOTS_JSON --output SDK_RECEIPT\n       openpilot-runtime-inputs --prepare-rust --plan PINNED_RUST_JSON --cache ASSET_CACHE --store TOOLCHAIN_STORE\n       openpilot-runtime-inputs --prepare-capnp --plan PINNED_CAPNP_ASSETS_JSON --cache ASSET_CACHE --store SDK_STORE\n       openpilot-runtime-inputs --prepare-cargo --plan PINNED_CRATES_JSON --lock CARGO_LOCK --cache ASSET_CACHE --store SOURCES_STORE\n       openpilot-runtime-inputs --prepare-source-lfs --root CLEAN_SOURCE --plan SOURCE_LFS_JSON --cache ASSET_CACHE --store PREFIX_STORE --objects OBJECT_STORE\n       openpilot-runtime-inputs --prepare-python --plan PINNED_PYTHON_JSON --cache ASSET_CACHE --store TOOLCHAIN_STORE\n       openpilot-runtime-inputs --prepare-assets --plan PINNED_ASSETS_JSON --cache ASSET_CACHE\n       openpilot-runtime-inputs --verify-models --root CLEAN_SOURCE --store MODEL_EXPORT\nCapture unpacked inputs, or assemble authenticated native inputs without executing installers. Source LFS preparation requires every current committed pointer and never mutates a checkout. Generic and system capture require independently verified provenance. Archive preparation authenticates vendor bytes on every reuse; it does not compile or activate a runtime.");
                return Ok(());
            }
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("missing input tree")?))
            }
            Some("--output") if output.is_none() => {
                output = Some(PathBuf::from(args.next().ok_or("missing receipt path")?))
            }
            Some("--prepare-rust") if !prepare_rust => prepare_rust = true,
            Some("--prepare-capnp") if !prepare_capnp => prepare_capnp = true,
            Some("--prepare-cargo") if !prepare_cargo => prepare_cargo = true,
            Some("--prepare-python") if !prepare_python => prepare_python = true,
            Some("--prepare-assets") if !prepare_assets => prepare_assets = true,
            Some("--capture-system-sdk") if !capture_system => capture_system = true,
            Some("--prepare-source-lfs") if !prepare_source_assets => prepare_source_assets = true,
            Some("--objects") if objects.is_none() => {
                objects = Some(PathBuf::from(
                    args.next().ok_or("missing immutable object store")?,
                ))
            }
            Some("--lock") if lock.is_none() => {
                lock = Some(PathBuf::from(args.next().ok_or("missing Cargo lock path")?))
            }
            Some("--plan") if plan.is_none() => {
                plan = Some(PathBuf::from(
                    args.next().ok_or("missing preparation plan")?,
                ))
            }
            Some("--cache") if cache.is_none() => {
                cache = Some(PathBuf::from(args.next().ok_or("missing asset cache")?))
            }
            Some("--store") if store.is_none() => {
                store = Some(PathBuf::from(args.next().ok_or("missing input store")?))
            }
            _ => return Err("unknown or duplicate input receipt argument".into()),
        }
    }
    if u8::from(prepare_rust)
        + u8::from(prepare_capnp)
        + u8::from(prepare_cargo)
        + u8::from(capture_system)
        + u8::from(prepare_source_assets)
        + u8::from(prepare_python)
        + u8::from(prepare_assets)
        + u8::from(verify_models)
        > 1
    {
        return Err("choose exactly one native preparation mode".into());
    }
    if !prepare_cargo && lock.is_some() {
        return Err("--lock requires --prepare-cargo".into());
    }
    if !prepare_source_assets && objects.is_some() {
        return Err("--objects requires --prepare-source-lfs".into());
    }
    if verify_models {
        if plan.is_some() || cache.is_some() || output.is_some() {
            return Err(
                "model verification accepts only --root SOURCE and --store MODEL_EXPORT".into(),
            );
        }
        let source = root.ok_or("--root SOURCE is required")?;
        let prefix = store.ok_or("--store MODEL_EXPORT is required")?;
        let files = openpilot_runtime_build::model_inputs::verify(&source, &prefix)?;
        println!(
            "{}",
            serde_json::json!({"schema":1,"files":files,"gpu_executed":false,"runtime_activated":false})
        );
        return Ok(());
    }
    if prepare_assets {
        if root.is_some() || output.is_some() || store.is_some() {
            return Err("asset preparation accepts only --plan and --cache".into());
        }
        let plan = openpilot_runtime_build::assets::Plan::read(&plan.ok_or("--plan is required")?)?;
        let receipts = openpilot_runtime_build::assets::prepare(
            &cache.ok_or("--cache is required")?,
            &plan,
            &openpilot_runtime_build::assets::Curl,
        )?;
        println!("{}", serde_json::to_string_pretty(&receipts)?);
        return Ok(());
    }
    if prepare_python {
        if root.is_some() || output.is_some() {
            return Err("Python preparation does not accept a capture root or output".into());
        }
        #[cfg(feature = "prepare-python")]
        {
            let plan =
                openpilot_runtime_build::python::Plan::read(&plan.ok_or("--plan is required")?)?;
            let prepared = openpilot_runtime_build::python::prepare(
                &plan,
                &cache.ok_or("--cache is required")?,
                &store.ok_or("--store is required")?,
                &openpilot_runtime_build::assets::Curl,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
            return Ok(());
        }
        #[cfg(not(feature = "prepare-python"))]
        return Err("this build does not include the prepare-python Cargo feature".into());
    }
    if prepare_source_assets {
        if output.is_some() {
            return Err("source LFS preparation does not accept a generic capture output".into());
        }
        #[cfg(feature = "prepare-source-assets")]
        {
            let plan = openpilot_runtime_build::source_assets::Plan::read(
                &plan.ok_or("--plan is required")?,
            )?;
            let prepared = openpilot_runtime_build::source_assets::prepare(
                &plan,
                &root.ok_or("--root is required")?,
                &cache.ok_or("--cache is required")?,
                &store.ok_or("--store is required")?,
                &objects.ok_or("--objects is required")?,
                &openpilot_runtime_build::assets::Curl,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
            return Ok(());
        }
        #[cfg(not(feature = "prepare-source-assets"))]
        return Err("this build does not include the prepare-source-assets Cargo feature".into());
    }
    if capture_system {
        if cache.is_some() || store.is_some() {
            return Err("system SDK capture does not install or prepare a cache".into());
        }
        let root = root.ok_or("--root is required")?.canonicalize()?;
        let output = output.ok_or("--output is required")?;
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()?;
        if parent.starts_with(&root) {
            return Err(
                "system SDK receipt must be outside the independently verified image root".into(),
            );
        }
        let plan =
            openpilot_runtime_build::system_sdk::Plan::read(&plan.ok_or("--plan is required")?)?;
        let snapshot = openpilot_runtime_build::system_sdk::Snapshot::capture(&root, &plan)?;
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        if bytes.len() + 1 > 16 * 1024 * 1024 {
            return Err("system SDK receipt exceeds 16 MiB".into());
        }
        let free = rustix::fs::statvfs(&parent)?;
        if free
            .f_bavail
            .checked_mul(free.f_frsize)
            .ok_or("SDK receipt space overflow")?
            < bytes.len() as u64 + 1
        {
            return Err("insufficient capacity for this SDK receipt; prior inputs retained".into());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
        temporary.write_all(&bytes)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&output)?;
        File::open(parent)?.sync_all()?;
        return Ok(());
    }
    if prepare_cargo {
        if root.is_some() || output.is_some() {
            return Err("Cargo preparation does not accept generic capture paths".into());
        }
        #[cfg(feature = "prepare-cargo")]
        {
            let plan = openpilot_runtime_build::cargo_inputs::Plan::read(
                &plan.ok_or("--plan is required")?,
            )?;
            let prepared = openpilot_runtime_build::cargo_inputs::prepare(
                &plan,
                &lock.ok_or("--lock is required")?,
                &cache.ok_or("--cache is required")?,
                &store.ok_or("--store is required")?,
                &openpilot_runtime_build::assets::Curl,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
            return Ok(());
        }
        #[cfg(not(feature = "prepare-cargo"))]
        return Err("this build does not include the prepare-cargo Cargo feature".into());
    }
    if prepare_capnp {
        if root.is_some() || output.is_some() {
            return Err("Cap'n Proto preparation does not accept generic capture paths".into());
        }
        #[cfg(feature = "prepare-capnp")]
        {
            let plan =
                openpilot_runtime_build::assets::Plan::read(&plan.ok_or("--plan is required")?)?;
            let prepared = openpilot_runtime_build::capnp::prepare(
                &plan,
                &cache.ok_or("--cache is required")?,
                &store.ok_or("--store is required")?,
                &openpilot_runtime_build::assets::Curl,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
            return Ok(());
        }
        #[cfg(not(feature = "prepare-capnp"))]
        return Err("this build does not include the prepare-capnp Cargo feature".into());
    }
    if prepare_rust {
        if root.is_some() || output.is_some() {
            return Err("Rust preparation does not accept generic capture paths".into());
        }
        #[cfg(feature = "prepare-rust")]
        {
            let plan = toolchain::Plan::read(&plan.ok_or("--plan is required")?)?;
            let prepared = toolchain::prepare(
                &plan,
                &cache.ok_or("--cache is required")?,
                &store.ok_or("--store is required")?,
                &openpilot_runtime_build::assets::Curl,
            )?;
            println!("{}", serde_json::to_string_pretty(&prepared)?);
            return Ok(());
        }
        #[cfg(not(feature = "prepare-rust"))]
        return Err("this build does not include the prepare-rust Cargo feature".into());
    }
    if plan.is_some() || cache.is_some() || store.is_some() {
        return Err("preparation paths require a native preparation mode".into());
    }
    let root = root.ok_or("--root is required")?;
    let output = output.ok_or("--output is required")?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .canonicalize()?;
    if parent.starts_with(root.canonicalize()?) {
        return Err("input receipt must be outside the input tree".into());
    }
    let tree = Tree::capture(&root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
    serde_json::to_writer_pretty(&mut temporary, &tree)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(&output)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime inputs: {error}");
        std::process::exit(1);
    }
}
