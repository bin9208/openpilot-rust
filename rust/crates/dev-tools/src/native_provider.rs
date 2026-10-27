//! Export only authenticated foreign FFmpeg/raylib providers from the old release.
//! Consumers use an independently pinned small component, never the old runtime.
use crate::{
    encoder_stage,
    native_codec_inputs::{directory, hash, insert, json, prepare_tree, regular, write, Prepared},
    seal::Result,
};
use openpilot_runtime_build::{
    assets, digest_file,
    inputs::{Entry, Tree},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const INVENTORY: &str = "be0bb63d0d7f71ea38a3f17f923d2b02c0149cc541f785916aaa93f311dbc3f2";
const RAYLIB_INVENTORY: &str = "4c19e76d8ce5072e60a2a244634f44b62f254b3f02facc08a3dd39637f486cfd";
const SOURCE: &str = "a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31";
const ORIGIN: &str = "9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291";
const MANIFEST: &str = "8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee";
const PROFILE: &str = "ffmpeg611-shared";
const PROVENANCE: &str = "provenance/native-provider.json";
const MANIFEST_PATH: &str = "provenance/original-runtime-manifest.json";
const COMPONENT_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub source: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub mode: u32,
    pub link: Option<PathBuf>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub profile: String,
    pub target: String,
    pub agnos_version: String,
    pub source_commit: String,
    pub source_archive: assets::Asset,
    pub runtime_manifest_sha256: String,
    pub members: BTreeMap<PathBuf, Member>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub schema: u32,
    pub role: String,
    pub profile: String,
    pub target: String,
    pub agnos_version: String,
    pub provider_source_commit: String,
    pub tree_sha256: String,
    pub asset: assets::Asset,
}
#[derive(Debug, Serialize)]
pub struct Export {
    pub descriptor: PathBuf,
    pub descriptor_sha256: String,
    pub archive: PathBuf,
    pub component_sha256: String,
    pub component_bytes: u64,
    pub provider_source_commit: String,
    pub tree_sha256: String,
    pub origin: Prepared,
    pub published_release: bool,
    pub provider_executed: bool,
}
#[derive(Debug, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub prepared: Prepared,
    pub provider_source_commit: String,
    pub offline_seed_bytes: u64,
    pub origin_archive_downloaded: bool,
}
fn origin_url() -> String {
    format!("https://github.com/bin9208/openpilot-rust/releases/download/runtime-{SOURCE}/openpilot-rust-{SOURCE}-aarch64.tar.gz")
}
fn component_names(profile: &str) -> Result<(&'static str, &'static str)> {
    match profile {
        PROFILE => Ok(("ffmpeg611", "ffmpeg611-shared.tar.gz")),
        "raylib600-shared" => Ok(("raylib600", "raylib600-shared.tar.gz")),
        _ => Err("unknown fixed foreign provider profile".into()),
    }
}
fn component_url(profile: &str, sha: &str) -> Result<String> {
    let (namespace, filename) = component_names(profile)?;
    Ok(format!("https://github.com/bin9208/openpilot-rust/releases/download/native-provider-{namespace}-{sha}/{filename}"))
}
fn safe(path: &Path) -> Result<()> {
    encoder_stage::safe(path.to_str().ok_or("non-UTF8 provider path")?)
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn mode(value: u32) -> u32 {
    if value & 0o111 != 0 {
        0o555
    } else {
        0o444
    }
}
impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("native provider plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }
    fn validate(&self) -> Result<()> {
        let (member_count, inventory) = match self.profile.as_str() {
            PROFILE => (36, INVENTORY),
            "raylib600-shared" => (38, RAYLIB_INVENTORY),
            _ => {
                return Err(
                    "foreign provider plan differs from the fixed original release inventory"
                        .into(),
                )
            }
        };
        if self.schema != 1
            || self.target != "aarch64-unknown-linux-gnu"
            || self.agnos_version != "19.8-carrot-bt1"
            || self.source_commit != SOURCE
            || self.source_archive.url != origin_url()
            || self.source_archive.sha256 != ORIGIN
            || self.source_archive.bytes != 1155969168
            || self.runtime_manifest_sha256 != MANIFEST
            || self.members.len() != member_count
            || hash(&serde_json::to_vec(&self.members)?) != inventory
        {
            return Err(
                "foreign provider plan differs from the fixed original release inventory".into(),
            );
        }
        for (path, m) in &self.members {
            safe(path)?;
            safe(&m.source)?;
            if let Some(link) = &m.link {
                safe(link)?;
                let target = path
                    .parent()
                    .ok_or("missing provider alias parent")?
                    .join(link);
                let file = self
                    .members
                    .get(&target)
                    .ok_or("provider alias has no pinned target")?;
                if file.link.is_some() || file.sha256 != m.sha256 || m.bytes != 0 || m.mode != 0 {
                    return Err("provider alias does not name its original pinned file".into());
                }
            } else if m.bytes > COMPONENT_LIMIT || !matches!(m.mode, 0o444 | 0o555) {
                return Err("invalid provider file mode/size".into());
            }
        }
        Ok(())
    }
}
fn expected(plan: &Plan) -> Result<Tree> {
    let mut entries = BTreeMap::new();
    for (path, m) in &plan.members {
        let entry = if let Some(target) = &m.link {
            Entry::Symlink {
                target: target.clone(),
            }
        } else {
            Entry::File {
                sha256: m.sha256.clone(),
                bytes: m.bytes,
                mode: m.mode,
            }
        };
        insert(&mut entries, path.clone(), entry)?;
    }
    let provenance = json(&("openpilot-foreign-provider-v1", plan))?;
    insert(
        &mut entries,
        PathBuf::from(PROVENANCE),
        Entry::File {
            sha256: hash(&provenance),
            bytes: provenance.len() as u64,
            mode: 0o444,
        },
    )?;
    Ok(Tree { schema: 1, entries })
}
fn manifest(plan: &Plan, bytes: &[u8]) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if value["source_commit"] != plan.source_commit || value["agnos_version"] != plan.agnos_version
    {
        return Err("original provider manifest identity differs".into());
    }
    let mut files = BTreeMap::new();
    for entry in value["files"]
        .as_array()
        .ok_or("original provider manifest has no files")?
    {
        let path = entry["path"]
            .as_str()
            .ok_or("invalid original provider manifest path")?;
        let sha = entry["sha256"]
            .as_str()
            .ok_or("invalid original provider manifest hash")?;
        if files.insert(path, sha).is_some() {
            return Err("duplicate original provider manifest path".into());
        }
    }
    for (path, m) in &plan.members {
        if path != Path::new(MANIFEST_PATH)
            && files
                .get(m.source.to_str().ok_or("non-UTF8 source member")?)
                .copied()
                != Some(m.sha256.as_str())
        {
            return Err(
                "selected foreign provider is absent from the pinned original manifest".into(),
            );
        }
    }
    Ok(())
}

