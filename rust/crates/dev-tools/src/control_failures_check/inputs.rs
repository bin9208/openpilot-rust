//! Held, bounded inputs for this controller; no executable/provider fallback.
use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    os::unix::fs::{FileExt, MetadataExt},
    path::{Path, PathBuf},
};
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
fn hash(file: &File) -> Result<String> {
    let length = file.metadata()?.len();
    if length > 256 << 20 {
        return Err("input exceeds256MiB".into());
    }
    let mut sum = Sha256::new();
    let mut bytes = [0; 65536];
    let mut offset = 0;
    while offset < length {
        let n = file.read_at(&mut bytes[..(length - offset).min(65536) as usize], offset)?;
        if n == 0 {
            return Err("held input truncated".into());
        }
        sum.update(&bytes[..n]);
        offset += n as u64;
    }
    Ok(format!("{:x}", sum.finalize()))
}
struct Input {
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    hash: String,
}
#[derive(Default)]
pub(super) struct Inputs {
    files: BTreeMap<PathBuf, Input>,
    trees: BTreeMap<PathBuf, Vec<PathBuf>>,
}
impl Inputs {
    pub fn add(&mut self, path: &Path) -> Result<()> {
        if self.files.contains_key(path) {
            return Ok(());
        }
        if self.files.len() >= 8192 {
            return Err("held file count8192 exceeded".into());
        }
        for parent in path.ancestors().skip(1) {
            if !fs::symlink_metadata(parent)?.is_dir() {
                return Err("held input parent alias/special file".into());
            }
        }
        let file = File::from(rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        let m = file.metadata()?;
        if !m.is_file() {
            return Err("held input must be regular".into());
        }
        let hash = hash(&file)?;
        self.files.insert(
            path.to_owned(),
            Input {
                file,
                identity: identity(&m),
                hash,
            },
        );
        if self.files.values().map(|v| v.identity.2).sum::<u64>() > 1024 << 20 {
            return Err("held input total exceeds1GiB".into());
        }
        self.verify_file(path)
    }
    fn members(root: &Path) -> Result<Vec<PathBuf>> {
        fn visit(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
            if out.len() >= 8192 {
                return Err("input tree count8192 exceeded".into());
            }
            let m = fs::symlink_metadata(path)?;
            if m.is_file() {
                out.push(path.to_owned());
            } else if m.is_dir() {
                out.push(path.to_owned());
                for row in fs::read_dir(path)? {
                    visit(&row?.path(), out)?;
                }
            } else {
                return Err("input tree alias/special file".into());
            }
            Ok(())
        }
        let mut out = Vec::new();
        visit(root, &mut out)?;
        out.sort();
        Ok(out)
    }
    pub fn tree(&mut self, root: &Path) -> Result<()> {
        let names = Self::members(root)?;
        for path in &names {
            if fs::symlink_metadata(path)?.is_file() {
                self.add(path)?;
            }
        }
        self.trees.insert(root.to_owned(), names);
        Ok(())
    }
    fn verify_file(&self, path: &Path) -> Result<()> {
        let own = &self.files[path];
        let m = fs::symlink_metadata(path)?;
        if !m.is_file()
            || identity(&m) != own.identity
            || identity(&own.file.metadata()?) != own.identity
            || hash(&own.file)? != own.hash
        {
            return Err(format!("held input changed: {}", path.display()).into());
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<()> {
        for path in self.files.keys() {
            self.verify_file(path)?;
        }
        for (root, names) in &self.trees {
            if Self::members(root)? != *names {
                return Err("input tree roster changed".into());
            }
        }
        Ok(())
    }
    pub fn report(&self) -> Value {
        let files = self
            .files
            .keys()
            .map(|path| {
                let actual = (|| -> Result<Value> {
                    let file = File::from(rustix::fs::open(
                        path,
                        rustix::fs::OFlags::RDONLY
                            | rustix::fs::OFlags::NOFOLLOW
                            | rustix::fs::OFlags::NONBLOCK
                            | rustix::fs::OFlags::CLOEXEC,
                        rustix::fs::Mode::empty(),
                    )?);
                    if !file.metadata()?.is_file() {
                        return Err("snapshot input nonregular".into());
                    }
                    Ok(json!({"identity":identity(&file.metadata()?),"sha256":hash(&file)?}))
                })();
                (
                    path.to_string_lossy().into_owned(),
                    actual.unwrap_or_else(|e| json!({"error":e.to_string()})),
                )
            })
            .collect::<BTreeMap<_, _>>();
        json!({"files":files,"trees":self.trees.iter().map(|(root,_)|(root.to_string_lossy().into_owned(),Self::members(root).map(|rows|json!(rows)).unwrap_or_else(|e|json!({"error":e.to_string()})))).collect::<BTreeMap<_,_>>()})
    }
}
