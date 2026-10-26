//! A verified update becomes active only at the next native boot boundary.
//! The journal remains valid if power is lost after active.json is replaced.
use crate::{
    digest_file,
    generation::{self, Config},
    selection, Result,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub schema: u32,
    pub configuration_sha256: String,
    pub expected_previous: String,
    pub candidate: String,
    pub source_commit: String,
}

fn current(config: &Config, pending: &Pending) -> Result<selection::Selected> {
    let current = selection::resolve(&config.stores)?;
    if current.id != pending.expected_previous && current.id != pending.candidate {
        return Err("pending native update was prepared for a different active generation".into());
    }
    Ok(current)
}

fn verify(config_path: &Path, config: &Config, pending: &Pending) -> Result<()> {
    if pending.schema != 1 || pending.configuration_sha256 != digest_file(config_path)? {
        return Err("pending native update configuration changed".into());
    }
    current(config, pending)?;
    let sealed = generation::load(&config.stores, &pending.candidate)?;
    if sealed.runtime.source.commit != pending.source_commit {
        return Err("pending native update source mismatch".into());
    }
    Ok(())
}

pub fn ready(config_path: &Path) -> Result<Option<Pending>> {
    let config = Config::read(config_path)?;
    let path = config.stores.generations.join("pending.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let pending = generation::read(&path)?;
    verify(config_path, &config, &pending)?;
    Ok(Some(pending))
}

pub fn stage(config_path: &Path, candidate: &str, expected_previous: &str) -> Result<Pending> {
    let config = Config::read(config_path)?;
    generation::directory(&config.stores.generations)?;
    let _lock = generation::lock(&config.stores.generations)?;
    generation::offroad(&config.stores.params)?;
    if selection::resolve(&config.stores)?.id != expected_previous {
        return Err("active native generation changed during build; candidate not staged".into());
    }
    let sealed = generation::load(&config.stores, candidate)?;
    let pending = Pending {
        schema: 1,
        configuration_sha256: digest_file(config_path)?,
        expected_previous: expected_previous.into(),
        candidate: candidate.into(),
        source_commit: sealed.runtime.source.commit,
    };
    let path = config.stores.generations.join("pending.json");
    if fs::symlink_metadata(&path).is_ok() {
        let previous: Pending = generation::read(&path)?;
        if previous != pending {
            return Err("another native update is pending; existing candidate retained".into());
        }
        verify(config_path, &config, &previous)?;
    } else {
        generation::offroad(&config.stores.params)?;
        generation::write(&path, &pending, false)?;
    }
    Ok(pending)
}

/// Called before any manager starts, while the launcher owns the boot lock.
/// Repeating this after an interrupted activation completes the same journal.
pub fn activate_at_boot(config_path: &Path) -> Result<Option<selection::Selected>> {
    let config = Config::read(config_path)?;
    generation::directory(&config.stores.generations)?;
    let _lock = generation::lock(&config.stores.generations)?;
    let Some(pending) = ready(config_path)? else {
        return Ok(None);
    };
    generation::offroad(&config.stores.params)?;
    let selected = selection::activate_locked(&config.stores, &pending.candidate)?;
    fs::remove_file(config.stores.generations.join("pending.json"))?;
    fs::File::open(&config.stores.generations)?.sync_all()?;
    Ok(Some(selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::Stores;

    #[test]
    fn absent_corrupt_changed_config_and_unverified_pending_updates_preserve_previous_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let config = Config {
            schema: 1,
            stores: Stores {
                sources: root.join("sources"),
                objects: root.join("objects"),
                generations: root.join("generations"),
                params: root.join("params"),
            },
            cache: root.join("cache"),
            toolchain: root.join("toolchain"),
            build_plan: root.join("plan.json"),
            layout: root.join("layout.json"),
            resources: crate::resources::Policy {
                schema: 1,
                memory_cgroup_parent: root.join("memory-cgroup"),
                maximum_memory_bytes: 1024 * 1024,
                minimum_available_memory_bytes: 1024 * 1024,
                filesystems: std::collections::BTreeMap::from([(
                    root.to_owned(),
                    crate::resources::DiskBudget {
                        maximum_growth_bytes: 1024 * 1024,
                        minimum_free_bytes: 1024 * 1024,
                    },
                )]),
            },
        };
        fs::create_dir(&config.stores.generations).unwrap();
        fs::create_dir_all(config.stores.params.join("d")).unwrap();
        fs::write(config.stores.params.join("d/IsOnroad"), b"0").unwrap();
        let config_path = root.join("config.json");
        generation::write(&config_path, &config, false).unwrap();
        let active_path = config.stores.generations.join("active.json");
        let active =
            serde_json::to_vec(&serde_json::json!({"schema":1,"candidates":["a".repeat(64)]}))
                .unwrap();
        fs::write(&active_path, &active).unwrap();
        fs::write(root.join("normal.log"), b"normal previous runtime log").unwrap();
        assert!(ready(&config_path).unwrap().is_none());
        assert!(activate_at_boot(&config_path).unwrap().is_none());
        let pending_path = config.stores.generations.join("pending.json");
        for contents in [
            b"{partial power-loss journal".to_vec(),
            serde_json::to_vec(&Pending {
                schema: 1,
                configuration_sha256: "wrong configuration".into(),
                expected_previous: "a".repeat(64),
                candidate: "b".repeat(64),
                source_commit: "c".repeat(40),
            })
            .unwrap(),
            serde_json::to_vec(&Pending {
                schema: 1,
                configuration_sha256: digest_file(&config_path).unwrap(),
                expected_previous: "a".repeat(64),
                candidate: "b".repeat(64),
                source_commit: "c".repeat(40),
            })
            .unwrap(),
        ] {
            fs::write(&pending_path, &contents).unwrap();
            assert!(ready(&config_path).is_err());
            assert!(stage(&config_path, &"b".repeat(64), &"a".repeat(64)).is_err());
            assert!(activate_at_boot(&config_path).is_err());
            assert_eq!(fs::read(&active_path).unwrap(), active);
            assert_eq!(fs::read(&pending_path).unwrap(), contents);
            assert_eq!(
                fs::read(root.join("normal.log")).unwrap(),
                b"normal previous runtime log"
            );
        }
        let lock = generation::lock(&config.stores.generations).unwrap();
        assert!(activate_at_boot(&config_path).is_err());
        drop(lock);
        assert_eq!(fs::read(active_path).unwrap(), active);
    }
}
