//! Only the original ARM headers/static archive, never the wheel's Mesa/Python.
use crate::{
    encoder_stage,
    native_codec_inputs::{hash, insert, json, prepare_tree, regular, write, Prepared},
    raylib_stage,
    seal::Result,
};
use openpilot_runtime_build::{
    assets,
    inputs::{Entry, Tree},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const VERSION: &str = "6.0.0.1.post103";
const RAYLIB_HEADER: &str = "047e7255f93f8c34039cab906ad76136706b5c7b4c5b5b065d84141963ee9b6b";

fn lock(root: &Path, asset: &assets::Asset) -> Result<()> {
    let file = regular(&root.join("uv.lock"))?;
    if file.metadata()?.len() > 8 * 1024 * 1024 {
        return Err("raylib source lock exceeds 8 MiB".into());
    }
    let value: toml::Value = std::io::read_to_string(file.take(8 * 1024 * 1024 + 1))?.parse()?;
    let rows = value
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or("invalid raylib source lock")?;
    let selected: Vec<_> = rows
        .iter()
        .filter(|r| r.get("name").and_then(toml::Value::as_str) == Some("comma-deps-raylib"))
        .collect();
    if selected.len() != 1
        || selected[0].get("version").and_then(toml::Value::as_str) != Some(VERSION)
    {
        return Err("raylib lock differs from original native SDK version".into());
    }
    let wheels = selected[0]
        .get("wheels")
        .and_then(toml::Value::as_array)
        .ok_or("raylib lock has no original wheels")?;
    let matches: Vec<_> = wheels
        .iter()
        .filter(|w| w.get("url").and_then(toml::Value::as_str) == Some(&asset.url))
        .collect();
    if matches.len() != 1
        || matches[0].get("hash").and_then(toml::Value::as_str)
            != Some(&format!("sha256:{}", asset.sha256))
        || matches[0].get("size").and_then(toml::Value::as_integer)
            != Some(i64::try_from(asset.bytes)?)
    {
        return Err("raylib lock differs from original ARM wheel URL/SHA/size".into());
    }
    Ok(())
}

fn expected(archive: &Path, asset: &assets::Asset, destination: Option<&Path>) -> Result<Tree> {
    let mut bytes = Vec::new();
    regular(archive)?
        .take(asset.bytes + 1)
        .read_to_end(&mut bytes)?;
    // The same bytes are authenticated and parsed by the legacy native SDK
    // admission. Additional unrelated shared libraries never enter this tree.
    let mut files = raylib_stage::native_arm_files(&bytes)?;
    if hash(
        files
            .get("include/raylib.h")
            .ok_or("missing original raylib header")?,
    ) != RAYLIB_HEADER
    {
        return Err("raylib header differs from the original released provider header".into());
    }
    let mut wheel = zip::ZipArchive::new(Cursor::new(&bytes))?;
    let prefix = format!("comma_deps_raylib-{VERSION}.dist-info/");
    let mut metadata = BTreeSet::new();
    let mut total = 0u64;
    for index in 0..wheel.len() {
        let mut file = wheel.by_index(index)?;
        let Some(relative) = file
            .name()?
            .strip_prefix(&prefix)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
        else {
            continue;
        };
        if file.is_dir() {
            continue;
        }
        encoder_stage::safe(&relative)?;
        let mode = file.unix_mode().unwrap_or(0);
        total = total
            .checked_add(file.size())
            .ok_or("raylib metadata size overflow")?;
        if mode & 0o6000 != 0 || !matches!(mode & 0o170000, 0 | 0o100000) || total > 4 * 1024 * 1024
        {
            return Err("unsupported or oversized native raylib metadata".into());
        }
        let size = file.size();
        let mut data = Vec::new();
        (&mut file).take(size + 1).read_to_end(&mut data)?;
        if data.len() as u64 != size {
            return Err("truncated raylib metadata".into());
        }
        if relative == "METADATA" {
            let text = std::str::from_utf8(&data)?;
            let version = format!("Version: {VERSION}");
            if !text.lines().any(|l| l == "Name: comma-deps-raylib")
                || !text.lines().any(|l| l == version)
            {
                return Err("native raylib package identity differs".into());
            }
        }
        if relative == "WHEEL"
            && !std::str::from_utf8(&data)?
                .lines()
                .any(|l| l == "Tag: py3-none-manylinux_2_28_aarch64")
        {
            return Err("native raylib target tag differs".into());
        }
        metadata.insert(relative.clone());
        if files
            .insert(format!("provenance/wheel-dist-info/{relative}"), data)
            .is_some()
        {
            return Err("duplicate raylib metadata".into());
        }
    }
    if !["METADATA", "WHEEL", "RECORD"]
        .iter()
        .all(|name| metadata.contains(*name))
    {
        return Err("native raylib wheel is missing original distribution metadata".into());
    }
    files.insert(
        "provenance/native-raylib-sdk.json".into(),
        json(&(
            "openpilot-native-raylib-sdk-v1",
            "aarch64-unknown-linux-gnu",
            VERSION,
            asset,
        ))?,
    );
    let mut entries = BTreeMap::new();
    for (name, data) in files {
        let path = PathBuf::from(name);
        if let Some(root) = destination {
            fs::create_dir_all(
                root.join(&path)
                    .parent()
                    .ok_or("missing native raylib parent")?,
            )?;
            write(&root.join(&path), &data)?;
        }
        insert(
            &mut entries,
            path,
            Entry::File {
                sha256: hash(&data),
                bytes: data.len() as u64,
                mode: 0o444,
            },
        )?;
    }
    Ok(Tree { schema: 1, entries })
}

pub fn prepare(
    root: &Path,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let asset = raylib_stage::native_arm_asset()?;
    lock(root, &asset)?;
    let plan = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([("raylib-sdk".into(), asset.clone())]),
    };
    let report = prepare_tree(
        hash(&json(&("openpilot-native-raylib-sdk-v1", VERSION, &asset))?),
        store,
        || {
            assets::prepare(cache, &plan, download)?
                .remove("raylib-sdk")
                .ok_or_else(|| "missing raylib SDK asset".into())
        },
        |archive, destination| expected(archive, &asset, destination),
    )?;
    lock(root, &asset)?;
    Ok(report)
}

