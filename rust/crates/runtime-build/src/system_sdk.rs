//! A read-only logical system namespace, separate from unpacked input Trees.
//! Absolute/directory links are resolved inside the supplied image, never on the
//! host by canonicalizing an image symlink. Runtime builds verify the live `/`.
use crate::{regular_file, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

pub const KIND: &str = "agnos-system-sdk";
pub const PLAN_PATH: &str = "rust/native-system-sdk.json";
const MAX_ENTRIES: usize = 100_000;
const MAX_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Full generations require one SDK identity in every compiler/linker group.
/// This metadata-only check also works after build-input caches are removed.
pub(crate) fn generation_receipts(
    plan: &crate::prepare::Plan,
) -> Result<BTreeMap<PathBuf, String>> {
    if plan.groups.is_empty() {
        return Err("native generation has no SDK-bound build groups".into());
    }
    let mut receipts = BTreeMap::new();
    let mut identity = None;
    for (group, recipe) in &plan.groups {
        let receipt = recipe
            .input_trees
            .iter()
            .find(|(root, _)| root.as_os_str() == std::ffi::OsStr::new("/"))
            .map(|(_, receipt)| receipt)
            .ok_or_else(|| {
                format!("native generation group {group} is missing the live / system SDK input")
            })?;
        let digest = recipe
            .inputs
            .get(receipt)
            .filter(|digest| hex(digest))
            .ok_or("native generation system SDK receipt is not SHA-pinned")?;
        if !receipt.is_absolute()
            || receipt
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
        {
            return Err(
                "native generation system SDK receipt requires an absolute contained path".into(),
            );
        }
        if identity.is_some_and(|previous| previous != digest) {
            return Err("native generation groups must bind the same system SDK identity".into());
        }
        identity = Some(digest);
        receipts.insert(receipt.clone(), digest.clone());
    }
    Ok(receipts)
}

/// Check the typed receipt against the retained source's complete SDK profile.
/// The actual driver and sealing Pins still verify that snapshot against `/`;
/// accepting the profile here alone does not certify the image or live system.
pub(crate) fn generation_profiles(source: &Path, plan: &crate::prepare::Plan) -> Result<()> {
    let receipts = generation_receipts(plan)?;
    let expected = Plan::read(&source.join(PLAN_PATH))?;
    for (path, pinned) in receipts {
        const LIMIT: u64 = 16 * 1024 * 1024;
        let file = regular_file(&path)?;
        if file.metadata()?.len() > LIMIT {
            return Err("native generation SDK receipt exceeds 16 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT || format!("{:x}", Sha256::digest(&bytes)) != pinned {
            return Err(
                "native generation SDK receipt differs from its independent SHA pin".into(),
            );
        }
        // Hash and parse the same bytes. Ordinary Tree receipts cannot stand in
        // for the logical factory namespace with its absolute/directory links.
        let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
        snapshot.validate()?;
        if snapshot.plan != expected {
            return Err("native generation SDK profile differs from its own source profile".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: u32,
    pub agnos_version: String,
    pub factory_image_sha256: String,
    /// True: complete subtree. False: immediate files and directory namespaces.
    /// Missing roots are recorded as absences, not silently omitted.
    pub roots: BTreeMap<PathBuf, bool>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    Absent,
    Directory {
        children: Option<BTreeSet<String>>,
    },
    Symlink {
        target: PathBuf,
    },
    File {
        sha256: String,
        bytes: u64,
        mode: u32,
    },
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub kind: String,
    pub schema: u32,
    pub plan: Plan,
    pub entries: BTreeMap<PathBuf, Entry>,
}

fn normal(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.as_os_str().len() <= 4096
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn scope(path: &Path) -> bool {
    [
        "usr",
        "lib",
        "lib64",
        "bin",
        "sbin",
        "VERSION",
        "etc/alternatives",
        "etc/ld.so.cache",
        "etc/ld.so.conf",
        "etc/ld.so.conf.d",
        "etc/clang",
        "etc/gcc",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}
fn reachable(path: &Path) -> bool {
    scope(path) || path == Path::new("etc")
}

impl Plan {
    pub fn read(path: &Path) -> Result<Self> {
        let file = regular_file(path)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err("system SDK plan exceeds 1 MiB".into());
        }
        let plan: Self = serde_json::from_reader(file.take(1024 * 1024 + 1))?;
        plan.validate()?;
        Ok(plan)
    }
    fn validate(&self) -> Result<()> {
        if self.schema != 1
            || self.agnos_version != "19.8-carrot-bt1"
            || !hex(&self.factory_image_sha256)
            || self.roots.is_empty()
            || self.roots.len() > 256
            || self.roots.keys().any(|p| !normal(p) || !scope(p))
        {
            return Err("invalid or unsupported system SDK plan".into());
        }
        Ok(())
    }
}

fn normalize(path: &Path) -> Result<PathBuf> {
    let mut output = PathBuf::new();
    for part in path.components() {
        match part {
            Component::RootDir => output.clear(),
            Component::Normal(name) => output.push(name),
            Component::CurDir => (),
            Component::ParentDir if output.pop() => (),
            _ => return Err("system SDK link escapes its logical image root".into()),
        }
    }
    if !normal(&output) || !reachable(&output) {
        return Err("system SDK path is outside compiler namespaces".into());
    }
    Ok(output)
}

struct Capture<'a> {
    root: &'a Path,
    entries: BTreeMap<PathBuf, Entry>,
    expanded: BTreeMap<PathBuf, bool>,
    bytes: u64,
}
impl Capture<'_> {
    fn insert(&mut self, path: PathBuf, value: Entry) -> Result<()> {
        match (self.entries.get(&path), &value) {
            (Some(Entry::Directory { children: Some(_) }), Entry::Directory { children: None }) => {
                return Ok(())
            }
            (Some(Entry::Directory { children: None }), Entry::Directory { .. }) => (),
            (Some(old), new) if old != new => {
                return Err("system SDK changed while being captured".into())
            }
            _ => (),
        }
        self.entries.insert(path, value);
        if self.entries.len() > MAX_ENTRIES {
            return Err("system SDK exceeds 100000 namespace entries".into());
        }
        Ok(())
    }

    /// Walk every component without letting the host follow an absolute image
    /// link. The returned path contains real directory parents in this image.
    fn resolve(&mut self, path: &Path) -> Result<(PathBuf, bool)> {
        let mut pending: VecDeque<_> = path
            .components()
            .map(|c| c.as_os_str().to_owned())
            .collect();
        let mut resolved = PathBuf::new();
        let mut links = 0;
        while let Some(part) = pending.pop_front() {
            resolved.push(part);
            if !normal(&resolved) || !reachable(&resolved) {
                return Err("unsafe system SDK path".into());
            }
            let physical = self.root.join(&resolved);
            let metadata = match fs::symlink_metadata(&physical) {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    self.insert(resolved.clone(), Entry::Absent)?;
                    return Ok((resolved, false));
                }
                Err(e) => return Err(e.into()),
            };
            if metadata.is_symlink() {
                let target = fs::read_link(&physical)?;
                if target.as_os_str().is_empty() || target.as_os_str().len() > 4096 {
                    return Err("invalid system SDK link target".into());
                }
                let next = normalize(&resolved.parent().unwrap_or(Path::new("")).join(&target))
                    .map_err(|error| {
                        format!(
                            "system SDK link {} -> {}: {error}",
                            resolved.display(),
                            target.display()
                        )
                    })?;
                self.insert(resolved, Entry::Symlink { target })?;
                links += 1;
                if links > 40 {
                    return Err(format!(
                        "system SDK link cycle or excessive indirection at {}",
                        physical.display()
                    )
                    .into());
                }
                let mut next: VecDeque<_> = next
                    .components()
                    .map(|c| c.as_os_str().to_owned())
                    .collect();
                next.extend(pending);
                pending = next;
                resolved = PathBuf::new();
            } else if metadata.is_dir() {
                self.insert(resolved.clone(), Entry::Directory { children: None })?;
            } else if metadata.is_file() {
                if !pending.is_empty() {
                    return Err("system SDK traverses a non-directory".into());
                }
                if !self.entries.contains_key(&resolved) {
                    let mode = metadata.permissions().mode() & 0o7777;
                    self.bytes = self
                        .bytes
                        .checked_add(metadata.len())
                        .ok_or("system SDK payload size overflow")?;
                    if mode & 0o6000 != 0 || self.bytes > MAX_BYTES {
                        return Err(format!(
                            "unsupported set-id or oversized system SDK file: {}",
                            resolved.display()
                        )
                        .into());
                    }
                    let mut file = regular_file(&physical)?;
                    let mut digest = Sha256::new();
                    let mut buffer = [0; 65536];
                    let mut bytes = 0;
                    loop {
                        let n = file.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        bytes += n as u64;
                        if bytes > metadata.len() {
                            return Err("system SDK file grew during capture".into());
                        }
                        digest.update(&buffer[..n]);
                    }
                    if bytes != metadata.len()
                        || file.metadata()?.permissions().mode() & 0o7777 != mode
                    {
                        return Err("system SDK file changed during capture".into());
                    }
                    self.insert(
                        resolved.clone(),
                        Entry::File {
                            sha256: format!("{:x}", digest.finalize()),
                            bytes,
                            mode,
                        },
                    )?;
                }
            } else {
                return Err(format!(
                    "unsupported special system SDK file: {}",
                    resolved.display()
                )
                .into());
            }
        }
        Ok((resolved, true))
    }

    fn walk(&mut self, requested: PathBuf, recursive: bool) -> Result<()> {
        let (path, exists) = self.resolve(&requested)?;
        if !exists || !matches!(self.entries.get(&path), Some(Entry::Directory { .. })) {
            return Ok(());
        }
        if !scope(&path) {
            return Err("cannot expand a system SDK namespace ancestor".into());
        }
        if self
            .expanded
            .get(&path)
            .is_some_and(|deep| *deep || !recursive)
        {
            return Ok(());
        }
        let mut names = BTreeSet::new();
        for item in fs::read_dir(self.root.join(&path))? {
            let name = item?
                .file_name()
                .into_string()
                .map_err(|_| "system SDK names require UTF-8")?;
            if !normal(Path::new(&name)) || Path::new(&name).components().count() != 1 {
                return Err("invalid system SDK child name".into());
            }
            names.insert(name);
        }
        self.insert(
            path.clone(),
            Entry::Directory {
                children: Some(names.clone()),
            },
        )?;
        self.expanded.insert(path.clone(), recursive);
        for name in names {
            let child = path.join(name);
            if recursive {
                self.walk(child, true)?;
            } else {
                self.resolve(&child)?;
            }
        }
        Ok(())
    }
}

impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate()?;
        if self.kind != KIND
            || self.schema != 1
            || self.entries.is_empty()
            || self.entries.len() > MAX_ENTRIES
            || self.entries.keys().any(|p| !normal(p) || !reachable(p))
        {
            return Err("invalid system SDK namespace receipt".into());
        }
        Ok(())
    }
    /// Caller must independently authenticate the factory image and expected
    /// receipt hash. Capturing a live directory does not certify its origin.
    pub fn capture(root: &Path, plan: &Plan) -> Result<Self> {
        plan.validate()?;
        if !fs::symlink_metadata(root)?.is_dir() {
            return Err("system SDK image root must be a real directory".into());
        }
        let root = root.canonicalize()?;
        let mut version = String::new();
        regular_file(&root.join("VERSION"))?
            .take(129)
            .read_to_string(&mut version)?;
        if version.trim() != plan.agnos_version {
            return Err("system SDK AGNOS version differs from its plan".into());
        }
        let mut capture = Capture {
            root: &root,
            entries: BTreeMap::new(),
            expanded: BTreeMap::new(),
            bytes: 0,
        };
        capture.walk(PathBuf::from("VERSION"), false)?;
        for (path, recursive) in &plan.roots {
            capture.walk(path.clone(), *recursive)?;
        }
        Ok(Self {
            kind: KIND.into(),
            schema: 1,
            plan: plan.clone(),
            entries: capture.entries,
        })
    }
    pub fn verify(&self, root: &Path) -> Result<()> {
        self.validate()?;
        let actual = Self::capture(root, &self.plan)?;
        if *self != actual {
            let path = self
                .entries
                .keys()
                .chain(actual.entries.keys())
                .find(|path| self.entries.get(*path) != actual.entries.get(*path))
                .ok_or("system SDK namespace receipt is inconsistent")?;
            return Err(format!(
                "system SDK differs from its independently pinned namespace receipt at {}",
                path.display()
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod generation_input_tests {
    use super::*;
    use crate::{catalog::Catalog, prepare, Recipe};

    fn full_plan(receipt: &Path, sha256: &str) -> prepare::Plan {
        let catalog: Catalog =
            serde_json::from_str(include_str!("../../../native-runtime-catalog.json")).unwrap();
        let plan = prepare::Plan {
            schema: 1,
            groups: catalog
                .groups
                .iter()
                .map(|(name, group)| {
                    (
                        name.clone(),
                        Recipe {
                            schema: 1,
                            target: catalog.target.clone(),
                            packages: group.packages.clone(),
                            binaries: group.binaries.clone(),
                            features: group.features.clone(),
                            inputs: BTreeMap::from([(receipt.to_owned(), sha256.into())]),
                            input_trees: BTreeMap::from([(PathBuf::from("/"), receipt.to_owned())]),
                            environment: BTreeMap::from([(
                                "OPENPILOT_NATIVE_INCREMENTAL".into(),
                                "1".into(),
                            )]),
                            expected_growth_bytes: 0,
                        },
                    )
                })
                .collect(),
            expected_source_growth_bytes: 0,
            remaining_object_growth_bytes: 0,
            runtime_inputs: prepare::RuntimeInputs::default(),
        };
        catalog.verify_plan(&plan).unwrap();
        plan
    }

    // Owned data tests profile binding, not a genuine factory SDK or live `/`.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Snapshot, prepare::Plan) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let image = temp.path().join("image");
        fs::create_dir_all(source.join("rust")).unwrap();
        fs::create_dir_all(image.join("usr/include")).unwrap();
        fs::create_dir_all(image.join("usr/lib/aarch64-linux-gnu")).unwrap();
        fs::write(image.join("VERSION"), b"19.8-carrot-bt1\n").unwrap();
        fs::write(
            image.join("usr/include/native-generation-fixture.h"),
            b"fixture header",
        )
        .unwrap();
        let profile = Plan {
            schema: 1,
            agnos_version: "19.8-carrot-bt1".into(),
            factory_image_sha256: "1".repeat(64),
            roots: BTreeMap::from([
                ("usr/include".into(), true),
                ("usr/lib/aarch64-linux-gnu".into(), true),
            ]),
        };
        fs::write(
            source.join(PLAN_PATH),
            serde_json::to_vec_pretty(&profile).unwrap(),
        )
        .unwrap();
        let snapshot = Snapshot::capture(&image, &profile).unwrap();
        let receipt = temp.path().join("sdk.json");
        fs::write(&receipt, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        let plan = full_plan(&receipt, &crate::digest_file(&receipt).unwrap());
        (temp, source, receipt, snapshot, plan)
    }

    fn repin(plan: &mut prepare::Plan, receipt: &Path) {
        let sha = crate::digest_file(receipt).unwrap();
        for recipe in plan.groups.values_mut() {
            recipe.inputs.insert(receipt.to_owned(), sha.clone());
        }
    }

    #[test]
    fn full_roster_requires_the_live_sdk_in_every_group() {
        let (_temp, _source, receipt, _snapshot, plan) = fixture();
        assert_eq!(plan.groups.len(), 13);
        assert_eq!(
            plan.groups
                .values()
                .map(|r| r.binaries.len())
                .sum::<usize>(),
            99
        );
        assert_eq!(generation_receipts(&plan).unwrap().len(), 1);
        for group in plan.groups.keys() {
            let mut missing = plan.clone();
            missing.groups.get_mut(group).unwrap().input_trees.clear();
            assert!(generation_receipts(&missing)
                .unwrap_err()
                .to_string()
                .contains(group));
        }
        let mut unpinned = plan.clone();
        unpinned.groups.get_mut("00").unwrap().inputs.clear();
        assert!(generation_receipts(&unpinned).is_err());
        let mut unpacked = plan.clone();
        let recipe = unpacked.groups.get_mut("00").unwrap();
        recipe.input_trees = BTreeMap::from([(PathBuf::from("/unpacked-sdk"), receipt.clone())]);
        assert!(generation_receipts(&unpacked).is_err());
        let mut mixed = plan.clone();
        mixed
            .groups
            .get_mut("01")
            .unwrap()
            .inputs
            .insert(receipt, "2".repeat(64));
        assert!(generation_receipts(&mixed)
            .unwrap_err()
            .to_string()
            .contains("same system SDK"));
    }

    #[test]
    fn typed_profile_must_match_the_retained_source_even_after_repinning() {
        let (_temp, source, receipt, snapshot, mut plan) = fixture();
        generation_profiles(&source, &plan).unwrap();
        let mut reduced: serde_json::Value = serde_json::to_value(&snapshot).unwrap();
        reduced["plan"]["roots"]
            .as_object_mut()
            .unwrap()
            .remove("usr/lib/aarch64-linux-gnu");
        fs::write(&receipt, serde_json::to_vec(&reduced).unwrap()).unwrap();
        assert!(generation_profiles(&source, &plan)
            .unwrap_err()
            .to_string()
            .contains("SHA pin"));
        repin(&mut plan, &receipt);
        assert!(generation_profiles(&source, &plan)
            .unwrap_err()
            .to_string()
            .contains("own source profile"));
        let mut wrong_image: serde_json::Value = serde_json::to_value(&snapshot).unwrap();
        wrong_image["plan"]["factory_image_sha256"] = "2".repeat(64).into();
        fs::write(&receipt, serde_json::to_vec(&wrong_image).unwrap()).unwrap();
        repin(&mut plan, &receipt);
        assert!(generation_profiles(&source, &plan)
            .unwrap_err()
            .to_string()
            .contains("own source profile"));
    }

    #[test]
    fn ordinary_tree_or_another_namespace_kind_cannot_replace_the_sdk() {
        let (_temp, source, receipt, snapshot, mut plan) = fixture();
        fs::write(&receipt, br#"{"schema":1,"entries":{}}"#).unwrap();
        repin(&mut plan, &receipt);
        assert!(generation_profiles(&source, &plan).is_err());
        let mut wrong: serde_json::Value = serde_json::to_value(&snapshot).unwrap();
        wrong["kind"] = "unrelated".into();
        fs::write(&receipt, serde_json::to_vec(&wrong).unwrap()).unwrap();
        repin(&mut plan, &receipt);
        assert!(generation_profiles(&source, &plan).is_err());
    }

    #[test]
    fn boot_metadata_validation_does_not_reopen_removed_input_caches() {
        let (_temp, source, receipt, _snapshot, plan) = fixture();
        generation_profiles(&source, &plan).unwrap();
        let expected = generation_receipts(&plan).unwrap();
        fs::remove_file(receipt).unwrap();
        assert!(generation_profiles(&source, &plan).is_err());
        assert_eq!(generation_receipts(&plan).unwrap(), expected);
    }
}
