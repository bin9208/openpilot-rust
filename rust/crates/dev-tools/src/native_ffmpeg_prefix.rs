//! One FFMPEG_DIR from independently authenticated 6.1.1 headers and providers.
use crate::{
    encoder_stage,
    native_codec_inputs::{
        directory, hash, json, publish_tree, regular, write, Materialized, Prepared, Published,
    },
    native_ffmpeg_headers, native_provider,
    seal::Result,
};
use openpilot_runtime_build::{
    assets, digest_file,
    inputs::{Entry, Tree},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const PROVENANCE: &str = "provenance/native-ffmpeg-prefix.json";

pub(crate) struct Part {
    pub(crate) root: PathBuf,
    pub(crate) receipt: PathBuf,
    pub(crate) sha256: String,
    pub(crate) tree: Tree,
}
impl Part {
    pub(crate) fn read(prepared: &Prepared) -> Result<Self> {
        let part = Self {
            root: prepared.root.clone(),
            receipt: prepared.tree_receipt.clone(),
            sha256: prepared.tree_sha256.clone(),
            tree: Tree::read(&prepared.tree_receipt)?,
        };
        part.verify()?;
        Ok(part)
    }
    fn verify(&self) -> Result<()> {
        if hash(&json(&self.tree)?) != self.sha256 || digest_file(&self.receipt)? != self.sha256 {
            return Err("native prefix input receipt changed".into());
        }
        self.tree.verify(&self.root)
    }
}

struct Composition {
    parts: [Part; 2],
    tree: Tree,
    sources: BTreeMap<PathBuf, PathBuf>,
    metadata: BTreeMap<PathBuf, Vec<u8>>,
    identity: String,
}
impl Composition {
    fn new(parts: [Part; 2]) -> Result<Self> {
        let provenance = json(&(
            "openpilot-native-ffmpeg-prefix-v1",
            "6.1.1",
            "aarch64-unknown-linux-gnu",
            "19.8-carrot-bt1",
            &parts[0].sha256,
            &parts[1].sha256,
        ))?;
        let identity = hash(&provenance);
        Self::with_metadata(
            parts,
            BTreeMap::from([(PROVENANCE.into(), provenance)]),
            identity,
        )
    }
    fn with_metadata(
        parts: [Part; 2],
        metadata: BTreeMap<PathBuf, Vec<u8>>,
        identity: String,
    ) -> Result<Self> {
        let mut tree = Tree {
            schema: 1,
            entries: BTreeMap::new(),
        };
        let mut sources = BTreeMap::new();
        for part in &parts {
            part.verify()?;
            for (path, entry) in &part.tree.entries {
                encoder_stage::safe(path.to_str().ok_or("non-UTF8 native prefix path")?)?;
                if metadata.contains_key(path) {
                    return Err("native input occupies generated prefix metadata".into());
                }
                if let Some(previous) = tree.entries.get(path) {
                    if previous != entry {
                        return Err(
                            format!("conflicting native prefix input: {}", path.display()).into(),
                        );
                    }
                    continue;
                }
                let entry = match entry {
                    Entry::Directory => Entry::Directory,
                    Entry::File {
                        sha256,
                        bytes,
                        mode,
                    } if matches!(*mode, 0o444 | 0o555) => {
                        sources.insert(path.clone(), part.root.join(path));
                        Entry::File {
                            sha256: sha256.clone(),
                            bytes: *bytes,
                            mode: *mode,
                        }
                    }
                    Entry::File { .. } => {
                        return Err("native prefix input must be read-only".into())
                    }
                    Entry::Symlink { target } => Entry::Symlink {
                        target: target.clone(),
                    },
                };
                tree.entries.insert(path.clone(), entry);
            }
        }
        for (path, bytes) in &metadata {
            crate::native_codec_inputs::insert(
                &mut tree.entries,
                path.clone(),
                Entry::File {
                    sha256: hash(bytes),
                    bytes: bytes.len() as u64,
                    mode: 0o444,
                },
            )?;
        }
        Ok(Self {
            parts,
            tree,
            sources,
            metadata,
            identity,
        })
    }
    fn verify(&self) -> Result<()> {
        for part in &self.parts {
            part.verify()?;
        }
        Ok(())
    }
    fn materialize(&self, root: &Path) -> Result<(Tree, Materialized)> {
        let mut counts = Materialized::default();
        for (path, entry) in &self.tree.entries {
            let target = root.join(path);
            match entry {
                Entry::Directory => {
                    fs::create_dir(&target)?;
                }
                Entry::Symlink { target: link } => symlink(link, &target)?,
                Entry::File { bytes, mode, .. } => {
                    if let Some(data) = self.metadata.get(path) {
                        write(&target, data)?;
                        counts.copied_bytes += *bytes;
                        continue;
                    }
                    let source = self
                        .sources
                        .get(path)
                        .ok_or("missing native prefix source")?;
                    match fs::hard_link(source, &target) {
                        Ok(()) => {
                            counts.linked_bytes = counts
                                .linked_bytes
                                .checked_add(*bytes)
                                .ok_or("prefix linked bytes overflow")?
                        }
                        Err(error) if error.raw_os_error() == Some(18) => {
                            // EXDEV: copy exclusively; never chmod a hardlinked input.
                            let mut output = OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                                .open(&target)?;
                            if io::copy(
                                &mut regular(source)?.take(
                                    bytes.checked_add(1).ok_or("prefix input size overflow")?,
                                ),
                                &mut output,
                            )? != *bytes
                            {
                                return Err("native prefix input size changed during copy".into());
                            }
                            output.set_permissions(fs::Permissions::from_mode(*mode))?;
                            output.sync_all()?;
                            counts.copied_bytes = counts
                                .copied_bytes
                                .checked_add(*bytes)
                                .ok_or("prefix copied bytes overflow")?;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
        Ok((Tree::capture(root)?, counts))
    }
}

fn compose(parts: [Part; 2], store: &Path) -> Result<Published> {
    let composition = Composition::new(parts)?;
    publish(composition, store)
}
pub(crate) fn compose_with_metadata(
    parts: [Part; 2],
    store: &Path,
    metadata: BTreeMap<PathBuf, Vec<u8>>,
    identity: String,
) -> Result<Published> {
    publish(
        Composition::with_metadata(parts, metadata, identity)?,
        store,
    )
}
fn publish(composition: Composition, store: &Path) -> Result<Published> {
    let identity = composition.identity.clone();
    let (published, _) = publish_tree(
        identity,
        store,
        || Ok(composition),
        |context| Ok(context.tree.clone()),
        Composition::materialize,
        Composition::verify,
    )?;
    Ok(published)
}

#[derive(Debug, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub prefix: Published,
    pub headers: native_ffmpeg_headers::Report,
    pub provider: native_provider::Report,
}

pub fn cli(arguments: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut mode = false;
    let mut args = arguments.iter();
    while let Some(key) = args.next() {
        if key == "--prepare-released-prefix" && !mode {
            mode = true;
            continue;
        }
        if ![
            "--header-plan",
            "--provider-plan",
            "--descriptor",
            "--descriptor-sha256",
            "--header-cache",
            "--provider-cache",
            "--store",
        ]
        .contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing FFmpeg prefix argument")?,
                )
                .is_some()
        {
            return Err("invalid, duplicate or mixed FFmpeg prefix argument".into());
        }
    }
    let value = |key: &str| {
        values
            .get(key)
            .map(|v| v.as_str())
            .ok_or_else(|| format!("{key} required for verified FFmpeg prefix"))
    };
    if !mode {
        return Err("--prepare-released-prefix required".into());
    }
    let pin = value("--descriptor-sha256")?;
    if pin.len() != 64
        || !pin
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("independent --descriptor-sha256 is invalid".into());
    }
    let descriptor = Path::new(value("--descriptor")?);
    let header_cache = Path::new(value("--header-cache")?);
    let provider_cache = Path::new(value("--provider-cache")?);
    let store = Path::new(value("--store")?);
    let header_plan = native_ffmpeg_headers::Plan::read(Path::new(value("--header-plan")?))?;
    let provider_plan = native_provider::Plan::read(Path::new(value("--provider-plan")?))?;
    if provider_plan.profile != "ffmpeg611-shared" {
        return Err("FFmpeg prefix requires its original FFmpeg shared provider profile".into());
    }
    // Reject an unrelated descriptor before preparing either input.
    if digest_file(descriptor)? != pin {
        return Err("FFmpeg provider descriptor checksum mismatch".into());
    }
    directory(store)?;
    let provider = native_provider::prepare(
        &provider_plan,
        descriptor,
        pin,
        provider_cache,
        &store.join("providers"),
        None,
        &assets::Curl,
    )?;
    let headers = native_ffmpeg_headers::prepare(
        &header_plan,
        header_cache,
        &store.join("headers"),
        &native_ffmpeg_headers::ReleasedSourceDownload,
    )?;
    let prefix = compose(
        [
            Part::read(&headers.prepared)?,
            Part::read(&provider.prepared)?,
        ],
        &store.join("prefixes"),
    )?;
    println!(
        "{}",
        serde_json::to_string(&Report {
            prefix,
            headers,
            provider
        })?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn part(parent: &Path, name: &str, files: &[(&str, &[u8])]) -> Part {
        let root = parent.join(name);
        fs::create_dir(&root).unwrap();
        for (path, bytes) in files {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            write(&root.join(path), bytes).unwrap();
        }
        let tree = Tree::capture(&root).unwrap();
        let receipt = parent.join(format!("{name}.json"));
        let bytes = json(&tree).unwrap();
        write(&receipt, &bytes).unwrap();
        Part {
            root,
            receipt,
            sha256: hash(&bytes),
            tree,
        }
    }
    fn parts(parent: &Path) -> [Part; 2] {
        [
            part(
                parent,
                "headers",
                &[("include/av.h", b"header"), ("share/license", b"same")],
            ),
            part(
                parent,
                "provider",
                &[("lib/libav.so.1", b"library"), ("share/license", b"same")],
            ),
        ]
    }
    #[test]
    fn composes_complete_prefix_reuses_inodes_and_has_path_independent_identity() {
        let temp = tempfile::tempdir().unwrap();
        let [headers, mut provider] = parts(temp.path());
        symlink("libav.so.1", provider.root.join("lib/libav.so")).unwrap();
        provider.tree = Tree::capture(&provider.root).unwrap();
        provider.receipt = temp.path().join("provider-with-link.json");
        let bytes = json(&provider.tree).unwrap();
        write(&provider.receipt, &bytes).unwrap();
        provider.sha256 = hash(&bytes);
        let inputs = [headers, provider];
        let source = inputs[0].root.join("include/av.h");
        let identity = Composition::new(inputs).unwrap().identity;
        let reread = |name: &str, receipt: &str| Part {
            root: temp.path().join(name),
            receipt: temp.path().join(receipt),
            sha256: digest_file(&temp.path().join(receipt)).unwrap(),
            tree: Tree::read(&temp.path().join(receipt)).unwrap(),
        };
        let load = || {
            [
                reread("headers", "headers.json"),
                reread("provider", "provider-with-link.json"),
            ]
        };
        let store = temp.path().join("store");
        let first = compose(load(), &store).unwrap();
        let repeated = compose(load(), &store).unwrap();
        assert_eq!(first.identity, identity);
        assert_eq!(first.root, repeated.root);
        assert_eq!(first.linked_bytes, 6 + 7 + 4);
        assert!(first.copied_bytes > 0);
        assert_eq!((repeated.copied_bytes, repeated.linked_bytes), (0, 0));
        assert_eq!(
            fs::metadata(source).unwrap().ino(),
            fs::metadata(first.root.join("include/av.h")).unwrap().ino()
        );
        assert_eq!(
            fs::read(first.root.join("lib/libav.so")).unwrap(),
            b"library"
        );
        let elsewhere = tempfile::tempdir().unwrap();
        let other = parts(elsewhere.path());
        let original = parts(&{
            let p = temp.path().join("second");
            fs::create_dir(&p).unwrap();
            p
        });
        assert_eq!(
            Composition::new(original).unwrap().identity,
            Composition::new(other).unwrap().identity
        );
    }
    #[test]
    fn conflicting_shared_notice_does_not_publish_a_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let inputs = [
            part(temp.path(), "a", &[("share/license", b"one")]),
            part(temp.path(), "b", &[("share/license", b"two")]),
        ];
        let store = temp.path().join("store");
        assert!(compose(inputs, &store)
            .unwrap_err()
            .to_string()
            .contains("conflicting"));
        assert!(!store.exists());
    }
    #[test]
    fn changed_input_bytes_cannot_become_a_cache_hit() {
        let temp = tempfile::tempdir().unwrap();
        let inputs = parts(temp.path());
        let path = inputs[0].root.join("include/av.h");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&path, b"forged").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(compose(inputs, &temp.path().join("store")).is_err());
    }
    #[test]
    fn different_filesystem_copies_only_the_verified_payload() {
        let temp = tempfile::tempdir().unwrap();
        let Ok(memory) = tempfile::tempdir_in("/dev/shm") else {
            eprintln!("EXDEV fixture unavailable: no writable /dev/shm");
            return;
        };
        if fs::metadata(temp.path()).unwrap().dev() == fs::metadata(memory.path()).unwrap().dev() {
            eprintln!("EXDEV fixture unavailable: temporary directories share a filesystem");
            return;
        }
        let first = compose(parts(memory.path()), &temp.path().join("store")).unwrap();
        assert_eq!(first.linked_bytes, 0);
        let provenance_bytes = fs::metadata(first.root.join(PROVENANCE)).unwrap().len();
        assert_eq!(first.copied_bytes, 6 + 7 + 4 + provenance_bytes);
        assert_eq!(
            fs::read(first.root.join("lib/libav.so.1")).unwrap(),
            b"library"
        );
        assert_ne!(
            fs::metadata(first.root.join("include/av.h")).unwrap().dev(),
            fs::metadata(memory.path().join("headers/include/av.h"))
                .unwrap()
                .dev()
        );
    }
    #[test]
    fn unexpected_generation_member_is_rejected_without_replacing_good_output() {
        let temp = tempfile::tempdir().unwrap();
        let first = compose(parts(temp.path()), &temp.path().join("store")).unwrap();
        let before = digest_file(&first.root.join("include/av.h")).unwrap();
        write(&first.root.parent().unwrap().join("unexpected"), b"extra").unwrap();
        let load = |name: &str| Part {
            root: temp.path().join(name),
            receipt: temp.path().join(format!("{name}.json")),
            sha256: digest_file(&temp.path().join(format!("{name}.json"))).unwrap(),
            tree: Tree::read(&temp.path().join(format!("{name}.json"))).unwrap(),
        };
        assert!(compose(
            [load("headers"), load("provider")],
            &temp.path().join("store")
        )
        .is_err());
        assert_eq!(
            digest_file(&first.root.join("include/av.h")).unwrap(),
            before
        );
    }
}
