use crate::{invalid, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

pub(crate) const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_UNPACKED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(crate) const SOURCE_ASSETS: [&str; 19] = [
    "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
    "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
    "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
    "openpilot/selfdrive/assets/images/speed_bg.png",
    "openpilot/selfdrive/assets/images/traffic_green.png",
    "openpilot/selfdrive/assets/images/traffic_red.png",
    "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
    "openpilot/selfdrive/carrot/web/assets/alert_camera.svg",
    "openpilot/selfdrive/carrot/web/assets/alert_police.svg",
    "openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx",
    "openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx",
    "panda/board/obj/bootstub.panda.bin",
    "panda/board/obj/bootstub.panda_h7.bin",
    "panda/board/obj/panda.bin.signed",
    "panda/board/obj/panda_h7.bin.signed",
    "panda/board/obj/panda_tesla_wake.bin.signed",
    "panda/board/obj/panda_h7_tesla_wake.bin.signed",
];
const REQUIRED: [&str; 16] = [
    "openpilot-launch",
    "openpilot-runtime-install",
    "openpilot-manager",
    "openpilot-process-child",
    "openpilot-reboot",
    "openpilot-startup-recovery",
    "openpilot-carrot-recovery",
    "openpilot-carrot-web-watchdog",
    "openpilot-carrot-server",
    "openpilot-agnos",
    "openpilot-agnos-updater",
    "openpilot-text-window",
    "openpilot-usbgpu-model",
    "openpilot-bootlog",
    "openpilot-usbgpu-worker",
    "openpilot-usbgpu-probe",
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Descriptor {
    pub schema: u32,
    pub source_commit: String,
    pub architecture: String,
    pub agnos_version: String,
    pub manifest_sha256: String,
    pub archive: Archive,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    pub name: String,
    pub sha256: String,
    pub bytes: u64,
    pub unpacked_bytes: u64,
}
impl Descriptor {
    pub fn validate(&self, sha: &str) -> Result<()> {
        if self.schema != 1
            || self.source_commit != sha
            || !hex(sha, 40)
            || self.architecture != "aarch64"
            || self.agnos_version != "19.8-carrot-bt1"
            || !hex(&self.manifest_sha256, 64)
            || !hex(&self.archive.sha256, 64)
            || self.archive.name != format!("openpilot-rust-{sha}-aarch64.tar.gz")
            || self.archive.bytes == 0
            || self.archive.bytes > MAX_UNPACKED_BYTES
            || self.archive.unpacked_bytes == 0
            || self.archive.unpacked_bytes > MAX_UNPACKED_BYTES
        {
            return Err(invalid(
                "invalid runtime release identity, schema, archive name or size",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema: u32,
    pub source_commit: String,
    pub agnos_version: String,
    pub files: Vec<Artifact>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub path: PathBuf,
    pub sha256: String,
    pub executable: bool,
}
impl Manifest {
    pub fn validate(&self, sha: &str) -> Result<()> {
        if self.schema != 1
            || self.source_commit != sha
            || !hex(sha, 40)
            || self.agnos_version != "19.8-carrot-bt1"
            || self.files.len() > 10000
        {
            return Err(invalid("invalid runtime manifest identity or size"));
        }
        let mut paths = BTreeSet::new();
        for artifact in &self.files {
            if !allowed_artifact(&artifact.path)
                || !hex(&artifact.sha256, 64)
                || !paths.insert(&artifact.path)
                || (source_asset(&artifact.path) && artifact.executable)
            {
                return Err(invalid(format!(
                    "invalid or duplicate manifest artifact: {}",
                    artifact.path.display()
                )));
            }
        }
        for binary in REQUIRED {
            let path = PathBuf::from(format!("rust/bin/{binary}"));
            if !self
                .files
                .iter()
                .any(|file| file.path == path && file.executable)
            {
                return Err(invalid(format!(
                    "required executable absent from manifest: {binary}"
                )));
            }
        }
        if !self.files.iter().any(|file| {
            file.path.starts_with("rust/bin/usbgpu-assets")
                && file.path != Path::new("rust/bin/usbgpu-assets")
        }) {
            return Err(invalid(
                "required GPU bootstrap assets absent from manifest",
            ));
        }
        Ok(())
    }
}
pub(crate) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn source_asset(path: &Path) -> bool {
    SOURCE_ASSETS.iter().any(|asset| Path::new(asset) == path)
}
pub(crate) fn clean_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.as_os_str().len() <= 4096
        && path.components().all(|p| matches!(p, Component::Normal(_)))
        && path.to_str().is_some_and(|s| {
            !s.contains('\0')
                && !s.contains('\\')
                && !s.contains("//")
                && !s.split('/').any(|p| p == "." || p == "..")
        })
}
pub(crate) fn allowed_artifact(path: &Path) -> bool {
    clean_path(path)
        && (path.starts_with("rust/bin") && path != Path::new("rust/bin") || source_asset(path))
}
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_JSON_BYTES {
        return Err(invalid("JSON exceeds size limit"));
    }
    Ok(serde_json::from_reader(file.take(MAX_JSON_BYTES))?)
}
pub(crate) fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub(crate) fn verify_generation(root: &Path, descriptor: &Descriptor) -> Result<Manifest> {
    let path = root.join("rust/runtime-manifest.json");
    if !fs::symlink_metadata(&path)?.is_file()
        || !fs::symlink_metadata(root.join(".runtime-release.json"))?.is_file()
    {
        return Err(invalid(
            "generation manifest and descriptor must be regular files",
        ));
    }
    if digest(&path)? != descriptor.manifest_sha256 {
        return Err(invalid("sealed manifest digest mismatch"));
    }
    let manifest: Manifest = read_json(&path)?;
    manifest.validate(&descriptor.source_commit)?;
    verify_files(root, &manifest)?;
    let mut source_bytes = 0_u64;
    for artifact in manifest.files.iter().filter(|f| source_asset(&f.path)) {
        source_bytes = source_bytes
            .checked_add(fs::metadata(root.join(&artifact.path))?.len())
            .ok_or_else(|| invalid("source asset size overflow"))?;
    }
    if source_bytes > 128 * 1024 * 1024 {
        return Err(invalid("source assets exceed reserved activation growth"));
    }
    let mut expected: BTreeSet<_> = manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .chain([
            PathBuf::from("rust/runtime-manifest.json"),
            PathBuf::from(".runtime-release.json"),
        ])
        .collect();
    // Older generations need not have a repair cache. This non-executable
    // retained download is verified against the descriptor again before use.
    match fs::symlink_metadata(root.join(crate::CACHED_ARCHIVE)) {
        Ok(metadata) if metadata.is_file() => {
            expected.insert(PathBuf::from(crate::CACHED_ARCHIVE));
        }
        Ok(_) => return Err(invalid("repair archive is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    verify_tree(root, Path::new(""), &expected)?;
    Ok(manifest)
}
pub(crate) fn verify_files(root: &Path, manifest: &Manifest) -> Result<()> {
    let root = root.canonicalize()?;
    let expected: BTreeSet<_> = manifest.files.iter().map(|f| f.path.clone()).collect();
    for artifact in &manifest.files {
        let path = root.join(&artifact.path).canonicalize()?;
        if !path.starts_with(&root) {
            return Err(invalid("artifact escapes package root"));
        }
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file()
            || metadata.permissions().mode() & 0o6000 != 0
            || (artifact.executable && metadata.permissions().mode() & 0o111 == 0)
        {
            return Err(invalid(format!(
                "artifact type or executable mode mismatch: {}",
                artifact.path.display()
            )));
        }
        if digest(&path)? != artifact.sha256 {
            return Err(invalid(format!(
                "artifact digest mismatch: {}",
                artifact.path.display()
            )));
        }
        verify_elf(&path, artifact.executable)?;
    }
    verify_tree(&root, Path::new("rust/bin"), &expected)
}
fn verify_elf(path: &Path, executable: bool) -> Result<()> {
    let mut bytes = [0; 64];
    let n = File::open(path)?.read(&mut bytes)?;
    if (executable || bytes[..4] == *b"\x7fELF")
        && (n < 64
            || bytes[..7] != *b"\x7fELF\x02\x01\x01"
            || u16::from_le_bytes([bytes[18], bytes[19]]) != 183
            || u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) != 1
            || u16::from_le_bytes([bytes[52], bytes[53]]) != 64)
    {
        return Err(invalid(format!(
            "artifact is not a valid AArch64 ELF: {}",
            path.display()
        )));
    }
    Ok(())
}
fn verify_tree(root: &Path, relative: &Path, expected: &BTreeSet<PathBuf>) -> Result<()> {
    let mut directories = vec![relative.to_path_buf()];
    let mut count = 0_u32;
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(root.join(&directory))? {
            let entry = entry?;
            count = count
                .checked_add(1)
                .ok_or_else(|| invalid("member count overflow"))?;
            if count > 20000 {
                return Err(invalid("too many package members"));
            }
            let path = directory.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                if !expected.iter().any(|file| file.starts_with(&path)) {
                    return Err(invalid("unexpected package directory"));
                }
                directories.push(path);
            } else if !expected.contains(&path) {
                return Err(invalid(format!(
                    "unexpected package member: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}
