//! Portable, authenticated original-model data. PKLs are hashed/copied, never executed.
use super::{read_regular, relative, sha, Original};
use crate::seal::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
fn fingerprint(path: &Path) -> Result<Value> {
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?;
    let mut file = File::from(fd);
    let before = file.metadata()?;
    if !before.is_file() || before.len() > MAX_FILE {
        return Err("fixture member not bounded regular file".into());
    }
    let mut hash = Sha256::new();
    let mut length = 0;
    let mut chunk = [0; 65536];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        length += n as u64;
        if length > before.len() {
            return Err("fixture member grew".into());
        }
        hash.update(&chunk[..n]);
    }
    let after = file.metadata()?;
    let path_after = fs::symlink_metadata(path)?;
    let identity = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    if length != before.len()
        || identity(&before) != identity(&after)
        || path_after.file_type().is_symlink()
        || identity(&after) != identity(&path_after)
    {
        return Err("fixture member changed during read".into());
    }
    Ok(json!({"bytes":length,"sha256":format!("{:x}",hash.finalize())}))
}
fn inventory(root: &Path) -> Result<BTreeMap<String, Value>> {
    fn visit(
        root: &Path,
        dir: &Path,
        files: &mut BTreeMap<String, Value>,
        total: &mut u64,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root)?;
            if matches!(
                path.extension().and_then(|v| v.to_str()),
                Some("py" | "pyc")
            ) {
                return Err("fixture must not contain Python tools".into());
            }
            if rel == Path::new("fixture.json") {
                continue;
            }
            let first = rel.components().next().ok_or("empty fixture member")?;
            if !["models", "catalog", "observations"]
                .iter()
                .any(|n| first.as_os_str() == *n)
            {
                return Err("unexpected fixture top-level member".into());
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err("fixture contains symlink".into());
            }
            if kind.is_dir() {
                visit(root, &path, files, total)?;
            } else if kind.is_file() {
                let name = rel.to_str().ok_or("fixture member is not UTF-8")?;
                relative(name)?;
                let pin = fingerprint(&path)?;
                *total = total
                    .checked_add(pin["bytes"].as_u64().ok_or("fixture size missing")?)
                    .ok_or("fixture total overflow")?;
                if *total > MAX_TOTAL || files.len() >= 8000 {
                    return Err("fixture tree exceeds bound".into());
                }
                files.insert(name.to_string(), pin);
            } else {
                return Err("fixture contains special file".into());
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files, &mut 0)?;
    Ok(files)
}
/// Held independently sealed fixture. Catalog and observations remain immutable
/// across native checkout/private-root runs, and every PKL stays an opaque blob.
pub struct Fixture {
    root: PathBuf,
    identity: (u64, u64),
    digest: String,
    manifest: Value,
    original: Original,
}
impl Fixture {
    pub fn open(root: &Path, expected: &str) -> Result<Self> {
        let meta = fs::symlink_metadata(root)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("fixture root not a real directory".into());
        }
        let raw = read_regular(&root.join("fixture.json"), 2_000_000)?;
        if sha(&raw) != expected {
            return Err("fixture manifest SHA-256 changed".into());
        }
        let manifest: Value = serde_json::from_slice(&raw)?;
        if manifest["version"] != 1
            || manifest["scope"]
                != "original-only compiled-model fixture; no native expected outputs"
        {
            return Err("fixture scope/version invalid".into());
        }
        let source = manifest["source_commit"]
            .as_str()
            .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("fixture source commit missing")?;
        let observed = inventory(root)?;
        let expected_files: BTreeMap<String, Value> =
            serde_json::from_value(manifest["files"].clone())?;
        if observed != expected_files {
            return Err("fixture file roster or bytes changed".into());
        }
        let original_hash = manifest["original_manifest_sha256"]
            .as_str()
            .ok_or("fixture original digest missing")?;
        let original = Original::open(&root.join("observations"), original_hash)?;
        if original.manifest()["checked_out_commit"] != source {
            return Err("fixture original source commit changed".into());
        }
        let model = observed
            .get("models/driving_tinygrad.pkl")
            .ok_or("fixture compiled driving model missing")?;
        if model["sha256"] != original.manifest()["model"]["sha256"]
            || model["bytes"] != original.manifest()["model"]["bytes"]
        {
            return Err("fixture original PKL differs from captured model".into());
        }
        let pointer: Value = serde_json::from_slice(&read_regular(
            &root.join("catalog/current.json"),
            1_000_000,
        )?)?;
        let generation = pointer["generation"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("fixture catalog generation invalid")?;
        let index_path = format!("catalog/{generation}/index.json");
        let index_raw = read_regular(&root.join(&index_path), 1_000_000)?;
        if sha(&index_raw) != generation || manifest["catalog_generation"] != generation {
            return Err("fixture catalog index digest changed".into());
        }
        let index: Value = serde_json::from_slice(&index_raw)?;
        for camera in [[1344, 760], [1928, 1208]] {
            let bundles = index["bundles"]
                .as_array()
                .ok_or("fixture catalog bundles missing")?;
            let rows = bundles
                .iter()
                .filter(|b| b["kind"] == "driving" && b["camera"] == json!(camera))
                .collect::<Vec<_>>();
            if rows.len() != 1 {
                return Err("fixture driving resolution missing/duplicated".into());
            }
            let directory = rows[0]["directory"]
                .as_str()
                .ok_or("fixture bundle directory missing")?;
            relative(directory)?;
            let prefix = format!("catalog/{generation}/{directory}/");
            let descriptor_name = prefix.clone() + "pipeline.json";
            let descriptor_raw = read_regular(&root.join(&descriptor_name), 4_000_000)?;
            if rows[0]["pipeline_sha256"] != sha(&descriptor_raw) {
                return Err("fixture pipeline index pin changed".into());
            }
            let descriptor: Value = serde_json::from_slice(&descriptor_raw)?;
            if descriptor["sources"]["driving"] != model["sha256"]
                || descriptor["camera"] != json!(camera)
                || descriptor["kind"] != "driving"
            {
                return Err("fixture catalog source differs from original PKL".into());
            }
            for name in ["graph.json", "weights.bin", "kernels.so"] {
                if !observed.contains_key(&(prefix.clone() + name)) {
                    return Err(format!("fixture native bundle file missing: {name}").into());
                }
            }
        }

        // Every catalog bundle, including both driver warps, must retain its
        // complete executable graph, weight bytes and descriptor index pins.
        let bundles = index["bundles"]
            .as_array()
            .ok_or("fixture bundles missing")?;
        if bundles.len() != 4 {
            return Err("fixture full four-bundle catalog missing".into());
        }

        for camera in [[1344, 760], [1928, 1208]] {
            let rows = bundles
                .iter()
                .filter(|bundle| bundle["kind"] == "driver" && bundle["camera"] == json!(camera))
                .collect::<Vec<_>>();
            if rows.len() != 1 {
                return Err("fixture driver resolution missing/duplicated".into());
            }
            let directory = rows[0]["directory"]
                .as_str()
                .ok_or("fixture driver directory missing")?;
            relative(directory)?;
            let descriptor: Value = serde_json::from_slice(&read_regular(
                &root.join(format!("catalog/{generation}/{directory}/pipeline.json")),
                4_000_000,
            )?)?;
            for (role, name) in [
                ("model", "dmonitoring_model_tinygrad.pkl".to_owned()),
                ("metadata", "dmonitoring_model_metadata.pkl".to_owned()),
                (
                    "warp",
                    format!("dm_warp_{}x{}_tinygrad.pkl", camera[0], camera[1]),
                ),
            ] {
                let file = observed
                    .get(&format!("models/{name}"))
                    .ok_or("fixture compiled driver model file missing")?;
                if descriptor["sources"][role] != file["sha256"] {
                    return Err("fixture driver catalog PKL/metadata source changed".into());
                }
            }
        }
        for bundle in bundles {
            let directory = bundle["directory"]
                .as_str()
                .ok_or("fixture bundle directory missing")?;
            relative(directory)?;
            let prefix = format!("catalog/{generation}/{directory}/");
            for (name, pin) in [
                ("pipeline.json", "pipeline_sha256"),
                ("graph.json", "graph_sha256"),
            ] {
                let bytes = read_regular(&root.join(prefix.clone() + name), 4_000_000)?;
                if bundle[pin] != sha(&bytes) {
                    return Err("fixture indexed bundle pin changed".into());
                }
            }
            let graph: Value = serde_json::from_slice(&read_regular(
                &root.join(prefix.clone() + "graph.json"),
                4_000_000,
            )?)?;
            for (name, pin) in [
                ("weights.bin", "weights_sha256"),
                ("kernels.so", "library_sha256"),
            ] {
                let file = observed
                    .get(&(prefix.clone() + name))
                    .ok_or("fixture executable bundle file missing")?;
                if graph[pin] != file["sha256"] {
                    return Err("fixture graph payload pin changed".into());
                }
            }
        }
        Ok(Self {
            root: root.to_owned(),
            identity: (meta.dev(), meta.ino()),
            digest: expected.to_owned(),
            manifest,
            original,
        })
    }
    pub fn catalog(&self) -> PathBuf {
        self.root.join("catalog")
    }
    pub fn original_directory(&self) -> PathBuf {
        self.root.join("observations")
    }
    pub fn original_manifest_sha256(&self) -> &str {
        self.manifest["original_manifest_sha256"]
            .as_str()
            .expect("validated fixture original digest")
    }
    pub fn verify(&self) -> Result<()> {
        let now = fs::symlink_metadata(&self.root)?;
        if !now.is_dir() || now.file_type().is_symlink() || (now.dev(), now.ino()) != self.identity
        {
            return Err("fixture root changed".into());
        }
        self.original.verify()?;
        Self::open(&self.root, &self.digest).map(|_| ())
    }
}
/// Archive all compiled model files, the complete native catalog and independent
/// observations. An accepted hosted gate and external manifest/ZIP pins are still
/// required before this candidate fixture becomes a maintained input.
pub fn capture(
    output: &Path,
    models: &Path,
    catalog: &Path,
    observations: &Path,
    source: &str,
) -> Result<String> {
    if output.exists() {
        return Err("fixture output already exists".into());
    }
    let parent = output.parent().ok_or("fixture output parent missing")?;
    let destination = parent
        .canonicalize()?
        .join(output.file_name().ok_or("fixture output name missing")?);
    for input in [models, catalog, observations] {
        let input = input.canonicalize()?;
        if destination.starts_with(&input) || input.starts_with(&destination) {
            return Err("fixture output overlaps its inputs".into());
        }
    }

    openpilot_disk_space::inspect(parent, MAX_TOTAL)?.require()?;
    fs::create_dir(output)?;
    fn copy_tree(from: &Path, to: &Path, total: &mut u64, count: &mut usize) -> Result<()> {
        let meta = fs::symlink_metadata(from)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("fixture input directory invalid".into());
        }
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let src = entry.path();
            let dst = to.join(entry.file_name());
            *count += 1;
            *total = total
                .checked_add(4096)
                .ok_or("fixture entry size overflow")?;
            if *count > 8000 || *total > MAX_TOTAL {
                return Err("fixture entry budget exceeded".into());
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                copy_tree(&src, &dst, total, count)?;
            } else if kind.is_file() {
                let before = fingerprint(&src)?;
                *total = total
                    .checked_add(before["bytes"].as_u64().ok_or("fixture size missing")?)
                    .ok_or("fixture copy size overflow")?;
                if *total > MAX_TOTAL || *count > 8000 {
                    return Err("fixture copy exceeds reserved bound".into());
                }
                let fd = rustix::fs::open(
                    &src,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::NONBLOCK
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?;
                let file = File::from(fd);
                let permissions = file.metadata()?.permissions();
                let length = before["bytes"].as_u64().ok_or("fixture size missing")?;
                let mut input = file.take(length + 1);
                let mut destination = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&dst)?;
                if std::io::copy(&mut input, &mut destination)? != length {
                    return Err("fixture input changed during copy".into());
                }
                fs::set_permissions(&dst, permissions)?;
                if fingerprint(&src)? != before || fingerprint(&dst)? != before {
                    return Err("fixture input changed while copying".into());
                }
            } else {
                return Err("fixture input contains link or special file".into());
            }
        }
        Ok(())
    }
    let mut copied = 0;
    let mut count = 0;
    copy_tree(models, &output.join("models"), &mut copied, &mut count)?;
    copy_tree(catalog, &output.join("catalog"), &mut copied, &mut count)?;
    copy_tree(
        observations,
        &output.join("observations"),
        &mut copied,
        &mut count,
    )?;
    let files = inventory(output)?;
    let original_hash = sha(&read_regular(
        &output.join("observations/manifest.json"),
        1_000_000,
    )?);
    let pointer: Value = serde_json::from_slice(&read_regular(
        &output.join("catalog/current.json"),
        1_000_000,
    )?)?;
    let manifest = json!({"version":1,"scope":"original-only compiled-model fixture; no native expected outputs","source_commit":source,"original_manifest_sha256":original_hash,"catalog_generation":pointer["generation"],"files":files});
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    let digest = sha(&bytes);
    use std::io::Write;
    let manifest_path = output.join("fixture.json");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&manifest_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    if let Err(error) = Fixture::open(output, &digest).and_then(|fixture| fixture.verify()) {
        fs::remove_file(manifest_path)?;
        return Err(error);
    }
    Ok(digest)
}
/// CLI implementation for parent registration as seal-driving-daemon-fixture.
pub fn cli() -> Result<()> {
    let mut values = std::collections::BTreeMap::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(key) = args.next() {
        let key = key.into_string().map_err(|_| "fixture option not UTF-8")?;
        if ![
            "--output",
            "--models",
            "--catalog",
            "--original-dir",
            "--source-commit",
            "--verify-sha",
        ]
        .contains(&key.as_str())
        {
            return Err("unknown fixture option".into());
        }
        let value = args.next().ok_or("fixture option value missing")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate fixture option".into());
        }
    }
    let path = |name: &str| -> Result<PathBuf> {
        Ok(PathBuf::from(
            values.get(name).ok_or("required fixture option missing")?,
        ))
    };
    let output = path("--output")?;
    if values.contains_key("--verify-sha") && values.len() != 2 {
        return Err("fixture verification accepts only output and verify-sha".into());
    }
    let digest = if let Some(digest) = values.get("--verify-sha") {
        let digest = digest.to_str().ok_or("fixture digest not UTF-8")?;
        Fixture::open(&output, digest)?.verify()?;
        digest.to_owned()
    } else {
        let source = values
            .get("--source-commit")
            .and_then(|v| v.to_str())
            .ok_or("fixture source commit missing")?;
        capture(
            &output,
            &path("--models")?,
            &path("--catalog")?,
            &path("--original-dir")?,
            source,
        )?
    };
    let fixture = Fixture::open(&output, &digest)?;
    println!(
        "{}",
        json!({"fixture":output,"manifest_sha256":digest,"catalog":fixture.catalog(),
        "original_dir":fixture.original_directory(),"original_manifest_sha256":fixture.original_manifest_sha256(),"pkls_executed":false})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_rejects_links_and_detects_same_size_byte_mutation() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("models")).unwrap();
        let path = root.path().join("models/data.pkl");
        fs::write(&path, b"abc").unwrap();
        let first = inventory(root.path()).unwrap();
        fs::write(&path, b"abd").unwrap();
        assert_ne!(first, inventory(root.path()).unwrap());
        std::os::unix::fs::symlink("missing", root.path().join("catalog")).unwrap();
        assert!(inventory(root.path()).is_err());
    }
    #[test]
    fn manifest_pin_and_incomplete_fixture_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("fixture.json"), b"{}").unwrap();
        assert!(Fixture::open(root.path(), &"0".repeat(64)).is_err());
        assert!(Fixture::open(root.path(), &sha(b"{}")).is_err());
    }
}
