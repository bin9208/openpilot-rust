//! Explicit admission for the four historically pinned driving oracle leaves.
use super::Binding;
use crate::seal::Result;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const RETIRED: [(&str, &str); 4] = [
    (
        "rust/tools/check_driving_daemon.py",
        "7e018d91804e4a26a29c28be9be12ddab7cdb26cc72c3098f41e2939704680bf",
    ),
    (
        "rust/tools/check_model_logging_faults.py",
        "f1330f5395a0181bf2f11722e175ad3fa5aa3d28ddb42dd159a0339a939ee1d3",
    ),
    (
        "rust/tools/driving_daemon_reference.py",
        "e20e38e9fba65667b62d012927bd575c74c6d213001edae388f5e8127a8df266",
    ),
    (
        "rust/tools/check_desire_reference.py",
        "b5529bcac8036ec559de785319b8bd131963fe05eb9aa149528992f7b4f88f8e",
    ),
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePolicy {
    Present,
    RetiredDriving,
}
impl SourcePolicy {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "present" => Ok(Self::Present),
            "retired-driving" => Ok(Self::RetiredDriving),
            _ => Err("source policy must be present or retired-driving".into()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::RetiredDriving => "retired-driving",
        }
    }
}
pub(super) fn verify_absent(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(format!(
                    "retired driving source present or uninspectable: {}",
                    path.display()
                )
                .into());
            }
        }
    }
    Ok(())
}
pub(crate) fn check_absence(root: &Path, policy: SourcePolicy) -> Result<()> {
    if policy == SourcePolicy::RetiredDriving {
        verify_absent(
            &RETIRED
                .iter()
                .map(|(name, _)| root.join(name))
                .collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

pub(super) fn bind(
    root: &Path,
    pins: &Map<String, Value>,
    policy: SourcePolicy,
    bindings: &mut Vec<Binding>,
) -> Result<Vec<PathBuf>> {
    let mut absent = Vec::new();
    if policy == SourcePolicy::RetiredDriving {
        for (name, sha) in RETIRED {
            if pins.get(name).and_then(Value::as_str) != Some(sha) {
                return Err(format!("retired driving historical pin changed: {name}").into());
            }
            absent.push(root.join(name));
        }
        verify_absent(&absent)?;
    }
    for (name, pin) in pins {
        if !Path::new(name)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("source pin escaped checkout".into());
        }
        if policy == SourcePolicy::RetiredDriving
            && RETIRED.iter().any(|(retired, _)| retired == name)
        {
            continue;
        }
        bindings.push(Binding::open(
            root.join(name),
            Some(pin.as_str().ok_or("source SHA-256 missing")?),
            4_000_000,
        )?);
    }
    Ok(absent)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    fn pins() -> Map<String, Value> {
        RETIRED
            .iter()
            .map(|(name, sha)| (name.to_string(), Value::String(sha.to_string())))
            .collect()
    }
    #[test]
    fn explicit_absence_rejects_missing_history_reappearance_links_and_directories() {
        let root = tempfile::tempdir().unwrap();
        let pins = pins();
        let mut bindings = Vec::new();
        assert!(bind(root.path(), &pins, SourcePolicy::Present, &mut bindings).is_err());
        let paths = bind(
            root.path(),
            &pins,
            SourcePolicy::RetiredDriving,
            &mut bindings,
        )
        .unwrap();
        for path in &paths {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"reappeared").unwrap();
            assert!(verify_absent(&paths).is_err());
            fs::remove_file(path).unwrap();
            symlink("missing-target", path).unwrap();
            assert!(verify_absent(&paths).is_err());
            fs::remove_file(path).unwrap();
            fs::create_dir(path).unwrap();
            assert!(verify_absent(&paths).is_err());
            fs::remove_dir(path).unwrap();
        }
        assert_eq!(paths.len(), RETIRED.len());
        for (name, _) in RETIRED {
            let mut changed = pins.clone();
            changed[name] = Value::String("a".repeat(64));
            assert!(
                bind(
                    root.path(),
                    &changed,
                    SourcePolicy::RetiredDriving,
                    &mut bindings
                )
                .is_err(),
                "changed historical pin: {name}"
            );
            changed = pins.clone();
            changed.remove(name);
            assert!(
                bind(
                    root.path(),
                    &changed,
                    SourcePolicy::RetiredDriving,
                    &mut bindings
                )
                .is_err(),
                "missing historical pin: {name}"
            );
        }
    }
    #[test]
    fn remaining_source_stays_bound_and_retirement_is_not_autodetected() {
        let root = tempfile::tempdir().unwrap();
        let mut pins = pins();
        let path = root.path().join("runtime.rs");
        fs::write(&path, b"original").unwrap();
        pins.insert(
            "runtime.rs".into(),
            Value::String(super::super::digest(b"original")),
        );
        let mut bindings = Vec::new();
        bind(
            root.path(),
            &pins,
            SourcePolicy::RetiredDriving,
            &mut bindings,
        )
        .unwrap();
        assert_eq!(bindings.len(), 1);
        bindings[0].verify().unwrap();
        fs::write(path, b"changed").unwrap();
        assert!(bindings[0].verify().is_err());
        assert!(SourcePolicy::parse("auto").is_err());
    }
}
