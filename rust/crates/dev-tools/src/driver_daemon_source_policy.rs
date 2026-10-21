//! Historical driver oracle pins remain mandatory after explicit retirement.
use crate::seal::Result;
use std::{collections::BTreeMap, fs, path::Path};
pub const RETIRED: [(&str, &str); 3] = [
    (
        "rust/tools/check_driver_daemon.py",
        "ea1266dc395dab662319bd3637ad90440989db3d2937756dbf07fbb993520f5b",
    ),
    (
        "rust/tools/model_logging_capture.py",
        "f00fa03c10597fce6c53931b45cf8867baded88a75948c664ffd075f3042535d",
    ),
    (
        "rust/tools/model_logging_reference.py",
        "ced29c79268f0df83bd9f741c8c840c6111521909592bfd7ad7b5602f0182910",
    ),
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePolicy {
    Present,
    RetiredDriver,
}
impl SourcePolicy {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "present" => Ok(Self::Present),
            "retired-driver" => Ok(Self::RetiredDriver),
            _ => Err("driver source policy must be present or retired-driver".into()),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::RetiredDriver => "retired-driver",
        }
    }
    pub fn skips(self, name: &str) -> bool {
        self == Self::RetiredDriver && RETIRED.iter().any(|(path, _)| *path == name)
    }
    pub fn verify(self, root: &Path, original: &BTreeMap<String, String>) -> Result<()> {
        if self == Self::Present {
            return Ok(());
        }
        for (name, expected) in RETIRED {
            if original.get(name).map(String::as_str) != Some(expected) {
                return Err(format!("driver retired historical source pin changed: {name}").into());
            }
            match fs::symlink_metadata(root.join(name)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    return Err(
                        format!("retired driver source present or uninspectable: {name}").into(),
                    )
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn all_retired_driver_pins_and_regular_or_link_reappearance_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("rust/tools")).unwrap();
        let pins = RETIRED
            .iter()
            .map(|(n, s)| (n.to_string(), s.to_string()))
            .collect::<BTreeMap<_, _>>();
        SourcePolicy::RetiredDriver
            .verify(root.path(), &pins)
            .unwrap();
        for (name, _) in RETIRED {
            let path = root.path().join(name);
            fs::write(&path, b"reappeared").unwrap();
            assert!(SourcePolicy::RetiredDriver
                .verify(root.path(), &pins)
                .is_err());
            fs::remove_file(&path).unwrap();
            symlink(root.path().join("missing"), &path).unwrap();
            assert!(SourcePolicy::RetiredDriver
                .verify(root.path(), &pins)
                .is_err());
            fs::remove_file(&path).unwrap();
            let mut changed = pins.clone();
            changed.insert(name.to_owned(), "0".repeat(64));
            assert!(SourcePolicy::RetiredDriver
                .verify(root.path(), &changed)
                .is_err());
            changed.remove(name);
            assert!(SourcePolicy::RetiredDriver
                .verify(root.path(), &changed)
                .is_err());
        }
        assert!(!SourcePolicy::RetiredDriver.skips("rust/tools/check_pipeline_reference.py"));
        assert!(SourcePolicy::parse("retired-driving").is_err());
        SourcePolicy::Present
            .verify(root.path(), &BTreeMap::new())
            .unwrap();
    }
}