pub fn cli(arguments: &[String]) -> Result<()> {
    if arguments
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("Usage: stage-startup-ui-raylib --prepare-native-inputs --cache ASSET_CACHE --store PREFIX_STORE [--root SOURCE]\nPrepare only the original ARM raylib headers/static archive and metadata. No Python, compiler or activation.");
        return Ok(());
    }
    let mut values = BTreeMap::new();
    let mut mode = false;
    let mut args = arguments.iter();
    while let Some(key) = args.next() {
        if key == "--prepare-native-inputs" && !mode {
            mode = true;
            continue;
        }
        if !["--root", "--cache", "--store"].contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing native raylib argument")?,
                )
                .is_some()
        {
            return Err("invalid, mixed or duplicate native raylib preparation argument".into());
        }
    }
    if !mode {
        return Err("--prepare-native-inputs required".into());
    }
    let root = values.get("--root").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."),
        |value| PathBuf::from(value.as_str()),
    );
    let cache = Path::new(values.get("--cache").ok_or("--cache required")?);
    let store = Path::new(values.get("--store").ok_or("--store required")?);
    println!(
        "{}",
        serde_json::to_string(&prepare(&root, cache, store, &assets::Curl)?)?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NoDownload;
    impl assets::Download for NoDownload {
        fn fetch(&self, _: &assets::Asset, _: &Path) -> Result<u64> {
            panic!("invalid source must fail before any download")
        }
    }
    #[test]
    fn original_arm_sdk_matches_the_source_lock_and_original_shared_header() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let asset = raylib_stage::native_arm_asset().unwrap();
        lock(&root, &asset).unwrap();
        assert_eq!(asset.bytes, 21707647);
        assert_eq!(
            asset.sha256,
            "d9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a"
        );
        let provider =
            crate::native_provider::Plan::read(&root.join("rust/native-raylib-provider.json"))
                .unwrap();
        assert_eq!(provider.profile, "raylib600-shared");
        assert_eq!(
            provider.members[Path::new("share/licenses/raylib/raylib.h")].sha256,
            RAYLIB_HEADER
        );
    }
    #[test]
    fn changed_lock_cannot_create_cache_or_prefix_or_invoke_a_downloader() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("uv.lock"), "package=[]").unwrap();
        let cache = temp.path().join("assets");
        let store = temp.path().join("prefixes");
        assert!(prepare(temp.path(), &cache, &store, &NoDownload).is_err());
        assert!(!cache.exists() && !store.exists());
    }
}
