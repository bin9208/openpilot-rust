//! Native preparation for fixed independent model-family reference data.
//! Compiler execution remains an explicit, separately tracked compiler-family dependency.
mod catalog;
mod usb;
use crate::seal::Result;
pub use catalog::{catalog_cli, prepare_catalog, CatalogKind, CatalogOptions};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{FileExt, MetadataExt},
    path::{Component, Path, PathBuf},
};
pub use usb::{stage_usb_assets, usb_cli, UsbOptions};
fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn relative(name: &str) -> Result<&Path> {
    require(
        !name.is_empty()
            && name
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != "..")
            && Path::new(name)
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
        "unsafe model preparation path",
    )?;
    Ok(Path::new(name))
}
fn real_parents(path: &Path) -> Result<()> {
    require(
        path.is_absolute(),
        "model preparation paths must be absolute",
    )?;
    for p in path.ancestors().skip(1) {
        let m = fs::symlink_metadata(p)?;
        require(
            m.is_dir() && !m.file_type().is_symlink(),
            "model preparation parent alias",
        )?;
    }
    Ok(())
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
struct Input {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    seal: Value,
    max: u64,
}
impl Input {
    fn open(path: &Path, max: u64) -> Result<Self> {
        real_parents(path)?;
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let file = File::from(fd);
        let m = file.metadata()?;
        require(
            m.is_file() && m.len() <= max,
            "preparation input is not bounded regular data",
        )?;
        let mut result = Self {
            path: path.to_owned(),
            file,
            identity: identity(&m),
            seal: Value::Null,
            max,
        };
        result.seal = result.hash()?;
        result.verify()?;
        Ok(result)
    }
    fn hash(&self) -> Result<Value> {
        let length = self.file.metadata()?.len();
        require(length <= self.max, "preparation input grew")?;
        let mut digest = Sha256::new();
        let mut offset = 0;
        let mut bytes = [0; 65536];
        while offset < length {
            let count = ((length - offset) as usize).min(bytes.len());
            let n = self.file.read_at(&mut bytes[..count], offset)?;
            require(n > 0, "preparation input truncated")?;
            digest.update(&bytes[..n]);
            offset += n as u64;
        }
        Ok(json!({"bytes":length,"sha256":format!("{:x}",digest.finalize())}))
    }
    fn verify(&self) -> Result<()> {
        real_parents(&self.path)?;
        require(
            identity(&self.file.metadata()?) == self.identity
                && identity(&fs::symlink_metadata(&self.path)?) == self.identity
                && self.hash()? == self.seal,
            "preparation held input changed",
        )
    }
    fn expect(&self, bytes: u64, sha: &str) -> Result<()> {
        require(
            self.seal == json!({"bytes":bytes,"sha256":sha}),
            "preparation immutable input pin mismatch",
        )
    }
    fn copy_to(&self, path: &Path) -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut offset = 0;
        let mut bytes = [0; 65536];
        while offset < self.identity.2 {
            let count = ((self.identity.2 - offset) as usize).min(bytes.len());
            let n = self.file.read_at(&mut bytes[..count], offset)?;
            require(n > 0, "preparation input truncated while copying")?;
            file.write_all(&bytes[..n])?;
            offset += n as u64;
        }
        file.flush()?;
        self.verify()
    }
}
fn json_file(path: &Path) -> Result<Value> {
    let input = Input::open(path, 2 * 1024 * 1024)?;
    let mut bytes = Vec::new();
    input
        .file
        .try_clone()?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    input.verify()?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
