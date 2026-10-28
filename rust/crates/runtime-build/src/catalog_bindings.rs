//! Bind independently prepared compiler inputs to the source catalog. Pins are
//! supplied by trusted preparers; this module never invents an expected digest.
use crate::{catalog, digest_file, inputs, prepare, regular_file, source, system_sdk, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Rust,
    Cargo,
    SystemSdk,
    Capnp,
    Ffmpeg,
    Libyuv,
    Raylib,
    Opencv,
    Eigen,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Cargo => "cargo",
            Self::SystemSdk => "system-sdk",
            Self::Capnp => "capnp",
            Self::Ffmpeg => "ffmpeg",
            Self::Libyuv => "libyuv",
            Self::Raylib => "raylib",
            Self::Opencv => "opencv",
            Self::Eigen => "eigen",
        }
    }
    fn environment(name: &str) -> Result<Self> {
        Ok(match name {
            "OPENPILOT_CARGO_VENDOR" => Self::Cargo,
            "OPENPILOT_CAPNP" => Self::Capnp,
            "FFMPEG_DIR" => Self::Ffmpeg,
            "LIBCLANG_PATH" => Self::SystemSdk,
            "ENCODER_LIBYUV_LIB" => Self::Libyuv,
            "STARTUP_UI_RAYLIB_ROOT" => Self::Raylib,
            "OPENPILOT_WEBCAM_OPENCV_ROOT" | "OPENPILOT_OPENCV_ROOT" => Self::Opencv,
            "LOCATIOND_EIGEN_INCLUDE" | "PARAMSD_EIGEN_INCLUDE" => Self::Eigen,
            _ => return Err("unsupported source catalog compiler input".into()),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub root: PathBuf,
    pub receipt: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeInputs {
    pub schema: u32,
    pub inputs: BTreeMap<Kind, Pin>,
    pub runtime_inputs: prepare::RuntimeInputs,
}

/// Explicit operation estimates, not a fixed free-space floor or measurements.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    pub schema: u32,
    pub group_growth_bytes: BTreeMap<String, u64>,
    pub expected_source_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
    pub estimate_basis: String,
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.to_str().is_some_and(|p| {
            !p.contains(['\n', '\r', '\0'])
                && !path
                    .components()
                    .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
        })
}

fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Pin {
    fn set(&self) -> catalog::InputSet {
        catalog::InputSet {
            files: BTreeMap::from([(self.receipt.clone(), self.sha256.clone())]),
            trees: BTreeMap::from([(self.root.clone(), self.receipt.clone())]),
            environment: BTreeMap::new(),
        }
    }

    fn validate(&self, kind: Kind) -> Result<()> {
        if !absolute(&self.root)
            || !absolute(&self.receipt)
            || !hex(&self.sha256)
            || (kind == Kind::SystemSdk) != (self.root == Path::new("/"))
            || self
                .root
                .to_string_lossy()
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, ':' | ','))
        {
            return Err(
                "native input needs an absolute unambiguous root and independent receipt SHA"
                    .into(),
            );
        }
        Ok(())
    }

    /// Hash and parse the same bounded bytes. Actual complete Tree comparison
    /// occurs once per input set in Catalog::compose, then at each build.
    fn verify_receipt(&self, kind: Kind) -> Result<()> {
        self.validate(kind)?;
        if self.root.canonicalize()? != self.root || self.receipt.canonicalize()? != self.receipt {
            return Err(
                "native input paths must be canonical and cannot alias another prefix".into(),
            );
        }
        const LIMIT: u64 = 16 * 1024 * 1024;
        let file = regular_file(&self.receipt)?;
        if file.metadata()?.len() > LIMIT {
            return Err("native input receipt exceeds 16 MiB".into());
        }
        let mut bytes = Vec::new();
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT || format!("{:x}", Sha256::digest(&bytes)) != self.sha256 {
            return Err("native input receipt differs from its independent SHA pin".into());
        }
        if kind == Kind::SystemSdk {
            serde_json::from_slice::<system_sdk::Snapshot>(&bytes)?.validate()?;
        } else {
            let tree: inputs::Tree = serde_json::from_slice(&bytes)?;
            if tree.schema != 1 || tree.entries.is_empty() || tree.entries.len() > 100_000 {
                return Err("invalid native input Tree".into());
            }
            let required: &[&str] = match kind {
                Kind::Rust => &["bin/cargo", "bin/rustc"],
                Kind::Cargo => &["provenance/native-cargo.json", "vendor"],
                Kind::Capnp => &["usr/bin/capnp"],
                Kind::Ffmpeg => &[
                    "include/libavcodec/avcodec.h",
                    "provenance/native-ffmpeg-prefix.json",
                ],
                Kind::Libyuv => &["lib/libyuv.a"],
                Kind::Raylib => &[
                    "include/raylib.h",
                    "include/rlgl.h",
                    "lib/libraylib_comma.a",
                ],
                Kind::Opencv => &[
                    "native-libraries.sha256",
                    "install/include/opencv4/opencv2/cvconfig.h",
                    "install/lib/libopencv_core.so.4.13.0",
                ],
                Kind::Eigen => &["eigen3/Eigen/Dense"],
                Kind::SystemSdk => unreachable!(),
            };
            if required
                .iter()
                .any(|p| !tree.entries.contains_key(Path::new(p)))
            {
                return Err(format!("native input Tree lacks {} layout", kind.name()).into());
            }
        }
        Ok(())
    }
}