/// No archive member is ever executed. Only the fixed table is written; aliases
/// are created after all regular members, and always target a pinned local file.
fn unpack(plan: &Plan, archive: &Path, root: &Path, component: bool) -> Result<()> {
    let tree = expected(plan)?;
    let selected: BTreeMap<_, _> = plan
        .members
        .iter()
        .map(|(p, m)| (m.source.clone(), p))
        .collect();
    let decoder = flate2::read::MultiGzDecoder::new(regular(archive)?);
    let bound = if component {
        128 * 1024 * 1024
    } else {
        8 * 1024 * 1024 * 1024
    };
    let mut tar = tar::Archive::new(decoder.take(bound + 1));
    let mut seen = BTreeSet::new();
    let mut written = BTreeSet::new();
    let mut total = 0u64;
    for item in tar.entries()?.raw(true) {
        let mut item = item?;
        let raw = item.path()?.into_owned();
        let path = if item.header().entry_type().is_dir() {
            PathBuf::from(
                raw.to_str()
                    .ok_or("non-UTF8 provider directory")?
                    .trim_end_matches('/'),
            )
        } else {
            raw
        };
        safe(&path)?;
        if !seen.insert(path.clone()) || seen.len() > 100_000 {
            return Err("duplicate or excessive provider archive members".into());
        }
        total = total
            .checked_add(item.size())
            .ok_or("provider archive size overflow")?;
        if total > bound {
            return Err("provider archive exceeds expansion bound".into());
        }
        let destination = if component {
            if !tree.entries.contains_key(&path) {
                return Err("component contains an unpinned or project-owned member".into());
            }
            path.clone()
        } else if let Some(path) = selected.get(&path) {
            path.to_path_buf()
        } else {
            std::io::copy(&mut item, &mut std::io::sink())?;
            continue;
        };
        let expected = tree
            .entries
            .get(&destination)
            .ok_or("unknown provider destination")?;
        let kind = item.header().entry_type();
        if item.header().mode()? & 0o6000 != 0 {
            return Err("set-id provider member".into());
        }
        match expected {
            Entry::Directory if component && kind.is_dir() && item.size() == 0 => (),
            Entry::Symlink { target } if kind.is_symlink() && item.size() == 0 => {
                if item.link_name()?.as_deref() != Some(target.as_path()) {
                    return Err("provider alias target differs from the fixed release".into());
                }
                written.insert(destination);
            }
            Entry::File {
                sha256,
                bytes,
                mode: expected_mode,
            } if kind.is_file() && item.size() == *bytes => {
                if (component && item.header().mode()? & 0o7777 != *expected_mode)
                    || (!component && mode(item.header().mode()?) != *expected_mode)
                {
                    return Err("provider file permissions differ".into());
                }
                let target = root.join(&destination);
                fs::create_dir_all(target.parent().ok_or("missing provider parent")?)?;
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                    .open(&target)?;
                let mut digest = Sha256::new();
                let mut count = 0;
                let mut buffer = [0; 65536];
                let mut metadata = Vec::new();
                loop {
                    let n = item.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    if count > *bytes {
                        return Err("provider member exceeds its fixed size".into());
                    }
                    file.write_all(&buffer[..n])?;
                    digest.update(&buffer[..n]);
                    if destination == Path::new(MANIFEST_PATH) {
                        if count > 4 * 1024 * 1024 {
                            return Err("provider manifest exceeds 4 MiB".into());
                        }
                        metadata.extend_from_slice(&buffer[..n]);
                    }
                }
                if count != *bytes || format!("{:x}", digest.finalize()) != *sha256 {
                    return Err("provider member differs from original pinned bytes".into());
                }
                if destination == Path::new(MANIFEST_PATH) {
                    manifest(plan, &metadata)?;
                }
                file.set_permissions(fs::Permissions::from_mode(*expected_mode))?;
                file.sync_all()?;
                written.insert(destination);
            }
            _ => return Err("provider member kind/size differs from pinned table".into()),
        }
    }
    let mut decoder = tar.into_inner();
    let mut tail = [0; 10240];
    let mut padding = 0;
    loop {
        let n = decoder.read(&mut tail)?;
        if n == 0 {
            break;
        }
        padding += n;
        if padding > 10240 || tail[..n].iter().any(|b| *b != 0) {
            return Err("unexpected provider archive tail".into());
        }
    }
    if decoder.limit() == 0 {
        return Err("provider archive exhausted expansion bound".into());
    }
    if !component {
        fs::create_dir_all(root.join("provenance"))?;
        write(
            &root.join(PROVENANCE),
            &json(&("openpilot-foreign-provider-v1", plan))?,
        )?;
        written.insert(PROVENANCE.into());
    }
    for (path, entry) in &tree.entries {
        if !matches!(entry, Entry::Directory) && !written.contains(path) {
            return Err("required provider member missing from archive".into());
        }
        if let Entry::Symlink { target } = entry {
            let path = root.join(path);
            fs::create_dir_all(path.parent().ok_or("missing alias parent")?)?;
            symlink(target, path)?;
        }
    }
    tree.verify(root)
}
fn origin(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Prepared> {
    let identity = hash(&json(&("openpilot-foreign-provider-v1", plan))?);
    let assets = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([("original-provider".into(), plan.source_archive.clone())]),
    };
    prepare_tree(
        identity,
        store,
        || {
            assets::prepare(cache, &assets, download)?
                .remove("original-provider")
                .ok_or_else(|| "missing original provider archive".into())
        },
        |archive, destination| {
            if let Some(root) = destination {
                unpack(plan, archive, root, false)?;
            }
            expected(plan)
        },
    )
}
fn compress(root: &Path, tree: &Tree, path: &Path) -> Result<()> {
    let gzip = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(File::create(path)?, flate2::Compression::new(6));
    let mut tar = tar::Builder::new(gzip);
    for (path, entry) in &tree.entries {
        let mut header = tar::Header::new_ustar();
        header.set_path(path)?;
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        match entry {
            Entry::Directory => {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append(&header, std::io::empty())?;
            }
            Entry::Symlink { target } => {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                header.set_link_name(target)?;
                header.set_cksum();
                tar.append(&header, std::io::empty())?;
            }
            Entry::File { bytes, mode, .. } => {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_size(*bytes);
                header.set_mode(*mode);
                header.set_cksum();
                tar.append(&header, regular(&root.join(path))?)?;
            }
        }
    }
    let raw = tar.into_inner()?.finish()?;
    raw.set_permissions(fs::Permissions::from_mode(0o444))?;
    raw.sync_all()?;
    Ok(())
}
fn room(path: &Path, growth: u64) -> Result<()> {
    let space = rustix::fs::statvfs(path)?;
    if space
        .f_bavail
        .checked_mul(space.f_frsize)
        .ok_or("provider space overflow")?
        < growth
    {
        return Err(
            "insufficient capacity for this provider operation; existing inputs retained".into(),
        );
    }
    Ok(())
}
pub fn export(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    output: &Path,
    download: &impl assets::Download,
) -> Result<Export> {
    plan.validate()?;
    export_inner(plan, cache, store, output, download)
}
fn export_inner(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    output: &Path,
    download: &impl assets::Download,
) -> Result<Export> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    match fs::symlink_metadata(output) {
        Ok(_) => return Err("provider export output already exists".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let origin = origin(plan, cache, store, download)?;
    if parent.starts_with(&origin.root) {
        return Err("provider export must be outside its immutable input prefix".into());
    }
    let tree = expected(plan)?;
    let payload = tree
        .entries
        .values()
        .map(|e| match e {
            Entry::File { bytes, .. } => *bytes,
            _ => 0,
        })
        .sum::<u64>();
    room(&parent, payload + payload / 1000 + 1024 * 1024)?;
    let temporary = tempfile::Builder::new()
        .prefix(".provider-export-")
        .tempdir_in(&parent)?;
    let (_, filename) = component_names(&plan.profile)?;
    let archive = temporary.path().join(filename);
    compress(&origin.root, &tree, &archive)?;
    tree.verify(&origin.root)?;
    let sha = digest_file(&archive)?;
    let bytes = regular(&archive)?.metadata()?.len();
    if bytes > COMPONENT_LIMIT {
        return Err("provider component exceeds 64 MiB".into());
    }
    let descriptor = Descriptor {
        schema: 1,
        role: "openpilot-foreign-provider".into(),
        profile: plan.profile.clone(),
        target: plan.target.clone(),
        agnos_version: plan.agnos_version.clone(),
        provider_source_commit: plan.source_commit.clone(),
        tree_sha256: hash(&json(&tree)?),
        asset: assets::Asset {
            url: component_url(&plan.profile, &sha)?,
            sha256: sha.clone(),
            bytes,
        },
    };
    let descriptor_bytes = json(&descriptor)?;
    write(&temporary.path().join("provider.json"), &descriptor_bytes)?;
    File::open(temporary.path())?.sync_all()?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        temporary.path(),
        rustix::fs::CWD,
        output,
        rustix::fs::RenameFlags::NOREPLACE,
    )?;
    File::open(parent)?.sync_all()?;
    Ok(Export {
        descriptor: output.join("provider.json"),
        descriptor_sha256: hash(&descriptor_bytes),
        archive: output.join(filename),
        component_sha256: sha,
        component_bytes: bytes,
        provider_source_commit: plan.source_commit.clone(),
        tree_sha256: descriptor.tree_sha256,
        origin,
        published_release: false,
        provider_executed: false,
    })
}
fn descriptor(plan: &Plan, path: &Path, pin: &str) -> Result<Descriptor> {
    let file = regular(path)?;
    if file.metadata()?.len() > 1024 * 1024 || !hex(pin) {
        return Err("provider descriptor differs from the independent expected SHA".into());
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if hash(&bytes) != pin {
        return Err("provider descriptor differs from the independent expected SHA".into());
    }
    let value: Descriptor = serde_json::from_slice(&bytes)?;
    if value.schema != 1
        || value.role != "openpilot-foreign-provider"
        || value.profile != plan.profile
        || value.target != plan.target
        || value.agnos_version != plan.agnos_version
        || value.provider_source_commit != plan.source_commit
        || value.tree_sha256 != hash(&json(&expected(plan)?)?)
        || value.asset.bytes == 0
        || !hex(&value.asset.sha256)
        || value.asset.bytes > COMPONENT_LIMIT
        || value.asset.url != component_url(&plan.profile, &value.asset.sha256)?
    {
        return Err("provider component identity/source/tree differs from pinned profile".into());
    }
    Ok(value)
}
fn seed(cache: &Path, asset: &assets::Asset, archive: &Path) -> Result<u64> {
    if regular(archive)?.metadata()?.len() != asset.bytes || digest_file(archive)? != asset.sha256 {
        return Err("offline provider component differs from independent descriptor".into());
    }
    directory(cache)?;
    let objects = cache.canonicalize()?.join("sha256");
    directory(&objects)?;
    let object = objects.join(&asset.sha256);
    directory(&object)?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(object.join("lock"))?;
    if !lock.metadata()?.is_file() {
        return Err("provider seed lock is not a regular file".into());
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)?;
    let blob = object.join("blob");
    match fs::symlink_metadata(&blob) {
        Ok(_) => {
            if regular(&blob)?.metadata()?.len() != asset.bytes
                || digest_file(&blob)? != asset.sha256
            {
                return Err("existing provider component cache is damaged".into());
            }
            Ok(0)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            room(&object, asset.bytes)?;
            let mut temporary = tempfile::NamedTempFile::new_in(&object)?;
            let n = std::io::copy(
                &mut regular(archive)?.take(asset.bytes + 1),
                temporary.as_file_mut(),
            )?;
            if n != asset.bytes || digest_file(temporary.path())? != asset.sha256 {
                return Err("provider component changed during seed copy".into());
            }
            temporary
                .as_file()
                .set_permissions(fs::Permissions::from_mode(0o444))?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(blob)?;
            File::open(object)?.sync_all()?;
            Ok(n)
        }
        Err(e) => Err(e.into()),
    }
}
pub fn prepare(
    plan: &Plan,
    path: &Path,
    pin: &str,
    cache: &Path,
    store: &Path,
    offline: Option<&Path>,
    download: &impl assets::Download,
) -> Result<Report> {
    plan.validate()?;
    prepare_inner(plan, path, pin, cache, store, offline, download)
}
fn prepare_inner(
    plan: &Plan,
    path: &Path,
    pin: &str,
    cache: &Path,
    store: &Path,
    offline: Option<&Path>,
    download: &impl assets::Download,
) -> Result<Report> {
    let descriptor = descriptor(plan, path, pin)?;
    let offline_seed_bytes = if let Some(path) = offline {
        seed(cache, &descriptor.asset, path)?
    } else {
        0
    };
    let assets = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([(plan.profile.clone(), descriptor.asset)]),
    };
    let prepared = prepare_tree(
        hash(&json(&("openpilot-foreign-provider-component-v1", plan))?),
        store,
        || {
            assets::prepare(cache, &assets, download)?
                .remove(&plan.profile)
                .ok_or_else(|| "missing provider component".into())
        },
        |archive, destination| {
            if let Some(root) = destination {
                unpack(plan, archive, root, true)?;
            }
            expected(plan)
        },
    )?;
    if digest_file(path)? != pin {
        return Err("provider descriptor changed during preparation".into());
    }
    Ok(Report {
        prepared,
        provider_source_commit: plan.source_commit.clone(),
        offline_seed_bytes,
        origin_archive_downloaded: false,
    })
}

