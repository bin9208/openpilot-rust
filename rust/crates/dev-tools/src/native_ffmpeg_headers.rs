//! Reconstruct the existing released FFmpeg 6.1.1 public header inventory from
//! its unchanged upstream source. Generated target headers retain their original
//! released bytes. This does not prepare, compile or attest provider libraries.
use crate::{
    encoder_stage, ffmpeg_build,
    native_codec_inputs::{hash, insert, json, prepare_tree, regular, write, Prepared},
    seal::Result,
};
use openpilot_runtime_build::{
    assets,
    inputs::{Entry, Tree},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const HEADER_INVENTORY: &str = "1b6513ff4ece0545db8776726b8b64bc2f5f00a002378ca7f7a53cf6a0f06542";
const LIBRARY_INVENTORY: &str = "270eebd548e54ad59717a0fa511a5e44bb0ec3bcd636398d868f9abcb3f96c4a";
const LIMIT: u64 = 512 * 1024 * 1024;
const LICENSES: [&str; 5] = [
    "COPYING.GPLv2",
    "COPYING.GPLv3",
    "COPYING.LGPLv2.1",
    "COPYING.LGPLv3",
    "LICENSE.md",
];

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub source_commit: String,
    pub archive_sha256: String,
    pub runtime_manifest_sha256: String,
    pub build_record_sha256: String,
    pub libraries: BTreeMap<PathBuf, String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub version: String,
    pub target: String,
    pub agnos_version: String,
    pub source: assets::Asset,
    pub released_provider: Provider,
    pub headers: BTreeMap<PathBuf, Header>,
    pub generated: BTreeMap<PathBuf, String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub prepared: Prepared,
    pub public_headers: usize,
    pub provider_libraries_prepared: bool,
    pub actual_target_execution: bool,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("released FFmpeg header plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }
    fn validate(&self) -> Result<()> {
        let p = &self.released_provider;
        if self.schema != 1
            || self.version != ffmpeg_build::VERSION
            || self.target != "aarch64-unknown-linux-gnu"
            || self.agnos_version != "19.8-carrot-bt1"
            || self.source.url != ffmpeg_build::SOURCE_URL
            || self.source.sha256 != ffmpeg_build::SOURCE_SHA256
            || self.source.bytes != 10458600
            || p.source_commit != "a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31"
            || p.archive_sha256
                != "9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291"
            || p.runtime_manifest_sha256
                != "8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee"
            || p.build_record_sha256
                != "fa7815aa65aa90a18fc55d80a36e348c3a0bbb7ad7b46d197390cd600574df28"
            || self.headers.len() != 140
            || hash(&serde_json::to_vec(&self.headers)?) != HEADER_INVENTORY
            || p.libraries.len() != 24
            || hash(&serde_json::to_vec(&p.libraries)?) != LIBRARY_INVENTORY
            || self
                .generated
                .keys()
                .map(|p| p.as_path())
                .collect::<Vec<_>>()
                != [
                    Path::new("libavutil/avconfig.h"),
                    Path::new("libavutil/ffversion.h"),
                ]
        {
            return Err(
                "released FFmpeg header/source/provider inventory differs from pinned profile"
                    .into(),
            );
        }
        for (path, pin) in &self.headers {
            encoder_stage::safe(path.to_str().ok_or("non-UTF8 FFmpeg header path")?)?;
            if pin.bytes > 1024 * 1024 {
                return Err("oversized FFmpeg header".into());
            }
        }
        for (path, body) in &self.generated {
            check_header(self, path, body.as_bytes())?;
        }
        Ok(())
    }
}

fn check_header(plan: &Plan, path: &Path, bytes: &[u8]) -> Result<()> {
    let pin = plan
        .headers
        .get(path)
        .ok_or("unknown released FFmpeg header")?;
    if bytes.len() as u64 != pin.bytes || hash(bytes) != pin.sha256 {
        return Err(format!(
            "FFmpeg header differs from the released provider: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}
fn add(
    entries: &mut BTreeMap<PathBuf, Entry>,
    root: Option<&Path>,
    path: PathBuf,
    bytes: &[u8],
) -> Result<()> {
    if let Some(root) = root {
        let target = root.join(&path);
        fs::create_dir_all(target.parent().ok_or("missing FFmpeg header parent")?)?;
        write(&target, bytes)?;
    }
    insert(
        entries,
        path,
        Entry::File {
            sha256: hash(bytes),
            bytes: bytes.len() as u64,
            mode: 0o444,
        },
    )
}

fn expected(plan: &Plan, archive: &Path, destination: Option<&Path>) -> Result<Tree> {
    if plan.source.bytes > 32 * 1024 * 1024 {
        return Err("FFmpeg header source exceeds 32 MiB".into());
    }
    let mut bytes = Vec::new();
    regular(archive)?
        .take(plan.source.bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != plan.source.bytes || hash(&bytes) != plan.source.sha256 {
        return Err("FFmpeg header parser bytes differ from authenticated source".into());
    }
    let decoder = xz2::read::XzDecoder::new_multi_decoder(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder.take(LIMIT + 64 * 1024 * 1024 + 1));
    let base = format!("ffmpeg-{}/", plan.version);
    let mut names = BTreeSet::new();
    let mut source_headers = BTreeSet::new();
    let mut licenses = BTreeSet::new();
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    for item in archive.entries()?.raw(true) {
        let mut item = item?;
        let name = item
            .path()?
            .to_str()
            .ok_or("non-UTF8 FFmpeg source name")?
            .trim_end_matches('/')
            .to_owned();
        encoder_stage::safe(&name)?;
        if !(name == base.trim_end_matches('/') || name.starts_with(&base))
            || !names.insert(name.clone())
            || names.len() > 100_000
        {
            return Err(
                "FFmpeg header source escapes prefix or has duplicate/excessive members".into(),
            );
        }
        total = total
            .checked_add(item.size())
            .ok_or("FFmpeg header source size overflow")?;
        if total > LIMIT {
            return Err("FFmpeg header source exceeds decompression bound".into());
        }
        let Some(relative) = name.strip_prefix(&base) else {
            continue;
        };
        let path = Path::new(relative);
        let header = plan.headers.contains_key(path);
        let license = LICENSES.contains(&relative);
        if !header && !license {
            std::io::copy(&mut item, &mut std::io::sink())?;
            continue;
        }
        if !item.header().entry_type().is_file()
            || item.header().mode()? & 0o6000 != 0
            || item.size() > 1024 * 1024
        {
            return Err("released FFmpeg header/notice is not a bounded regular file".into());
        }
        let size = item.size();
        let mut bytes = Vec::new();
        (&mut item).take(size + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size {
            return Err("truncated FFmpeg header/notice".into());
        }
        if header {
            check_header(plan, path, &bytes)?;
            if !plan.generated.contains_key(path) {
                source_headers.insert(path.to_owned());
                add(
                    &mut entries,
                    destination,
                    Path::new("include").join(path),
                    &bytes,
                )?;
            }
        } else {
            licenses.insert(relative.to_owned());
            add(
                &mut entries,
                destination,
                Path::new("share/licenses/ffmpeg").join(path),
                &bytes,
            )?;
        }
    }
    let mut decoder = archive.into_inner();
    let mut buffer = [0; 10240];
    let mut padding = 0;
    loop {
        let n = decoder.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        padding += n;
        if padding > 10240 || buffer[..n].iter().any(|b| *b != 0) {
            return Err("unexpected FFmpeg header source archive tail".into());
        }
    }
    if decoder.limit() == 0
        || source_headers.len() + plan.generated.len() != plan.headers.len()
        || licenses != LICENSES.map(String::from).into_iter().collect()
    {
        return Err("released FFmpeg public headers or source notices are missing".into());
    }
    for (path, body) in &plan.generated {
        check_header(plan, path, body.as_bytes())?;
        add(
            &mut entries,
            destination,
            Path::new("include").join(path),
            body.as_bytes(),
        )?;
    }
    add(
        &mut entries,
        destination,
        PathBuf::from("provenance/released-ffmpeg-headers.json"),
        &json(plan)?,
    )?;
    Ok(Tree { schema: 1, entries })
}

fn prepare_inner(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Report> {
    let identity = hash(&json(&("openpilot-released-ffmpeg-headers-v1", plan))?);
    let assets = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([("ffmpeg-6-1-1-source".into(), plan.source.clone())]),
    };
    let prepared = prepare_tree(
        identity,
        store,
        || {
            assets::prepare(cache, &assets, download)?
                .remove("ffmpeg-6-1-1-source")
                .ok_or_else(|| "missing FFmpeg header source".into())
        },
        |archive, destination| expected(plan, archive, destination),
    )?;
    Ok(Report {
        prepared,
        public_headers: plan.headers.len(),
        provider_libraries_prepared: false,
        actual_target_execution: false,
    })
}
pub fn prepare(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Report> {
    plan.validate()?;
    prepare_inner(plan, cache, store, download)
}

pub fn cli(args: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut mode = false;
    let mut args = args.iter();
    while let Some(key) = args.next() {
        if key == "--prepare-released-headers" && !mode {
            mode = true;
            continue;
        }
        if !["--plan", "--cache", "--store"].contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing released header argument")?,
                )
                .is_some()
        {
            return Err("invalid, mixed or duplicate released FFmpeg header argument".into());
        }
    }
    if !mode {
        return Err("--prepare-released-headers required".into());
    }
    let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-ffmpeg-headers.json");
    let plan = Plan::read(
        values
            .get("--plan")
            .map_or(default.as_path(), |v| Path::new(v)),
    )?;
    let cache = Path::new(values.get("--cache").ok_or("--cache required")?);
    let store = Path::new(values.get("--store").ok_or("--store required")?);
    println!(
        "{}",
        serde_json::to_string(&prepare(&plan, cache, store, &assets::Curl)?)?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, os::unix::fs::PermissionsExt};
    fn original() -> Plan {
        Plan::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-ffmpeg-headers.json"))
            .unwrap()
    }
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
    fn fixture(link: bool) -> (Plan, Download) {
        let mut plan = original();
        let body = b"owned version header\n";
        plan.headers
            .retain(|path, _| plan.generated.contains_key(path));
        plan.headers.insert(
            "libavutil/version.h".into(),
            Header {
                sha256: hash(body),
                bytes: body.len() as u64,
            },
        );
        let compressed = xz2::write::XzEncoder::new(Vec::new(), 1);
        let mut tar = tar::Builder::new(compressed);
        for name in std::iter::once("libavutil/version.h").chain(LICENSES) {
            let mut header = tar::Header::new_ustar();
            header.set_path(format!("ffmpeg-6.1.1/{name}")).unwrap();
            header.set_mode(0o644);
            if link && name == "libavutil/version.h" {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("/outside").unwrap();
                header.set_size(0);
                header.set_cksum();
                tar.append(&header, &[][..]).unwrap();
            } else {
                header.set_size(body.len() as u64);
                header.set_cksum();
                tar.append(&header, &body[..]).unwrap();
            }
        }
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        plan.source.bytes = bytes.len() as u64;
        plan.source.sha256 = hash(&bytes);
        (
            plan,
            Download {
                bytes,
                calls: Cell::new(0),
            },
        )
    }
    #[test]
    fn exact_released_inventory_and_generated_configuration_are_not_replaceable() {
        let mut plan = original();
        assert_eq!(plan.headers.len(), 140);
        assert_eq!(plan.released_provider.libraries.len(), 24);
        plan.generated
            .get_mut(Path::new("libavutil/avconfig.h"))
            .unwrap()
            .push(' ');
        assert!(plan.validate().is_err());
        let mut plan = original();
        plan.headers.remove(Path::new("libavcodec/defs.h"));
        assert!(plan.validate().is_err());
        let mut plan = original();
        plan.released_provider
            .libraries
            .insert("lib/libavutil.so.58".into(), "0".repeat(64));
        assert!(plan.validate().is_err());
        let mut plan = original();
        plan.source.sha256 = "0".repeat(64);
        assert!(plan.validate().is_err());
    }
    #[test]
    fn authenticated_source_and_original_generated_bytes_prepare_repeat_without_copy_or_transfer() {
        let temp = tempfile::tempdir().unwrap();
        let (plan, download) = fixture(false);
        let cache = temp.path().join("assets");
        let store = temp.path().join("prefixes");
        let first = prepare_inner(&plan, &cache, &store, &download).unwrap();
        let repeated = prepare_inner(&plan, &cache, &store, &download).unwrap();
        assert_eq!(first.prepared.tree_sha256, repeated.prepared.tree_sha256);
        assert_eq!(first.prepared.root, repeated.prepared.root);
        assert!(first.prepared.copied_bytes > 0);
        assert_eq!(
            (
                repeated.prepared.copied_bytes,
                repeated.prepared.asset.transferred_bytes,
                repeated.prepared.asset.stored_bytes
            ),
            (0, 0, 0)
        );
        assert_eq!(download.calls.get(), 1);
        assert!(
            !first.provider_libraries_prepared
                && !first.prepared.compiler_executed
                && !first.actual_target_execution
        );
        assert_eq!(
            fs::read(first.prepared.root.join("include/libavutil/avconfig.h")).unwrap(),
            plan.generated[Path::new("libavutil/avconfig.h")].as_bytes()
        );
        assert!(first
            .prepared
            .root
            .join("share/licenses/ffmpeg/COPYING.GPLv3")
            .is_file());
        let target = first.prepared.root.join("include/libavutil/version.h");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        let size = fs::metadata(&target).unwrap().len();
        fs::write(&target, vec![b'x'; size as usize]).unwrap();
        fs::set_permissions(target, fs::Permissions::from_mode(0o444)).unwrap();
        assert!(prepare_inner(&plan, &cache, &store, &download).is_err());
        assert_eq!(download.calls.get(), 1);
    }
    #[test]
    fn missing_wrong_or_linked_source_headers_cannot_publish_over_the_previous_prefix() {
        let temp = tempfile::tempdir().unwrap();
        let (mut plan, download) = fixture(false);
        let cache = temp.path().join("assets");
        let store = temp.path().join("prefixes");
        let first = prepare_inner(&plan, &cache, &store, &download).unwrap();
        plan.headers.insert(
            "libavcodec/absent.h".into(),
            Header {
                sha256: hash(b"absent"),
                bytes: 6,
            },
        );
        assert!(prepare_inner(&plan, &cache, &store, &download).is_err());
        plan.headers.remove(Path::new("libavcodec/absent.h"));
        plan.headers
            .get_mut(Path::new("libavutil/version.h"))
            .unwrap()
            .sha256 = "0".repeat(64);
        assert!(prepare_inner(&plan, &cache, &store, &download).is_err());
        let (linked, bytes) = fixture(true);
        assert!(prepare_inner(&linked, &cache, &store, &bytes).is_err());
        Tree::read(&first.prepared.tree_receipt)
            .unwrap()
            .verify(&first.prepared.root)
            .unwrap();
        assert!(!temp.path().join("outside").exists());
    }
}
