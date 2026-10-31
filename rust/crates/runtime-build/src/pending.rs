//! A verified update becomes active only at the next native boot boundary.
//! The journal remains valid if power is lost after active.json is replaced.
use crate::{
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

fn verify(
    config_path: &Path,
    config: &Config,
    configuration_sha256: &str,
    pending: &Pending,
) -> Result<()> {
    Config::verify_pin(config_path, configuration_sha256)?;
    if pending.schema != 1 || pending.configuration_sha256 != configuration_sha256 {
        return Err("pending native update configuration changed".into());
    }
    current(config, pending)?;
    let sealed = generation::load(&config.stores, &pending.candidate)?;
    if sealed.runtime.source.commit != pending.source_commit {
        return Err("pending native update source mismatch".into());
    }
    Config::verify_pin(config_path, configuration_sha256)?;
    Ok(())
}

pub fn ready(config_path: &Path) -> Result<Option<Pending>> {
    let (config, pin) = Config::read_pinned(config_path)?;
    ready_with(config_path, &config, &pin)
}

pub fn ready_pinned(config_path: &Path, pin: &str) -> Result<Option<Pending>> {
    let config = Config::read_expected(config_path, pin)?;
    ready_with(config_path, &config, pin)
}

fn ready_with(config_path: &Path, config: &Config, pin: &str) -> Result<Option<Pending>> {
    let path = config.stores.generations.join("pending.json");
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Config::verify_pin(config_path, pin)?;
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let pending = generation::read(&path)?;
    verify(config_path, config, pin, &pending)?;
    Ok(Some(pending))
}

pub fn stage(config_path: &Path, candidate: &str, expected_previous: &str) -> Result<Pending> {
    let (config, pin) = Config::read_pinned(config_path)?;
    stage_with(config_path, &config, &pin, candidate, expected_previous)
}

pub fn stage_pinned(
    config_path: &Path,
    pin: &str,
    candidate: &str,
    expected_previous: &str,
) -> Result<Pending> {
    let config = Config::read_expected(config_path, pin)?;
    stage_with(config_path, &config, pin, candidate, expected_previous)
}

/// A newly prepared input configuration builds the candidate; the running
/// launch configuration remains the stable owner of the boot journal. Never
/// replace that live configuration with a later plan just to make SHA checks
/// pass. The sealed candidate carries its own complete source/input records.
pub fn stage_prepared_pinned(
    running_path: &Path,
    running_pin: &str,
    candidate_path: &Path,
    candidate_pin: &str,
    source: &Path,
    candidate: &str,
    expected_previous: &str,
) -> Result<Pending> {
    let running = Config::read_expected(running_path, running_pin)?;
    let requested = Config::read_expected(candidate_path, candidate_pin)?;
    crate::supervision::check_request_owner()?;
    if serde_json::to_value(&running.stores)? != serde_json::to_value(&requested.stores)? {
        return Err("prepared update belongs to different native generation stores".into());
    }
    let before = crate::source::inspect(source)?;
    let sealed = generation::load(&requested.stores, candidate)?;
    let plan = crate::prepare::Plan::read(&requested.build_plan)?;
    let layout: generation::Layout = generation::read(&requested.layout)?;
    if sealed.runtime.source != before
        || serde_json::to_value(&sealed.prepared.plan)? != serde_json::to_value(&plan)?
        || serde_json::to_value(&sealed.layout)? != serde_json::to_value(&layout)?
        || sealed
            .prepared
            .reports
            .values()
            .any(|r| r.toolchain_directory != requested.toolchain)
    {
        return Err("prepared update differs from its actual source, build plan or layout".into());
    }
    Config::verify_pin(candidate_path, candidate_pin)?;
    if crate::source::inspect(source)? != before {
        return Err("prepared update source changed before staging".into());
    }
    stage_with(
        running_path,
        &running,
        running_pin,
        candidate,
        expected_previous,
    )
}

fn stage_with(
    config_path: &Path,
    config: &Config,
    pin: &str,
    candidate: &str,
    expected_previous: &str,
) -> Result<Pending> {
    Config::verify_pin(config_path, pin)?;
    generation::directory(&config.stores.generations)?;
    let _lock = generation::lock(&config.stores.generations)?;
    generation::offroad(&config.stores.params)?;
    if selection::resolve(&config.stores)?.id != expected_previous {
        return Err("active native generation changed during build; candidate not staged".into());
    }
    let sealed = generation::load(&config.stores, candidate)?;
    let pending = Pending {
        schema: 1,
        configuration_sha256: pin.into(),
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
        verify(config_path, config, pin, &previous)?;
    } else {
        generation::offroad(&config.stores.params)?;
        Config::verify_pin(config_path, pin)?;
        crate::supervision::check_request_owner()?;
        generation::write(&path, &pending, false)?;
    }
    Config::verify_pin(config_path, pin)?;
    Ok(pending)
}

/// Called before any manager starts, while the launcher owns the boot lock.
/// Repeating this after an interrupted activation completes the same journal.
pub fn activate_at_boot(config_path: &Path) -> Result<Option<selection::Selected>> {
    let (config, pin) = Config::read_pinned(config_path)?;
    activate_with(config_path, &config, &pin)
}

pub fn activate_at_boot_pinned(
    config_path: &Path,
    pin: &str,
) -> Result<Option<selection::Selected>> {
    let config = Config::read_expected(config_path, pin)?;
    activate_with(config_path, &config, pin)
}

fn activate_with(
    config_path: &Path,
    config: &Config,
    pin: &str,
) -> Result<Option<selection::Selected>> {
    Config::verify_pin(config_path, pin)?;
    generation::directory(&config.stores.generations)?;
    let _lock = generation::lock(&config.stores.generations)?;
    let Some(pending) = ready_with(config_path, config, pin)? else {
        return Ok(None);
    };
    generation::offroad(&config.stores.params)?;
    Config::verify_pin(config_path, pin)?;
    let selected = selection::activate_locked(&config.stores, &pending.candidate)?;
    Config::verify_pin(config_path, pin)?;
    fs::remove_file(config.stores.generations.join("pending.json"))?;
    fs::File::open(&config.stores.generations)?.sync_all()?;
    Ok(Some(selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{digest_file, generation::Stores};

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
        let original_config = fs::read(&config_path).unwrap();
        let (_, pin) = Config::read_pinned(&config_path).unwrap();
        let mut changed = config.clone();
        changed.cache = root.join("other-cache");
        generation::write(&config_path, &changed, true).unwrap();
        fs::write(&pending_path, b"owned previous pending bytes").unwrap();
        for result in [
            ready_pinned(&config_path, &pin).map(|_| ()),
            stage_pinned(&config_path, &pin, &"b".repeat(64), &"a".repeat(64)).map(|_| ()),
            activate_at_boot_pinned(&config_path, &pin).map(|_| ()),
        ] {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("configuration changed"));
            assert_eq!(fs::read(&active_path).unwrap(), active);
            assert_eq!(
                fs::read(&pending_path).unwrap(),
                b"owned previous pending bytes"
            );
            assert_eq!(
                fs::read(root.join("normal.log")).unwrap(),
                b"normal previous runtime log"
            );
        }
        assert!(!changed.cache.exists());
        fs::write(&config_path, original_config).unwrap();
        fs::remove_file(&pending_path).unwrap();
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
