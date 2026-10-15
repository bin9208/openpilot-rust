//! Local-only native bundle creation; no upload, process selection or device access.
use super::{dbc, identity};
use serde_json::{json, Value};
use std::{fs, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn space(parent: &Path, growth: u64) -> Result<()> {
    let status = rustix::fs::statvfs(parent)?;
    let available = status.f_bavail.saturating_mul(status.f_frsize);
    if available < 25 * 1024u64.pow(3) + growth {
        return Err(format!(
            "native replay requires 25 GiB plus {growth} bytes growth; available {available}"
        )
        .into());
    }
    Ok(())
}
pub fn dbcs(source: &Path, destination: &Path) -> Result<()> {
    let schemas = dbc::generate(source)?;
    let growth = schemas.values().map(|v| v.len() as u64).sum::<u64>() + 1024 * 1024;
    space(
        destination
            .parent()
            .ok_or("asset destination parent absent")?,
        growth,
    )?;
    fs::create_dir(destination)?;
    let mut hashes = serde_json::Map::new();
    for (name, contents) in schemas {
        fs::write(destination.join(&name), contents)?;
        hashes.insert(
            name.clone(),
            json!(identity::file_hash(&destination.join(name))?),
        );
    }
    fs::write(
        destination.join("radarcan-assets.json"),
        serde_json::to_vec_pretty(
            &json!({"native":true,"upstream":identity::UPSTREAM,"asset_sha256":hashes}),
        )?,
    )?;
    Ok(())
}
fn tree_size(source: &Path) -> Result<u64> {
    let mut size = 0u64;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        size = size
            .checked_add(if kind.is_dir() {
                tree_size(&entry.path())?
            } else if kind.is_file() {
                entry.metadata()?.len()
            } else {
                return Err("native assets reject links and special files".into());
            })
            .ok_or("asset size overflow")?;
    }
    Ok(size)
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination)?;
    for item in fs::read_dir(source)? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_dir() {
            copy_tree(&item.path(), &destination.join(item.file_name()))?;
        } else if kind.is_file() {
            fs::copy(item.path(), destination.join(item.file_name()))?;
        } else {
            return Err("native asset bundle rejects links and special files".into());
        }
    }
    Ok(())
}
pub fn bundle(
    root: &Path,
    destination: &Path,
    mut manifest: Value,
    numerics: Option<&Path>,
) -> Result<()> {
    if identity::fingerprint(root)? != env!("RADAR_REPLAY_SOURCE") {
        return Err(
            "replay executable is stale for selected checkout; rebuild before packaging".into(),
        );
    }
    if let Some(numerics) = numerics {
        openpilot_radarcan::numerics::Numerics::load(numerics)?;
    }
    let executable = std::env::current_exe()?;
    // Measure providers before copying; retain the required free-space reserve.
    space(
        destination
            .parent()
            .ok_or("bundle destination parent absent")?,
        fs::metadata(&executable)?.len()
            + numerics.map(tree_size).transpose()?.unwrap_or(0)
            + 128 * 1024 * 1024,
    )?;
    fs::create_dir(destination)?;
    fs::copy(&executable, destination.join("openpilot-radar-replay"))?;
    manifest["binary_sha256"] = json!(identity::file_hash(&executable)?);
    manifest["mode"] = json!(["prepared_frames", "recorded_live_tracks"]);
    manifest["raw_can_provider"] = Value::Null;
    if let Some(numerics) = numerics {
        dbcs(
            &root.join("opendbc_repo/opendbc/dbc"),
            &destination.join("dbc"),
        )?;
        copy_tree(numerics, &destination.join("numerics"))?;
        manifest["mode"] = json!(["prepared_frames", "recorded_live_tracks", "raw_can_batches"]);
        manifest["raw_can_provider"] = json!({"dbc_sha256":identity::directory_hash(&destination.join("dbc"))?,"numerics_sha256":identity::directory_hash(&destination.join("numerics"))?});
    }
    for (source, target) in [
        (root.join("LICENSE"), "LICENSE"),
        (root.join("opendbc_repo/LICENSE"), "OPENDBC-LICENSE"),
    ] {
        if source.is_file() {
            fs::copy(source, destination.join(target))?;
        }
    }
    fs::copy(
        root.join("rust/upstream/dbc-generators.json"),
        destination.join("dbc-source-provenance.json"),
    )?;
    fs::write(
        destination.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

/// Test/reference staging preserves tracked source tables and adds missing
/// generated tables atomically. It never overwrites a previous local schema.
pub fn stage_dbcs(source: &Path) -> Result<Vec<String>> {
    use std::io::Write;
    let schemas = dbc::generate(source)?;
    let growth = schemas
        .iter()
        .filter(|(name, _)| !source.join(name).exists())
        .map(|(_, content)| content.len() as u64)
        .sum::<u64>();
    space(source, growth)?;
    let mut added = Vec::new();
    for (name, content) in schemas {
        let path = source.join(&name);
        if path.exists() {
            continue;
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        file.write_all(content.as_bytes())?;
        added.push(name);
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_needs_no_python_and_preserves_existing_source_tables() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../opendbc_repo/opendbc/dbc");
        let target =
            std::env::temp_dir().join(format!("radar-native-stage-{}", std::process::id()));
        space(&std::env::temp_dir(), 32 * 1024 * 1024).unwrap();
        fs::create_dir(&target).unwrap();
        let fixture: Value =
            serde_json::from_str(include_str!("../../../../upstream/dbc-generators.json")).unwrap();
        let mut existing = Vec::new();
        for name in fixture["inputs"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|name| name.ends_with(".dbc"))
        {
            let destination = target.join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source.join(name), &destination).unwrap();
            existing.push((
                destination.clone(),
                identity::file_hash(&destination).unwrap(),
            ));
        }
        let added = stage_dbcs(&target).unwrap();
        assert_eq!(added.len(), 38);
        assert!(stage_dbcs(&target).unwrap().is_empty());
        for (path, checksum) in existing {
            assert_eq!(identity::file_hash(&path).unwrap(), checksum);
        }
        assert!(!target.join("generator/generator.py").exists());
        fs::remove_dir_all(target).unwrap();
    }
}
