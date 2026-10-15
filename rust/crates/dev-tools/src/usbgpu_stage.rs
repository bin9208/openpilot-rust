//! Stage verified generic eGPU companions without a synthetic remote catalog.
use crate::{
    disk_space,
    seal::{digest, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
const FIRMWARE: [&str; 8] = [
    "psp_14_0_2_sos.bin",
    "gc_12_0_0_pfp.bin",
    "gc_12_0_0_me.bin",
    "gc_12_0_0_mec.bin",
    "gc_12_0_0_rlc.bin",
    "gc_12_0_0_imu.bin",
    "sdma_7_0_0.bin",
    "smu_14_0_2.bin",
];
const MAX_COMPANIONS: u64 = 128 << 20;
pub struct Bundle {
    pub path: PathBuf,
    pub manifest_sha256: String,
    pub firmware_license: PathBuf,
    pub model: Option<PathBuf>,
}
#[derive(Deserialize)]
struct Manifest {
    version: u32,
    model_sha256: String,
    files: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}
struct Copy {
    source: PathBuf,
    target: PathBuf,
    sha256: String,
    bytes: u64,
}
fn check(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn regular(root: &Path, relative: &str) -> Result<PathBuf> {
    check(
        !relative.is_empty() && !Path::new(relative).is_absolute(),
        "invalid native companion path",
    )?;
    let mut result = root.to_owned();
    for component in Path::new(relative).components() {
        check(
            matches!(component, Component::Normal(_)),
            "invalid native companion path",
        )?;
        result.push(component);
        check(!result.is_symlink(), "native companion symlink")?;
    }
    check(result.is_file(), "missing native companion")?;
    Ok(result)
}
fn read_json(path: &Path) -> Result<Value> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    check(
        bytes.len() <= 16 * 1024 * 1024,
        "native companion JSON too large",
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn add(files: &mut Vec<Copy>, source: PathBuf, target: PathBuf, hash: String) -> Result<()> {
    let bytes = fs::metadata(&source)?.len();
    files.push(Copy {
        source,
        target,
        sha256: hash,
        bytes,
    });
    Ok(())
}
// Struct field order deliberately matches the existing Python producer record.
#[derive(Serialize)]
struct Provider<'a> {
    version: u32,
    kind: &'a str,
    model_sha256: &'a str,
    model_checkpoint: &'a str,
    pickle: Pickle<'a>,
    assets_manifest_sha256: &'a str,
    runtime_archive_sha256: &'a str,
}
#[derive(Serialize)]
struct Pickle<'a> {
    path: &'a str,
    size: u64,
    sha256: &'a str,
}

pub fn stage(root: &Path, bundle: &Bundle) -> Result<Value> {
    if bundle.model.is_none() {
        let pinned = openpilot_usbgpu::model_delivery::pinned_manifest();
        return stage_pinned(root, bundle, &pinned.sha256, pinned.size);
    }
    let manifest = read_json(&regular(&bundle.path, "manifest.json")?)?;
    let digest = manifest["model_sha256"]
        .as_str()
        .ok_or("native source model identity missing")?;
    let selected = openpilot_usbgpu::model_delivery::known_manifest(digest)
        .ok_or("native provider model is not a supported source-pinned PKL")?;
    stage_pinned(root, bundle, &selected.sha256, selected.size)
}
fn stage_pinned(root: &Path, bundle: &Bundle, pinned_sha: &str, pinned_size: u64) -> Result<Value> {
    let root = root.canonicalize()?;
    let manifest = regular(&bundle.path, "manifest.json")?;
    check(
        digest(&manifest)? == bundle.manifest_sha256,
        "native package manifest identity mismatch",
    )?;
    let record: Manifest = serde_json::from_value(read_json(&manifest)?)?;
    check(
        record.version == 1 && record.files.len() <= 512,
        "unsupported native companion manifest",
    )?;
    check(
        sha(&record.model_sha256),
        "invalid selected native model identity",
    )?;
    let selected = &record.model_sha256;
    let destination = root.join("rust/bin/usbgpu-assets");
    let mut files = Vec::new();
    add(
        &mut files,
        manifest,
        destination.join("manifest.json"),
        bundle.manifest_sha256.clone(),
    )?;
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for entry in &record.files {
        total = total
            .checked_add(entry.bytes)
            .ok_or("oversized native companion package")?;
        check(
            total <= MAX_COMPANIONS,
            "incomplete/oversized native companion package",
        )?;
        let source = regular(&bundle.path, &entry.path)?;
        check(
            names.insert(entry.path.clone())
                && entry.path != "manifest.json"
                && entry.path != "native-provider.json"
                && fs::metadata(&source)?.len() == entry.bytes
                && digest(&source)? == entry.sha256,
            "native companion checksum/size/duplicate mismatch",
        )?;
        add(
            &mut files,
            source,
            destination.join(&entry.path),
            entry.sha256.clone(),
        )?;
    }
    let metadata = read_json(&regular(
        &root,
        "rust/crates/usbgpu/assets/amd-metadata.json",
    )?)?;
    let mut required = BTreeSet::from([
        "tinygrad-LICENSE".to_owned(),
        "probe-gfx1200.json".into(),
        "source-provenance.json".into(),
        format!("models/{selected}/model.hcq.json"),
        format!("models/{selected}/model.hcq-meta.json"),
    ]);
    for name in FIRMWARE {
        required.insert(format!("firmware/{name}"));
        check(
            metadata["firmware_hashes"][name].as_str()
                == Some(digest(&regular(&bundle.path, &format!("firmware/{name}"))?)?.as_str()),
            "firmware differs from source-pinned AMD metadata",
        )?;
    }
    for camera in ["1344x760", "1928x1208"] {
        required.insert(format!("warp-gfx1200-{camera}.json"));
        let prefix = format!("warp-qcom-a630-{camera}");
        for name in ["graph.json", "weights.bin", "provenance.json"] {
            required.insert(format!("{prefix}/{name}"));
        }
        let graph = read_json(&regular(&bundle.path, &format!("{prefix}/graph.json"))?)?;
        let kernels = graph["kernels"]
            .as_array()
            .ok_or("invalid native warp graph kernels")?;
        check(
            kernels.len() <= 512,
            "incomplete/oversized native companion package",
        )?;
        for index in 0..kernels.len() {
            required.insert(format!("{prefix}/kernel-{index}.bin"));
        }
    }
    check(
        required.is_subset(&names),
        "incomplete/oversized native companion package",
    )?;
    let licenses = root.join("rust/bin/licenses/usbgpu");
    for (source, target) in [
        (bundle.firmware_license.clone(), "LICENSE.amdgpu"),
        (
            regular(&bundle.path, "tinygrad-LICENSE")?,
            "tinygrad-LICENSE",
        ),
        (
            regular(&bundle.path, "source-provenance.json")?,
            "build-provenance.json",
        ),
    ] {
        let hash = digest(&source)?;
        add(&mut files, source, licenses.join(target), hash)?;
    }
    let provider = if let Some(model) = &bundle.model {
        check(
            selected == pinned_sha,
            "native offline provider must contain the exact source-pinned PKL",
        )?;
        let descriptor = read_json(&regular(
            &bundle.path,
            &format!("models/{selected}/model.hcq.json"),
        )?)?;
        check(
            descriptor["model_bytes"].as_u64() == Some(pinned_size)
                && fs::metadata(model)?.len() == pinned_size
                && digest(model)? == *selected,
            "native offline provider must contain the exact source-pinned PKL",
        )?;
        let metadata = read_json(&regular(
            &bundle.path,
            &format!("models/{selected}/model.hcq-meta.json"),
        )?)?;
        let provenance = read_json(&regular(&bundle.path, "source-provenance.json")?)?;
        let path = format!("models/{selected}/model.pkl");
        add(
            &mut files,
            model.clone(),
            destination.join(&path),
            selected.clone(),
        )?;
        Some(format!(
            "{}\n",
            serde_json::to_string_pretty(&Provider {
                version: 1,
                kind: "native-generic-hcq",
                model_sha256: selected,
                model_checkpoint: metadata["checkpoint"]
                    .as_str()
                    .ok_or("invalid native model checkpoint")?,
                pickle: Pickle {
                    path: &path,
                    size: pinned_size,
                    sha256: selected
                },
                assets_manifest_sha256: &bundle.manifest_sha256,
                runtime_archive_sha256: provenance["runtime_sha256"]
                    .as_str()
                    .ok_or("invalid native runtime provenance")?,
            })?
        ))
    } else {
        None
    };
    let provider_target = destination.join("native-provider.json");
    let growth = files
        .iter()
        .try_fold(0u64, |total, file| {
            total
                .checked_add(file.bytes)
                .ok_or("native companion size overflow")
        })?
        .checked_add(provider.as_ref().map_or(0, |value| value.len() as u64))
        .ok_or("native companion size overflow")?;
    disk_space::inspect(&root, growth)?.require()?;
    for file in &files {
        for parent in file.target.ancestors().skip(1) {
            check(!parent.is_symlink(), "package directory symlink")?;
            check(
                !parent.exists() || parent.is_dir(),
                "package parent is not a directory",
            )?;
        }
        check(
            !file.target.is_symlink()
                && (!file.target.exists() || digest(&file.target)? == file.sha256),
            "conflicting package artifact",
        )?;
    }
    if let Some(provider) = &provider {
        check(
            !provider_target.is_symlink()
                && (!provider_target.exists()
                    || fs::read_to_string(&provider_target)? == *provider),
            "conflicting native provider record",
        )?;
    }
    // Freeze and revalidate all bytes before publishing, with bounded reads so
    // mutable inputs cannot exceed the disk space reserved above.
    let frozen = tempfile::tempdir_in(&root)?;
    for (index, file) in files.iter().enumerate() {
        let path = frozen.path().join(index.to_string());
        let mut source = File::open(&file.source)?;
        let mut output = File::create(&path)?;
        copy_exact(&mut source, &mut output, file.bytes)?;
        check(
            digest(&path)? == file.sha256,
            "native companion changed while copying",
        )?;
    }
    for (index, file) in files.iter().enumerate() {
        fs::create_dir_all(file.target.parent().ok_or("artifact parent missing")?)?;
        if !file.target.exists() {
            fs::hard_link(frozen.path().join(index.to_string()), &file.target)?;
        } else {
            check(
                !file.target.is_symlink() && digest(&file.target)? == file.sha256,
                "conflicting package artifact",
            )?;
        }
    }
    if let Some(provider) = &provider {
        if !provider_target.exists() {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&provider_target)?;
            output.write_all(provider.as_bytes())?;
        } else {
            check(
                !provider_target.is_symlink() && fs::read_to_string(&provider_target)? == *provider,
                "conflicting native provider record",
            )?;
        }
    }
    Ok(
        json!({"package":destination,"manifest_sha256":bundle.manifest_sha256,"model_sha256":selected,"files":files.len()+usize::from(provider.is_some()),"bytes":growth}),
    )
}
fn copy_exact(input: &mut impl Read, output: &mut impl Write, size: u64) -> Result<()> {
    let limit = size
        .checked_add(1)
        .ok_or("native companion copy overflow")?;
    check(
        std::io::copy(&mut input.take(limit), output)? == size,
        "native companion size changed while copying",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growing_input_is_bounded_by_reserved_size() {
        let mut input = std::io::Cursor::new(vec![1u8; 1024]);
        let mut output = Vec::new();
        assert!(copy_exact(&mut input, &mut output, 2).is_err());
        assert_eq!(output.len(), 3);
    }
}
