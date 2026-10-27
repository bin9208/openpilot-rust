//! Restore the released OpenCV headers without configuring or rebuilding OpenCV.
use crate::{
    encoder_stage,
    native_codec_inputs::{hash, insert, json, prepare_tree, regular, write, Prepared},
    opencv_build,
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

const INVENTORY: &str = "2b59ab600acfff15039709ff4ca395ec02a2723c440208be7d91334465f81c63";
const ORIGINAL_MANIFEST: &str = "d34c38a3ae14a51624759d526a825e596297ff203ad3ad6861cd08a32798934d";
const LIMIT: u64 = 1024 * 1024 * 1024;
const NOTICES: [(&str, &str, u64); 2] = [
    (
        "LICENSE",
        "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
        11358,
    ),
    (
        "COPYRIGHT",
        "854538f426422b0f989a5e5b7641e5943753d10ef29ea54901dcd36039e43f58",
        790,
    ),
];
const PATCH: &[u8] = include_bytes!("../../opencv-runtime/native/opencv-defined-access.patch");

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub source: Option<PathBuf>,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub version: String,
    pub target: String,
    pub agnos_version: String,
    pub source: assets::Asset,
    pub released_provider_source: String,
    pub released_runtime_manifest_sha256: String,
    pub original_header_manifest: String,
    pub headers: BTreeMap<PathBuf, Header>,
    pub generated: BTreeMap<PathBuf, String>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    #[serde(flatten)]
    pub prepared: Prepared,
    pub public_headers: usize,
    pub source_archive_decompressed: bool,
    pub provider_libraries_prepared: bool,
    pub actual_target_execution: bool,
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("OpenCV header plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.version != "4.13.0"
            || self.target != "aarch64-unknown-linux-gnu"
            || self.agnos_version != "19.8-carrot-bt1"
            || self.source.url
                != format!(
                    "https://codeload.github.com/opencv/opencv/tar.gz/{}",
                    opencv_build::REVISION
                )
            || self.source.sha256 != opencv_build::ARCHIVE_SHA256
            || self.source.bytes != 95450266
            || self.released_provider_source != "a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31"
            || self.released_runtime_manifest_sha256
                != "8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee"
            || hash(self.original_header_manifest.as_bytes()) != ORIGINAL_MANIFEST
            || self.headers.len() != 170
            || hash(&serde_json::to_vec(&self.headers)?) != INVENTORY
            || hash(PATCH) != opencv_build::PATCH_SHA256
            || self
                .generated
                .keys()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>()
                != [
                    Path::new("opencv2/cvconfig.h"),
                    Path::new("opencv2/opencv_modules.hpp"),
                ]
        {
            return Err(
                "OpenCV headers/source/configuration differ from the original released provider"
                    .into(),
            );
        }
        let mut released = BTreeMap::new();
        for line in self.original_header_manifest.lines() {
            let (sha, path) = line
                .split_once("  ")
                .ok_or("invalid original OpenCV hash row")?;
            encoder_stage::safe(path)?;
            if released.insert(path, sha).is_some() {
                return Err("duplicate original OpenCV hash row".into());
            }
        }
        let prefix = "install/include/opencv4/";
        if released.keys().filter(|p| p.starts_with(prefix)).count() != self.headers.len() {
            return Err("original OpenCV header roster differs".into());
        }
        let mut sources = BTreeSet::new();
        for (path, header) in &self.headers {
            encoder_stage::safe(path.to_str().ok_or("non-UTF8 OpenCV header")?)?;
            if header.bytes > 1024 * 1024
                || released
                    .get(format!("{prefix}{}", path.display()).as_str())
                    .copied()
                    != Some(header.sha256.as_str())
            {
                return Err("OpenCV header differs from original released hash manifest".into());
            }
            match (&header.source, self.generated.get(path)) {
                (Some(source), None) => {
                    encoder_stage::safe(source.to_str().ok_or("non-UTF8 OpenCV source member")?)?;
                    if !sources.insert(source) {
                        return Err("duplicate OpenCV source selector".into());
                    }
                }
                (None, Some(body)) => check_header(header, body.as_bytes())?,
                _ => {
                    return Err(
                        "OpenCV header must have exactly one original or generated source".into(),
                    )
                }
            }
        }
        Ok(())
    }
}
fn check_header(pin: &Header, bytes: &[u8]) -> Result<()> {
    if bytes.len() as u64 != pin.bytes || hash(bytes) != pin.sha256 {
        return Err("OpenCV header differs from the released provider bytes".into());
    }
    Ok(())
}
fn add(tree: &mut Tree, path: PathBuf, sha256: String, bytes: u64) -> Result<()> {
    insert(
        &mut tree.entries,
        path,
        Entry::File {
            sha256,
            bytes,
            mode: 0o444,
        },
    )
}
fn metadata(plan: &Plan) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    Ok(BTreeMap::from([
        (
            PathBuf::from("provenance/released-opencv-headers.json"),
            json(plan)?,
        ),
        (
            PathBuf::from("provenance/original-opencv-native-libraries.sha256"),
            plan.original_header_manifest.as_bytes().to_vec(),
        ),
        (
            PathBuf::from("share/licenses/opencv/opencv-defined-access.patch"),
            PATCH.to_vec(),
        ),
    ]))
}
fn expected(plan: &Plan) -> Result<Tree> {
    // The plan's complete inventory is independently pinned in this source.
    // Reuse verifies this expected tree, never a locally regenerated receipt;
    // the common publisher also hashes the original raw asset before/after.
    let mut tree = Tree {
        schema: 1,
        entries: BTreeMap::new(),
    };
    for (path, pin) in &plan.headers {
        add(
            &mut tree,
            Path::new("install/include/opencv4").join(path),
            pin.sha256.clone(),
            pin.bytes,
        )?;
    }
    for (name, sha, bytes) in NOTICES {
        add(
            &mut tree,
            Path::new("share/licenses/opencv").join(name),
            sha.into(),
            bytes,
        )?;
    }
    for (path, body) in metadata(plan)? {
        add(&mut tree, path, hash(&body), body.len() as u64)?;
    }
    Ok(tree)
}
fn harden_header(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if hash(&bytes) != opencv_build::PATCHED[0].1 {
        return Err("original OpenCV SSE header hash differs".into());
    }
    let body = String::from_utf8(bytes)?;
    let body = body.replacen(
        "#include <algorithm>\n",
        "#include <algorithm>\n#include <cstring>\n",
        1,
    );
    let marker = "////////////// Lookup table access ////////////////////\n\n";
    let insertion="// Scalar table addresses may be aligned only to the source element type.\ntemplate <typename T, typename Source>\ninline T v_lut_load_unaligned(const Source* pointer)\n{\n    T value;\n    std::memcpy(&value, pointer, sizeof(value));\n    return value;\n}\n\n";
    let mut body = body.replacen(marker, &format!("{marker}{insertion}"), 1);
    for kind in ["short", "int", "int64_t"] {
        for i in 0..8 {
            body = body.replace(
                &format!("*(const {kind}*)(tab + idx[{i}])"),
                &format!("v_lut_load_unaligned<{kind}>(tab + idx[{i}])"),
            );
        }
    }
    if hash(body.as_bytes()) != opencv_build::PATCHED[0].2 {
        return Err("released OpenCV SSE hardening output hash differs".into());
    }
    Ok(body.into_bytes())
}
fn put(root: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let target = root.join(path);
    fs::create_dir_all(target.parent().ok_or("missing OpenCV header parent")?)?;
    write(&target, bytes)
}
fn materialize(plan: &Plan, archive: &Path, root: &Path) -> Result<Tree> {
    let mut bytes = Vec::new();
    regular(archive)?
        .take(plan.source.bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != plan.source.bytes || hash(&bytes) != plan.source.sha256 {
        return Err("OpenCV header parser bytes differ from authenticated source".into());
    }
    let base = format!("opencv-{}/", opencv_build::REVISION);
    let selectors: BTreeMap<_, _> = plan
        .headers
        .iter()
        .filter_map(|(path, pin)| {
            pin.source
                .as_ref()
                .map(|source| (source.clone(), (path, pin)))
        })
        .collect();
    let decoder = flate2::read::MultiGzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder.take(LIMIT + 64 * 1024 * 1024 + 1));
    let mut names = BTreeSet::new();
    let mut selected = BTreeSet::new();
    let mut notices = BTreeSet::new();
    let mut total = 0u64;
    let mut pax = 0;
    for item in archive.entries()? {
        let mut item = item?;
        if item.header().entry_type().is_pax_global_extensions() {
            pax += 1;
            if pax > 1 || item.size() > 8192 {
                return Err("oversized/duplicate OpenCV global metadata".into());
            }
            std::io::copy(&mut item, &mut std::io::sink())?;
            continue;
        }
        let name = item
            .path()?
            .to_str()
            .ok_or("non-UTF8 OpenCV source path")?
            .trim_end_matches('/')
            .to_owned();
        encoder_stage::safe(&name)?;
        let kind = item.header().entry_type();
        if !(name == base.trim_end_matches('/') || name.starts_with(&base))
            || !names.insert(name.clone())
            || names.len() > 100_000
            || !(kind.is_file() || kind.is_dir())
        {
            return Err(
                "OpenCV source escapes its prefix or has duplicate/unsupported members".into(),
            );
        }
        total = total
            .checked_add(item.size())
            .ok_or("OpenCV header source size overflow")?;
        if total > LIMIT {
            return Err("OpenCV header source exceeds decompression bound".into());
        }
        let Some(relative) = name.strip_prefix(&base) else {
            continue;
        };
        let selector = selectors.get(Path::new(relative));
        let notice = NOTICES.iter().find(|(name, _, _)| *name == relative);
        if selector.is_none() && notice.is_none() {
            std::io::copy(&mut item, &mut std::io::sink())?;
            continue;
        }
        if !kind.is_file() || item.header().mode()? & 0o6000 != 0 || item.size() > 1024 * 1024 {
            return Err("OpenCV selected header/notice is not a bounded regular file".into());
        }
        let size = item.size();
        let mut data = Vec::new();
        (&mut item).take(size + 1).read_to_end(&mut data)?;
        if data.len() as u64 != size {
            return Err("truncated OpenCV header/notice".into());
        }
        if let Some((path, pin)) = selector {
            if relative == opencv_build::PATCHED[0].0 {
                data = harden_header(data)?;
            }
            check_header(pin, &data)?;
            put(
                root,
                &Path::new("install/include/opencv4").join(path),
                &data,
            )?;
            selected.insert(path.as_path().to_owned());
        } else if let Some((name, sha, size)) = notice {
            if data.len() as u64 != *size || hash(&data) != *sha {
                return Err("original OpenCV notice differs".into());
            }
            put(root, &Path::new("share/licenses/opencv").join(name), &data)?;
            notices.insert(*name);
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
            return Err("unexpected OpenCV source archive tail".into());
        }
    }
    if decoder.limit() == 0
        || selected.len() + plan.generated.len() != plan.headers.len()
        || notices != NOTICES.map(|(n, _, _)| n).into_iter().collect()
    {
        return Err("original OpenCV headers/notices are missing".into());
    }
    for (path, body) in &plan.generated {
        check_header(
            plan.headers
                .get(path)
                .ok_or("unknown generated OpenCV header")?,
            body.as_bytes(),
        )?;
        put(
            root,
            &Path::new("install/include/opencv4").join(path),
            body.as_bytes(),
        )?;
    }
    for (path, body) in metadata(plan)? {
        put(root, &path, &body)?;
    }
    let tree = expected(plan)?;
    tree.verify(root)?;
    Ok(tree)
}
fn prepare_inner(
    plan: &Plan,
    cache: &Path,
    store: &Path,
    download: &impl assets::Download,
) -> Result<Report> {
    let assets = assets::Plan {
        schema: 1,
        reserve_bytes: 0,
        assets: BTreeMap::from([("opencv-4-13-source".into(), plan.source.clone())]),
    };
    let prepared = prepare_tree(
        hash(&json(&("openpilot-released-opencv-headers-v1", plan))?),
        store,
        || {
            assets::prepare(cache, &assets, download)?
                .remove("opencv-4-13-source")
                .ok_or_else(|| "missing OpenCV header source".into())
        },
        |archive, destination| match destination {
            Some(root) => materialize(plan, archive, root),
            None => expected(plan),
        },
    )?;
    Ok(Report {
        source_archive_decompressed: prepared.copied_bytes > 0,
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
pub fn cli(arguments: &[String]) -> Result<()> {
    let mut values = BTreeMap::new();
    let mut mode = false;
    let mut args = arguments.iter();
    while let Some(key) = args.next() {
        if key == "--prepare-released-headers" && !mode {
            mode = true;
            continue;
        }
        if !["--plan", "--cache", "--store"].contains(&key.as_str())
            || values
                .insert(
                    key.as_str(),
                    args.next().ok_or("missing OpenCV header argument")?,
                )
                .is_some()
        {
            return Err("invalid, mixed or duplicate released OpenCV header argument".into());
        }
    }
    if !mode {
        return Err("--prepare-released-headers required".into());
    }
    let default = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-opencv-headers.json");
    let plan = Plan::read(
        values
            .get("--plan")
            .map_or(default.as_path(), |p| Path::new(p)),
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
    fn original() -> Plan {
        Plan::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native-opencv-headers.json"))
            .unwrap()
    }
    struct NoDownload;
    impl assets::Download for NoDownload {
        fn fetch(&self, _: &assets::Asset, _: &Path) -> Result<u64> {
            panic!("invalid header plan must fail before download")
        }
    }
    #[test]
    fn original_source_and_generated_configuration_are_required_before_any_cache_creation() {
        for kind in 0..6 {
            let temp = tempfile::tempdir().unwrap();
            let mut plan = original();
            match kind {
                0 => plan.source.sha256 = "0".repeat(64),
                1 => plan.source.bytes -= 1,
                2 => {
                    plan.headers
                        .get_mut(Path::new("opencv2/core.hpp"))
                        .unwrap()
                        .source = Some("modules/imgproc/include/opencv2/core.hpp".into())
                }
                3 => plan
                    .generated
                    .get_mut(Path::new("opencv2/cvconfig.h"))
                    .unwrap()
                    .push_str("#define HAVE_OPENCL\n"),
                4 => {
                    plan.original_header_manifest = plan.original_header_manifest.replace(
                        "5d04fe4002d97cb71e3dd4b5b41d4c6375759be72cdda4b44a37003a8cf6f870",
                        &"0".repeat(64),
                    )
                }
                _ => {
                    plan.headers.remove(Path::new("opencv2/opencv_modules.hpp"));
                }
            }
            let cache = temp.path().join("assets");
            let store = temp.path().join("prefixes");
            assert!(prepare(&plan, &cache, &store, &NoDownload).is_err());
            assert!(!cache.exists() && !store.exists());
        }
    }
    #[test]
    fn expected_tree_contains_all_original_headers_and_no_libraries_or_compiler() {
        let plan = original();
        assert_eq!(
            plan.headers.values().filter(|h| h.source.is_some()).count(),
            168
        );
        assert_eq!(plan.generated.len(), 2);
        let tree = expected(&plan).unwrap();
        assert_eq!(
            tree.entries
                .keys()
                .filter(|p| p.starts_with("install/include/opencv4")
                    && matches!(tree.entries[*p], Entry::File { .. }))
                .count(),
            170
        );
        assert!(!tree
            .entries
            .keys()
            .any(|p| p.starts_with("install/lib") || p.starts_with("bin")));
    }
}