impl NativeInputs {
    /// Pure assignment: changing a provider does not pin the whole input-list
    /// JSON into every group, which would discard unrelated compiler caches.
    pub fn bindings(
        &self,
        catalog: &catalog::Catalog,
        budgets: &Budgets,
    ) -> Result<catalog::Bindings> {
        if self.schema != 1
            || budgets.schema != 1
            || budgets.group_growth_bytes.keys().ne(catalog.groups.keys())
            || budgets.group_growth_bytes.values().any(|bytes| *bytes == 0)
            || budgets.expected_source_growth_bytes == 0
            || budgets.remaining_object_growth_bytes == 0
            || budgets.estimate_basis.trim().is_empty()
            || budgets.estimate_basis.len() > 4096
        {
            return Err("native assembly requires explicit nonzero operation growth estimates and their basis".into());
        }
        let common = [Kind::Rust, Kind::Cargo, Kind::SystemSdk];
        let mut required: BTreeSet<_> = common.into_iter().collect();
        for group in catalog.groups.values() {
            for name in group.compiler_inputs.keys() {
                required.insert(Kind::environment(name)?);
            }
        }
        if self.inputs.keys().copied().collect::<BTreeSet<_>>() != required {
            return Err(
                "native input roles must exactly cover the source catalog requirements".into(),
            );
        }
        let mut sets = BTreeMap::new();
        for (kind, pin) in &self.inputs {
            pin.validate(*kind)?;
            sets.insert(kind.name().into(), pin.set());
        }
        sets.get_mut("cargo").unwrap().environment.insert(
            "OPENPILOT_CARGO_VENDOR".into(),
            self.inputs[&Kind::Cargo]
                .root
                .join("vendor")
                .display()
                .to_string(),
        );
        for (key, value) in [
            ("CC", "/usr/bin/gcc"),
            ("CXX", "/usr/bin/g++"),
            ("AR", "/usr/bin/ar"),
            ("RANLIB", "/usr/bin/ranlib"),
        ] {
            let env = &mut sets.get_mut("system-sdk").unwrap().environment;
            env.insert(key.into(), value.into());
            env.insert(format!("HOST_{key}"), value.into());
        }
        let mut groups = BTreeMap::new();
        for (name, group) in &catalog.groups {
            let mut selected = BTreeSet::new();
            let sdk = &self.inputs[&Kind::SystemSdk];
            // Environment-only sets still bind an actual stable SDK receipt.
            // Its pin is already common; no unrelated provider identity leaks.
            let mut environment = catalog::InputSet {
                files: BTreeMap::from([(sdk.receipt.clone(), sdk.sha256.clone())]),
                ..catalog::InputSet::default()
            };
            for (key, suffix) in &group.compiler_inputs {
                let role = Kind::environment(key)?;
                if !common.contains(&role) {
                    selected.insert(role.name().to_owned());
                }
                if role != Kind::Cargo {
                    let root = &self.inputs[&role].root;
                    let path = if suffix.is_empty() {
                        root.clone()
                    } else {
                        root.join(suffix)
                    };
                    environment
                        .environment
                        .insert(key.clone(), path.display().to_string());
                }
            }
            let mut libraries = Vec::new();
            for (role, suffix) in [
                (Kind::Capnp, "usr/lib/aarch64-linux-gnu"),
                (Kind::Ffmpeg, "lib"),
                (Kind::Opencv, "install/lib"),
            ] {
                if selected.contains(role.name()) {
                    libraries.push(self.inputs[&role].root.join(suffix).display().to_string());
                }
            }
            if !libraries.is_empty() {
                environment
                    .environment
                    .insert("LD_LIBRARY_PATH".into(), libraries.join(":"));
                environment.environment.insert(
                    "RUSTFLAGS".into(),
                    libraries
                        .iter()
                        .map(|p| format!("-C link-arg=-Wl,-rpath-link,{p}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            let env_name = format!("environment-{name}");
            sets.insert(env_name.clone(), environment);
            selected.insert(env_name);
            groups.insert(name.clone(), selected.into_iter().collect());
        }
        Ok(catalog::Bindings {
            schema: 1,
            input_sets: sets,
            common: common.into_iter().map(|kind| kind.name().into()).collect(),
            groups,
            group_growth_bytes: budgets.group_growth_bytes.clone(),
            expected_source_growth_bytes: budgets.expected_source_growth_bytes,
            remaining_object_growth_bytes: budgets.remaining_object_growth_bytes,
            runtime_inputs: self.runtime_inputs.clone(),
        })
    }

    pub fn check_output(&self, root: &Path, output: &Path) -> Result<()> {
        let parent = output
            .parent()
            .ok_or("native output needs a parent")?
            .canonicalize()?;
        let target = parent.join(output.file_name().ok_or("native output needs a name")?);
        let source = root.canonicalize()?;
        let runtime_roots = self
            .runtime_inputs
            .trees
            .keys()
            .map(|tree| source.join(tree).canonicalize())
            .collect::<std::io::Result<Vec<_>>>()?;
        if !absolute(&target)
            || target.starts_with(&source)
            || system_sdk::scope(target.strip_prefix("/")?)
            || self
                .inputs
                .iter()
                .any(|(kind, pin)| *kind != Kind::SystemSdk && target.starts_with(&pin.root))
            || runtime_roots.iter().any(|tree| target.starts_with(tree))
        {
            return Err(
                "native plan output must remain outside source and pinned input Trees".into(),
            );
        }
        Ok(())
    }
}

pub fn assemble(
    root: &Path,
    cargo: &Path,
    inputs_path: &Path,
    budgets_path: &Path,
    output: &Path,
) -> Result<(catalog::Bindings, prepare::Plan, catalog::Audit)> {
    let inputs_sha = digest_file(inputs_path)?;
    let budgets_sha = digest_file(budgets_path)?;
    let inputs: NativeInputs = catalog::read(inputs_path)?;
    let budgets: Budgets = catalog::read(budgets_path)?;
    inputs.check_output(root, output)?;
    // Validate roles before executing a caller-supplied metadata Cargo binary.
    let before = source::inspect(&root.canonicalize()?)?;
    let catalog_sha = digest_file(&root.join(catalog::PATH))?;
    let source_catalog: catalog::Catalog = catalog::read(&root.join(catalog::PATH))?;
    let bindings = inputs.bindings(&source_catalog, &budgets)?;
    for (kind, pin) in &inputs.inputs {
        pin.verify_receipt(*kind)?;
    }
    if cargo != inputs.inputs[&Kind::Rust].root.join("bin/cargo") || cargo.canonicalize()? != cargo
    {
        return Err("metadata Cargo must be the independently pinned native Rust input".into());
    }
    // Verify actual compiler content before executing the pinned Cargo binary.
    let plan = source_catalog.compose(&bindings, root)?;
    let (catalog, mut audit) = catalog::audit(root, cargo)?;
    catalog.verify_compiler_bindings(&plan)?;
    system_sdk::generation_profiles(root, &plan)?;
    if audit.source != before
        || audit.catalog_sha256 != catalog_sha
        || source::inspect(&root.canonicalize()?)? != audit.source
        || digest_file(&root.join(catalog::PATH))? != audit.catalog_sha256
        || digest_file(cargo)? != audit.metadata_cargo_sha256
        || digest_file(inputs_path)? != inputs_sha
        || digest_file(budgets_path)? != budgets_sha
    {
        return Err("native input assembly changed during verification".into());
    }
    // Recheck every externally supplied receipt, without recapturing the large
    // trees. Build/sealing still perform their own complete before/after checks.
    for (kind, pin) in &inputs.inputs {
        pin.verify_receipt(*kind)?;
    }
    audit.compiler_inputs_bound = Some(true);
    audit.bindings_sha256 = Some(catalog::json_sha256(&bindings)?);
    audit.plan_sha256 = Some(catalog::json_sha256(&plan)?);
    audit.native_inputs_sha256 = Some(inputs_sha);
    audit.budgets_sha256 = Some(budgets_sha);
    audit.growth_estimate_basis = Some(budgets.estimate_basis);
    Ok((bindings, plan, audit))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independently_pinned_receipts_reject_tampering_role_substitution_and_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("rust");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        for name in ["cargo", "rustc"] {
            std::fs::write(
                root.join("bin").join(name),
                "owned metadata fixture; never executed",
            )
            .unwrap();
        }
        let receipt = base.join("tree.json");
        std::fs::write(
            &receipt,
            serde_json::to_vec(&inputs::Tree::capture(&root).unwrap()).unwrap(),
        )
        .unwrap();
        let pin = Pin {
            root,
            receipt: receipt.clone(),
            sha256: digest_file(&receipt).unwrap(),
        };
        pin.verify_receipt(Kind::Rust).unwrap();
        assert!(pin.verify_receipt(Kind::Cargo).is_err());
        let mut wrong = pin.clone();
        wrong.sha256 = "0".repeat(64);
        assert!(wrong.verify_receipt(Kind::Rust).is_err());
        let alias = base.join("alias.json");
        std::os::unix::fs::symlink(&receipt, &alias).unwrap();
        wrong = pin.clone();
        wrong.receipt = alias;
        assert!(wrong.verify_receipt(Kind::Rust).is_err());
        std::fs::write(&receipt, "{}").unwrap();
        assert!(pin.verify_receipt(Kind::Rust).is_err());
    }
}
