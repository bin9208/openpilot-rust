use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
fn id(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
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
pub(crate) struct Bound {
    path: PathBuf,
    file: File,
    metadata: Metadata,
    pub(crate) hash: String,
}
impl Bound {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| format!("opening statistics input {}: {error}", path.display()))?;
        let mut file = File::from(fd);
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("statistics input is not a regular file".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        let result = Self {
            path: path.into(),
            file,
            metadata,
            hash: format!("{:x}", hash.finalize()),
        };
        result.verify()?;
        Ok(result)
    }
    pub(crate) fn verify(&self) -> Result<()> {
        if id(&self.metadata) != id(&self.file.metadata()?)
            || id(&self.metadata) != id(&fs::metadata(&self.path)?)
        {
            return Err(
                format!("statistics checker input changed: {}", self.path.display()).into(),
            );
        }
        Ok(())
    }
    fn bytes(&self) -> Result<Vec<u8>> {
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![];
        file.read_to_end(&mut bytes)?;
        self.verify()?;
        Ok(bytes)
    }
}
pub(super) struct Inputs {
    files: Vec<Bound>,
    absent: Vec<PathBuf>,
    pub(super) hashes: Value,
}
impl Inputs {
    pub(super) fn open(
        root: &Path,
        binding: &Path,
        binaries: &Path,
        worker: &Path,
    ) -> Result<Self> {
        Self::open_with(
            root,
            binding,
            &[
                ("checker", worker.to_owned()),
                ("stats_daemon", binaries.join("examples/stats_daemon")),
                ("stats_producer", binaries.join("examples/stats_producer")),
            ],
            None,
        )
    }
    pub(super) fn open_with(
        root: &Path,
        binding: &Path,
        artifacts: &[(&str, PathBuf)],
        extra_pins: Option<&Value>,
    ) -> Result<Self> {
        let manifest: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/stats-daemon/manifest.json"
        ))?;
        let params: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/stats-daemon/params-sources.json"
        ))?;
        let mut pins = manifest["source_sha256"]
            .as_object()
            .ok_or("source pins missing")?
            .clone();
        pins.extend(params.as_object().ok_or("Params pins missing")?.clone());
        if let Some(extra) = extra_pins {
            pins.extend(
                extra
                    .as_object()
                    .ok_or("additional source pins missing")?
                    .clone(),
            );
        }
        let mut files = vec![];
        let mut absent = vec![];
        let mut hashes = serde_json::Map::new();
        for (name, expected) in pins {
            let path = root.join(&name);
            if matches!(
                name.as_str(),
                "rust/tools/check_stats_daemon.py"
                    | "rust/tools/check_stats_failures.py"
                    | "rust/tools/check_stats_log_fault.py"
                    | "rust/tools/check_stats_interrupted.py"
                    | "rust/tools/stats_source.py"
            ) && fs::symlink_metadata(&path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                absent.push(path);
                continue;
            }
            let input = Bound::open(&path)?;
            if expected.as_str() != Some(&input.hash) {
                return Err(format!("statistics original source changed: {name}").into());
            }
            hashes.insert(name, json!(input.hash));
            files.push(input);
        }
        let binding_file = Bound::open(binding)?;
        let receipt = Bound::open(
            &binding
                .parent()
                .ok_or("binding parent missing")?
                .join("provenance.json"),
        )?;
        let provenance: Value = serde_json::from_slice(&receipt.bytes()?)?;
        if provenance["result"] != "pass"
            || provenance["module_sha256"] != binding_file.hash
            || provenance["sources"] != params
        {
            return Err("original Params binding provenance differs".into());
        }
        hashes.insert("binding".into(), json!(binding_file.hash));
        files.push(binding_file);
        files.push(receipt);
        for (name, path) in artifacts {
            let input = Bound::open(path)?;
            hashes.insert((*name).into(), json!(input.hash));
            files.push(input);
        }
        Ok(Self {
            files,
            absent,
            hashes: Value::Object(hashes),
        })
    }
    pub(super) fn verify(&self) -> Result<()> {
        for file in &self.files {
            file.verify()?;
        }
        for path in &self.absent {
            match fs::symlink_metadata(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err("retired statistics helper reappeared".into()),
            }
        }
        Ok(())
    }
}
