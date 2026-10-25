//! Only the owned two checkers may be absent; shared device_probe stays live.
use crate::{
    proclog_reference_check::inputs::{Bound, Sources as ProcSources},
    seal::Result,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
const CHECKER: &str = "rust/tools/check_aarch64.py";
const PIN: &str = "9821fb198619e6396f773e80605d34cd90bab94b0f4945410a140fa15a47c030";
pub(super) struct Sources {
    root: PathBuf,
    proc: ProcSources,
    helper: Option<Bound>,
    device: Bound,
}
impl Sources {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let helper = match fs::symlink_metadata(root.join(CHECKER)) {
            Ok(_) => Some(Bound::open(&root.join(CHECKER), Some(PIN))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            root: root.into(),
            proc: ProcSources::open(root)?,
            helper,
            device: Bound::open(
                &root.join("rust/tools/device_probe.py"),
                Some("f838b79cbaaa53d3c07f386c4cea42259db324dd1e61a6aff9167541c4b32e1f"),
            )?,
        })
    }
    pub(super) fn aliases(&self, path: &Path) -> Result<bool> {
        Ok(self.proc.aliases(path)?
            || self.device.aliases(path)?
            || self
                .helper
                .as_ref()
                .map(|helper| helper.aliases(path))
                .transpose()?
                .unwrap_or(false))
    }
    pub(super) fn verify(&self) -> Result<()> {
        self.proc.verify()?;
        if let Some(helper) = &self.helper {
            helper.verify_path(&self.root.join(CHECKER))?;
        } else if !matches!(fs::symlink_metadata(self.root.join(CHECKER)),Err(e)if e.kind()==std::io::ErrorKind::NotFound)
        {
            return Err("AArch64 historical helper reappeared".into());
        }
        self.device
            .verify_path(&self.root.join("rust/tools/device_probe.py"))
    }
}
