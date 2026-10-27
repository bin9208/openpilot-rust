//! Device-local resource/storage policy. Versioned input descriptors belong to
//! the requested Git source, not the running generation's old build plan.
use crate::{assets, catalog_bindings::Budgets, generation, resources::Policy, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const ENV: &str = "OPENPILOT_NATIVE_MACHINE_CONFIG";
pub const LAUNCH_PIN: &str = "OPENPILOT_NATIVE_LAUNCH_MACHINE_SHA256";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub stores: generation::Stores,
    pub cache: PathBuf,
    pub seed_cache: PathBuf,
    /// Overlay input checkouts live outside build/output stores, so a builder
    /// never writes around the very source it is validating.
    pub updater_staging: PathBuf,
    pub expected_source_growth_bytes: u64,
    pub expected_compile_growth_bytes: u64,
    pub remaining_object_growth_bytes: u64,
    pub compiler_growth_bytes: u64,
    pub resources: Policy,
    pub budgets: Budgets,
    /// Independent trusted boot-controller descriptor; preparation uses the
    /// requested source's own input index. Optional until boot is configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_seed: Option<assets::Asset>,
}

fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}

impl Config {
    pub fn read_pinned(path: &Path) -> Result<(Self, String)> {
        if !absolute(path) {
            return Err("native machine policy path must be absolute".into());
        }
        let parent = path
            .parent()
            .ok_or("native machine policy parent missing")?;
        if parent.canonicalize()? != parent {
            return Err("native machine policy parent is redirected".into());
        }
        let (config, pin): (Self, String) = generation::read_pinned(path)?;
        config.resources.validate()?;
        let roots = [
            &config.cache,
            &config.seed_cache,
            &config.updater_staging,
            &config.stores.sources,
            &config.stores.objects,
            &config.stores.generations,
            &config.stores.params,
        ];
        if config.schema != 1
            || roots.iter().any(|p| !absolute(p))
            || roots.iter().enumerate().any(|(i, a)| {
                roots
                    .iter()
                    .skip(i + 1)
                    .any(|b| a.starts_with(b) || b.starts_with(a))
            })
            || [
                config.expected_source_growth_bytes,
                config.expected_compile_growth_bytes,
                config.remaining_object_growth_bytes,
                config.compiler_growth_bytes,
                config.budgets.expected_source_growth_bytes,
                config.budgets.remaining_object_growth_bytes,
            ]
            .contains(&0)
            || config.budgets.schema != 1
            || config.budgets.group_growth_bytes.is_empty()
            || config.budgets.group_growth_bytes.values().any(|v| *v == 0)
            || config.budgets.estimate_basis.trim().is_empty()
            || config.budgets.estimate_basis.len() > 4096
        {
            return Err("native machine policy requires separate absolute stores and explicit resource/growth estimates".into());
        }
        for root in roots {
            let parent = root.parent().ok_or("native machine store parent missing")?;
            if parent.canonicalize()? != parent
                || root.symlink_metadata().is_ok_and(|m| !m.is_dir())
                || (root.exists() && root.canonicalize()? != *root)
            {
                return Err("native machine store or its parent is redirected".into());
            }
        }
        Ok((config, pin))
    }

    pub fn verify_pin(path: &Path, expected: &str) -> Result<()> {
        if Self::read_pinned(path)?.1 != expected {
            return Err("native machine policy changed during the operation".into());
        }
        Ok(())
    }

    pub fn verify_stores(&self, stores: &generation::Stores) -> Result<()> {
        if serde_json::to_value(&self.stores)? != serde_json::to_value(stores)? {
            return Err(
                "native machine policy belongs to different running generation stores".into(),
            );
        }
        Ok(())
    }

    pub fn verify_source_paths(&self, source: &Path) -> Result<()> {
        if !absolute(source)
            || self.updater_staging.starts_with(source)
            || [
                &self.cache,
                &self.seed_cache,
                &self.stores.sources,
                &self.stores.objects,
                &self.stores.generations,
                &self.stores.params,
            ]
            .iter()
            .any(|p| p.starts_with(source) || source.starts_with(p))
        {
            return Err("native machine writable stores overlap the requested source".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::DiskBudget;
    use std::{collections::BTreeMap, fs};

    fn fixture(root: &Path) -> Config {
        Config {
            schema: 1,
            stores: generation::Stores {
                sources: root.join("sources"),
                objects: root.join("objects"),
                generations: root.join("generations"),
                params: root.join("params"),
            },
            cache: root.join("cache"),
            seed_cache: root.join("seed"),
            updater_staging: root.join("updater"),
            expected_source_growth_bytes: 1,
            expected_compile_growth_bytes: 1,
            remaining_object_growth_bytes: 1,
            compiler_growth_bytes: 1,
            resources: Policy {
                schema: 1,
                memory_cgroup_parent: root.join("not-a-controller"),
                maximum_memory_bytes: 1,
                minimum_available_memory_bytes: 1,
                filesystems: BTreeMap::from([(
                    root.into(),
                    DiskBudget {
                        maximum_growth_bytes: 1,
                        minimum_free_bytes: 1,
                    },
                )]),
            },
            budgets: Budgets {
                schema: 1,
                group_growth_bytes: BTreeMap::from([("00".into(), 1)]),
                expected_source_growth_bytes: 1,
                remaining_object_growth_bytes: 1,
                estimate_basis: "Owned schema fixture, not a native build resource result".into(),
            },
            boot_seed: None,
        }
    }

    #[test]
    fn policy_pin_binds_each_operation_and_overlay_input_is_not_an_output_store() {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = fixture(temp.path());
        let path = temp.path().join("machine.json");
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let (initial, pin) = Config::read_pinned(&path).unwrap();
        initial.verify_stores(&policy.stores).unwrap();
        initial
            .verify_source_paths(&initial.updater_staging.join("generation/merged"))
            .unwrap();
        assert!(initial
            .verify_source_paths(&initial.stores.generations.join("legacy/merged"))
            .is_err());
        policy.resources.maximum_memory_bytes += 1;
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        assert!(Config::verify_pin(&path, &pin).is_err());
        assert!(!policy.cache.exists());
        assert!(!policy.updater_staging.exists());
    }

    #[test]
    fn policy_cannot_alias_a_store_or_hide_a_redirected_cache() {
        let temp = tempfile::tempdir().unwrap();
        let mut policy = fixture(temp.path());
        let path = temp.path().join("machine.json");
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let alias = temp.path().join("policy-parent-alias");
        std::os::unix::fs::symlink(temp.path(), &alias).unwrap();
        assert!(Config::read_pinned(&alias.join("machine.json"))
            .unwrap_err()
            .to_string()
            .contains("parent is redirected"));
        policy.updater_staging = policy.stores.generations.join("updater");
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        assert!(Config::read_pinned(&path).is_err());
        policy = fixture(temp.path());
        let elsewhere = temp.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &policy.cache).unwrap();
        fs::write(&path, serde_json::to_vec(&policy).unwrap()).unwrap();
        assert!(Config::read_pinned(&path)
            .unwrap_err()
            .to_string()
            .contains("redirected"));
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    }
}
