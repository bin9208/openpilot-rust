//! Complete data/source gates shared by composition and source-built sealing.
use crate::{regular_file, source, Result};
use std::{
    collections::BTreeMap,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

/// Required ordinary Git assets; LFS data and independent Panda firmware have
/// their own source contracts. Never admit an arbitrary user checkout snapshot.
pub const ORDINARY: &[&str] = &[
    "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
    "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
    "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
    "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
    "openpilot/selfdrive/assets/images/speed_bg.png",
    "openpilot/selfdrive/assets/images/traffic_green.png",
    "openpilot/selfdrive/assets/images/traffic_red.png",
    "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
];

pub fn verify(source: &Path, provider: &Path, files: &BTreeMap<PathBuf, PathBuf>) -> Result<()> {
    let before = source::inspect(source)?;
    crate::source_lfs::verify_layout(source, provider, files)?;
    crate::model_inputs::verify_mapped(source, provider, files)?;
    crate::panda_inputs::verify_mapped(source, provider, files)?;
    crate::mpc_inputs::verify_mapped(source, provider, files)?;
    crate::dbc_inputs::verify_mapped(source, provider, files)?;
    crate::usbgpu_inputs::verify_mapped(source, provider, files)?;
    let provider = provider.canonicalize()?;
    for name in ORDINARY {
        let result = Command::new("/usr/bin/git")
            .arg("-C")
            .arg(source)
            .args(["show", &format!("HEAD:{name}")])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()?;
        if !result.status.success()
            || result.stdout.len() > 64 * 1024 * 1024
            || result
                .stdout
                .starts_with(b"version https://git-lfs.github.com/spec/v1\n")
        {
            return Err(
                "required ordinary runtime asset is absent or no longer an ordinary Git blob"
                    .into(),
            );
        }
        let relative = files
            .get(Path::new(name))
            .ok_or("runtime layout omitted an ordinary source asset")?;
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("unsafe ordinary runtime asset mapping".into());
        }
        let path = provider.join(relative).canonicalize()?;
        if !path.starts_with(&provider)
            || regular_file(&path)?.metadata()?.len() != result.stdout.len() as u64
            || regular_file(&path)?.metadata()?.permissions().mode() & 0o7777 != 0o444
        {
            return Err("ordinary source asset escaped its provider or changed length".into());
        }
        let mut actual = Vec::new();
        regular_file(&path)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut actual)?;
        if actual != result.stdout {
            return Err("ordinary runtime asset differs from its current Git blob".into());
        }
    }
    if source::inspect(source)? != before {
        return Err("source changed during complete runtime layout verification".into());
    }
    Ok(())
}