pub fn cli(args: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut mode = None;
    let mut args = args.iter();
    while let Some(key) = args.next() {
        if matches!(
            key.as_str(),
            "--export-released-provider" | "--prepare-released-provider"
        ) && mode.is_none()
        {
            mode = Some(key.as_str());
            continue;
        }
        if ![
            "--plan",
            "--cache",
            "--store",
            "--output",
            "--descriptor",
            "--descriptor-sha256",
            "--offline-archive",
        ]
        .contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing native provider argument")?,
                )
                .is_some()
        {
            return Err("mixed/duplicate/unknown native provider arguments".into());
        }
    }
    let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-ffmpeg-provider.json");
    let plan = Plan::read(
        values
            .get("--plan")
            .map_or(default.as_path(), |v| Path::new(v)),
    )?;
    let cache = Path::new(values.get("--cache").ok_or("--cache required")?);
    let store = Path::new(values.get("--store").ok_or("--store required")?);
    if mode == Some("--export-released-provider") {
        if values
            .keys()
            .any(|k| !["--plan", "--cache", "--store", "--output"].contains(k))
        {
            return Err("provider export cannot consume a component descriptor".into());
        }
        println!(
            "{}",
            serde_json::to_string(&export(
                &plan,
                cache,
                store,
                Path::new(values.get("--output").ok_or("--output required")?),
                &assets::Curl
            )?)?
        );
    } else if mode == Some("--prepare-released-provider") {
        if values.contains_key("--output") {
            return Err("provider preparation does not publish an archive".into());
        }
        println!(
            "{}",
            serde_json::to_string(&prepare(
                &plan,
                Path::new(values.get("--descriptor").ok_or("--descriptor required")?),
                values
                    .get("--descriptor-sha256")
                    .ok_or("independent --descriptor-sha256 required")?,
                cache,
                store,
                values.get("--offline-archive").map(|v| Path::new(v)),
                &assets::Curl
            )?)?
        );
    } else {
        return Err("choose provider export or preparation".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_profiles_keep_their_inventory_and_release_namespace() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut raylib = Plan::read(&root.join("native-raylib-provider.json")).unwrap();
        assert_eq!(raylib.members.len(), 38);
        assert!(raylib.members.keys().all(|p| p.starts_with("lib")
            || p.starts_with("share/licenses/raylib")
            || p == Path::new(MANIFEST_PATH)));
        let sha = "1".repeat(64);
        assert_eq!(component_url(PROFILE, &sha).unwrap(), format!("https://github.com/bin9208/openpilot-rust/releases/download/native-provider-ffmpeg611-{sha}/ffmpeg611-shared.tar.gz"));
        assert_eq!(component_url(&raylib.profile, &sha).unwrap(), format!("https://github.com/bin9208/openpilot-rust/releases/download/native-provider-raylib600-{sha}/raylib600-shared.tar.gz"));
        assert!(component_url("unknown", &sha).is_err());
        raylib.profile = PROFILE.into();
        assert!(raylib.validate().is_err());
        raylib.profile = "raylib600-shared".into();
        let first = raylib.members.values_mut().next().unwrap();
        first.source = "rust/bin/openpilot-manager".into();
        assert!(raylib.validate().is_err());
    }
    use std::cell::Cell;
    struct Download {
        bytes: Vec<u8>,
        calls: Cell<usize>,
    }
    impl assets::Download for Download {
        fn fetch(&self, _: &assets::Asset, path: &Path) -> Result<u64> {
            self.calls.set(self.calls.get() + 1);
            fs::write(path, &self.bytes)?;
            Ok(self.bytes.len() as u64)
        }
    }
    fn original() -> Plan {
        Plan::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-ffmpeg-provider.json"))
            .unwrap()
    }
    fn fixture() -> (Plan, Download) {
        let mut plan = original();
        plan.members.clear();
        let data = b"owned foreign library";
        plan.members.insert(
            "lib/libfixture.so.1".into(),
            Member {
                source: "rust/bin/lib/libfixture.so.1".into(),
                sha256: hash(data),
                bytes: data.len() as u64,
                mode: 0o444,
                link: None,
            },
        );
        plan.members.insert(
            "lib/libfixture.so".into(),
            Member {
                source: "rust/bin/lib/libfixture.so".into(),
                sha256: hash(data),
                bytes: 0,
                mode: 0,
                link: Some("libfixture.so.1".into()),
            },
        );
        let manifest=json(&serde_json::json!({"source_commit":SOURCE,"agnos_version":plan.agnos_version.as_str(),
            "files":[{"path":"rust/bin/lib/libfixture.so.1","sha256":hash(data)},{"path":"rust/bin/lib/libfixture.so","sha256":hash(data)}]})).unwrap();
        plan.runtime_manifest_sha256 = hash(&manifest);
        plan.members.insert(
            MANIFEST_PATH.into(),
            Member {
                source: "rust/runtime-manifest.json".into(),
                sha256: hash(&manifest),
                bytes: manifest.len() as u64,
                mode: 0o444,
                link: None,
            },
        );
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gzip);
        for (name, bytes) in [
            ("rust/bin/lib/libfixture.so.1", &data[..]),
            ("rust/runtime-manifest.json", manifest.as_slice()),
            (
                "rust/bin/openpilot-manager",
                &b"OLD PROJECT BINARY MUST BE EXCLUDED"[..],
            ),
        ] {
            let mut header = tar::Header::new_ustar();
            header.set_path(name).unwrap();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append(&header, bytes).unwrap();
        }
        let mut header = tar::Header::new_ustar();
        header.set_path("rust/bin/lib/libfixture.so").unwrap();
        header.set_link_name("libfixture.so.1").unwrap();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_mode(0o777);
        header.set_size(0);
        header.set_cksum();
        tar.append(&header, std::io::empty()).unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        plan.source_archive.bytes = bytes.len() as u64;
        plan.source_archive.sha256 = hash(&bytes);
        (
            plan,
            Download {
                bytes,
                calls: Cell::new(0),
            },
        )
    }
    #[test]
    fn fixed_production_inventory_cannot_add_project_code_or_relabel_the_original_source() {
        let mut plan = original();
        assert_eq!(plan.members.len(), 36);
        plan.source_commit = "0".repeat(40);
        assert!(plan.validate().is_err());
        let mut plan = original();
        plan.members.insert(
            "bin/openpilot-manager".into(),
            Member {
                source: "rust/bin/openpilot-manager".into(),
                sha256: "0".repeat(64),
                bytes: 1,
                mode: 0o555,
                link: None,
            },
        );
        assert!(plan.validate().is_err());
        let mut plan = original();
        plan.members
            .get_mut(Path::new("lib/libavcodec.so"))
            .unwrap()
            .link = Some("../../outside".into());
        assert!(plan.validate().is_err());
    }
    #[test]
    fn component_round_trip_excludes_old_project_code_and_repeats_without_origin_transfer() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = fixture();
        let exported = export_inner(
            &plan,
            &temp.path().join("origin-cache"),
            &temp.path().join("origin-store"),
            &temp.path().join("export"),
            &download,
        )
        .unwrap();
        assert!(!exported.published_release && !exported.provider_executed);
        assert!(!exported.origin.root.join("bin/openpilot-manager").exists());
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(
            regular(&exported.archive).unwrap(),
        ));
        let names = tar
            .entries()
            .unwrap()
            .map(|e| e.unwrap().path().unwrap().into_owned())
            .collect::<Vec<_>>();
        assert!(!names
            .iter()
            .any(|p| p.to_string_lossy().contains("openpilot-manager")));
        let unexpected = Download {
            bytes: vec![],
            calls: Cell::new(0),
        };
        let cache = temp.path().join("component-cache");
        let store = temp.path().join("component-store");
        let first = prepare_inner(
            &plan,
            &exported.descriptor,
            &exported.descriptor_sha256,
            &cache,
            &store,
            Some(&exported.archive),
            &unexpected,
        )
        .unwrap();
        let repeat = prepare_inner(
            &plan,
            &exported.descriptor,
            &exported.descriptor_sha256,
            &cache,
            &store,
            None,
            &unexpected,
        )
        .unwrap();
        assert_eq!(first.prepared.tree_sha256, exported.tree_sha256);
        assert_eq!(first.prepared.root, repeat.prepared.root);
        assert_eq!(first.offline_seed_bytes, exported.component_bytes);
        assert!(first.prepared.copied_bytes > 0);
        assert_eq!(
            (
                repeat.offline_seed_bytes,
                repeat.prepared.copied_bytes,
                repeat.prepared.asset.transferred_bytes,
                repeat.prepared.asset.stored_bytes
            ),
            (0, 0, 0, 0)
        );
        assert!(
            !repeat.origin_archive_downloaded
                && !repeat.prepared.compiler_executed
                && !repeat.prepared.runtime_activated
        );
        assert_eq!(download.calls.get(), 1);
        assert_eq!(unexpected.calls.get(), 0);
        assert_eq!(
            fs::read_link(repeat.prepared.root.join("lib/libfixture.so")).unwrap(),
            Path::new("libfixture.so.1")
        );
    }
    #[test]
    fn independent_descriptor_pin_rejects_mutation_and_wrong_original_source_before_cache_creation()
    {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = fixture();
        let exported = export_inner(
            &plan,
            &temp.path().join("origin-cache"),
            &temp.path().join("origin-store"),
            &temp.path().join("export"),
            &download,
        )
        .unwrap();
        let mut changed: Descriptor =
            serde_json::from_reader(regular(&exported.descriptor).unwrap()).unwrap();
        changed.provider_source_commit = "0".repeat(40);
        let path = temp.path().join("forged.json");
        let bytes = json(&changed).unwrap();
        fs::write(&path, &bytes).unwrap();
        let forged_pin = hash(&bytes);
        for pin in [&exported.descriptor_sha256, &forged_pin] {
            assert!(prepare_inner(
                &plan,
                &path,
                pin,
                &temp.path().join("forged-cache"),
                &temp.path().join("forged-store"),
                Some(&exported.archive),
                &download
            )
            .is_err());
        }
        assert!(!temp.path().join("forged-cache").exists());
        assert!(!temp.path().join("forged-store").exists());
    }
    #[test]
    fn tampered_payload_alias_and_extra_project_member_do_not_replace_verified_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = fixture();
        let exported = export_inner(
            &plan,
            &temp.path().join("origin-cache"),
            &temp.path().join("origin-store"),
            &temp.path().join("export"),
            &download,
        )
        .unwrap();
        let report = prepare_inner(
            &plan,
            &exported.descriptor,
            &exported.descriptor_sha256,
            &temp.path().join("component-cache"),
            &temp.path().join("component-store"),
            Some(&exported.archive),
            &download,
        )
        .unwrap();
        let target = report.prepared.root.join("lib/libfixture.so.1");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&target, b"owned altered library").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(prepare_inner(
            &plan,
            &exported.descriptor,
            &exported.descriptor_sha256,
            &temp.path().join("component-cache"),
            &temp.path().join("component-store"),
            None,
            &download
        )
        .is_err());
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_ustar();
        header.set_path("bin/openpilot-manager").unwrap();
        header.set_size(1);
        header.set_mode(0o555);
        header.set_cksum();
        tar.append(&header, &b"x"[..]).unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        let bad = temp.path().join("project-code.tar.gz");
        fs::write(&bad, bytes).unwrap();
        let destination = temp.path().join("reject");
        fs::create_dir(&destination).unwrap();
        assert!(unpack(&plan, &bad, &destination, true).is_err());
        assert!(!destination.join("bin/openpilot-manager").exists());
        expected(&plan)
            .unwrap()
            .verify(&exported.origin.root)
            .unwrap();
        let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_ustar();
        header.set_path("lib/libfixture.so").unwrap();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_link_name("/outside").unwrap();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        tar.append(&header, std::io::empty()).unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        let bad = temp.path().join("escaping-alias.tar.gz");
        fs::write(&bad, bytes).unwrap();
        let destination = temp.path().join("reject-link");
        fs::create_dir(&destination).unwrap();
        assert!(unpack(&plan, &bad, &destination, true).is_err());
        assert!(!destination.join("lib/libfixture.so").exists());
    }
}
