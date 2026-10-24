//! Continuous ELF identity and bytes through the complete comparison.
use super::{require, save, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Path, PathBuf},
};
struct Provider {
    path: PathBuf,
    held: File,
}
impl Provider {
    fn open(path: &Path) -> Result<Self> {
        let held = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        )?);
        require(
            held.metadata()?.is_file(),
            "logger provider is not a regular file",
        )?;
        let mut header = [0_u8; 64];
        held.read_exact_at(&mut header, 0)?;
        let machine = match std::env::consts::ARCH {
            "x86_64" => 62,
            "aarch64" => 183,
            _ => return Err("unsupported logger QA host architecture".into()),
        };
        require(
            &header[..6] == b"\x7fELF\x02\x01"
                && u16::from_le_bytes([header[18], header[19]]) == machine,
            "logger provider is not a host ELF64 executable",
        )?;
        Ok(Self {
            path: path.into(),
            held,
        })
    }
    fn snapshot(&self) -> Result<Value> {
        let held = self.held.metadata()?;
        let path = fs::symlink_metadata(&self.path)?;
        require(
            (held.dev(), held.ino()) == (path.dev(), path.ino()),
            "logger provider path identity changed",
        )?;
        Ok(
            json!({"path":self.path,"dev":held.dev(),"ino":held.ino(),"size":held.len(),"mode":held.mode(),"mtime":held.mtime(),"mtime_nsec":held.mtime_nsec(),"ctime":held.ctime(),"ctime_nsec":held.ctime_nsec(),"sha256":crate::seal::digest(Path::new(&format!("/proc/self/fd/{}",self.held.as_raw_fd())))?}),
        )
    }
}
pub(super) struct Providers {
    held: Vec<Provider>,
    before: Value,
    output: PathBuf,
    verified: bool,
}
impl Providers {
    pub fn new(paths: &[&Path], output: &Path) -> Result<Self> {
        let held = paths
            .iter()
            .map(|path| Provider::open(path))
            .collect::<Result<Vec<_>>>()?;
        let before = json!(held
            .iter()
            .map(Provider::snapshot)
            .collect::<Result<Vec<_>>>()?);
        save(&output.join("providers.before.json"), &before)?;
        Ok(Self {
            held,
            before,
            output: output.into(),
            verified: false,
        })
    }
    pub fn verify(&mut self) -> Result<()> {
        let after = json!(self
            .held
            .iter()
            .map(Provider::snapshot)
            .collect::<Result<Vec<_>>>()?);
        save(&self.output.join("providers.after.json"), &after)?;
        require(
            after == self.before,
            "logger provider changed during actual traffic",
        )?;
        self.verified = true;
        Ok(())
    }
}
impl Drop for Providers {
    fn drop(&mut self) {
        if !self.verified {
            if let Err(e) = self.verify() {
                eprintln!(
                    "logger QA held provider verification: {}",
                    e.to_string().chars().take(512).collect::<String>()
                );
            }
        }
    }
}
