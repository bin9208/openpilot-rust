//! Stage the architecture-specific json11 wheel pinned in the repository lock.
use crate::{
    disk_space,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::{digest, Result},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io,
    path::{Component, Path, PathBuf},
    time::Duration,
};
pub struct Staged {
    pub prefix: PathBuf,
    pub dependency: Value,
}
const GROWTH: u64 = 256 * 1024 * 1024;
/// Returns the original prefix/dependency pair without inventing a caller receipt.
pub fn stage(
    root: &Path,
    output: &Path,
    architecture: &str,
    runner: &mut dyn Runner,
    free_bytes: u64,
) -> Result<Staged> {
    fs::create_dir_all(output)?;
    disk_space::Space::new(free_bytes, GROWTH)?.require()?;
    let lock: toml::Value = toml::from_str(&fs::read_to_string(root.join("uv.lock"))?)?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("missing lock packages")?;
    let package = packages
        .iter()
        .find(|p| p.get("name").and_then(toml::Value::as_str) == Some("comma-deps-json11"))
        .ok_or("missing pinned json11 package")?;
    if !["x86_64", "aarch64"].contains(&architecture) {
        return Err(format!("unsupported native json11 architecture: {architecture}").into());
    }
    let selector = format!("manylinux_2_28_{architecture}");
    let wheel = package
        .get("wheels")
        .and_then(toml::Value::as_array)
        .ok_or("missing locked json11 wheels")?
        .iter()
        .find(|w| {
            w.get("url")
                .and_then(toml::Value::as_str)
                .is_some_and(|url| url.contains(&selector))
        })
        .ok_or("missing architecture-specific json11 wheel")?;
    let url = wheel
        .get("url")
        .and_then(toml::Value::as_str)
        .ok_or("wheel URL missing")?;
    let filename = url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or("invalid wheel filename")?;
    let archive = output.join(filename);
    let argv = vec![
        "curl".into(),
        "-fsSL".into(),
        url.into(),
        "-o".into(),
        archive.to_string_lossy().into_owned(),
    ];
    let captured = runner.run(&CommandSpec::new(argv, Duration::from_secs(300)))?;
    if captured.code != 0 {
        return Err(format!(
            "json11 download exited {}: {}",
            captured.code,
            String::from_utf8_lossy(&captured.stderr)
        )
        .into());
    }
    let sha256 = digest(&archive)?;
    if wheel.get("hash").and_then(toml::Value::as_str) != Some(format!("sha256:{sha256}").as_str())
    {
        return Err("json11 wheel hash mismatch".into());
    }
    let mut wheel = zip::ZipArchive::new(File::open(&archive)?)?;
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    // Inspect the complete locked archive before creating any extracted output.
    for index in 0..wheel.len() {
        let entry = wheel.by_index(index)?;
        let path = entry.enclosed_name().ok_or("unsafe json11 ZIP path")?;
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || !paths.insert(path)
        {
            return Err("unsafe or duplicate json11 ZIP path".into());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("json11 ZIP symlink is not supported".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("json11 ZIP size overflow")?;
        if total > GROWTH {
            return Err("json11 ZIP exceeds reserved staging growth".into());
        }
    }
    let extracted = output.join("json11");
    fs::create_dir_all(&extracted)?;
    for index in 0..wheel.len() {
        let mut entry = wheel.by_index(index)?;
        let path = extracted.join(entry.enclosed_name().ok_or("unsafe json11 ZIP path")?);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
        } else {
            fs::create_dir_all(path.parent().ok_or("ZIP entry parent missing")?)?;
            let mut file = File::create(path)?;
            io::copy(&mut entry, &mut file)?;
        }
    }
    let mut prefixes = fs::read_dir(&extracted)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    prefixes.retain(|path| {
        path.file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".data"))
    });
    prefixes.sort();
    let prefix = prefixes
        .into_iter()
        .map(|path| path.join("purelib/json11/install"))
        .find(|path| path.is_dir())
        .ok_or("json11 install prefix missing from wheel")?;
    Ok(Staged {
        prefix,
        dependency: json!({"json11_version":package.get("version").and_then(toml::Value::as_str).ok_or("json11 version missing")?,"json11_sha256":sha256,"json11_architecture":architecture}),
    })
}
pub fn cli() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut values = BTreeMap::new();
    while let Some(key) = args.next() {
        if key == "--help" || key == "-h" {
            println!(
                "Usage: stage-json11 --root ROOT --output OUTPUT [--architecture x86_64|aarch64]"
            );
            return Ok(());
        }
        if !["--root", "--output", "--architecture"].contains(&key.as_str()) {
            return Err("unknown json11 staging argument".into());
        }
        let value = args.next().ok_or("argument requires value")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate json11 staging argument".into());
        }
    }
    let root = PathBuf::from(values.remove("--root").ok_or("--root required")?);
    let output = PathBuf::from(values.remove("--output").ok_or("--output required")?);
    let architecture = values
        .remove("--architecture")
        .unwrap_or_else(|| std::env::consts::ARCH.into());
    let space = disk_space::inspect(&output, GROWTH)?;
    let staged = stage(
        &root,
        &output,
        &architecture,
        &mut NativeRunner,
        space.free_bytes,
    )?;
    println!(
        "{}",
        json!({"prefix":staged.prefix,"dependency":staged.dependency})
    );
    Ok(())
}
