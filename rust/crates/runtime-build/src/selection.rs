//! A durable pointer to fully sealed source-built runtimes, retaining fallbacks.
use crate::{
    generation::{self, Sealed, Stores},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    /// Current followed by up to two previously selected, verified generations.
    candidates: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Selected {
    pub id: String,
    pub source_commit: String,
    pub source_root: PathBuf,
    pub recovered_previous: bool,
    pub rejected: Vec<Rejected>,
}

#[derive(Debug, Serialize)]
pub struct Rejected {
    pub id: String,
    pub reason: String,
}

impl State {
    fn validate(&self) -> Result<()> {
        let unique: BTreeSet<_> = self.candidates.iter().collect();
        if self.schema != 1
            || self.candidates.is_empty()
            || self.candidates.len() > 3
            || unique.len() != self.candidates.len()
            || self.candidates.iter().any(|id| {
                id.len() != 64
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err("invalid active source-generation state".into());
        }
        Ok(())
    }
}

fn state(stores: &Stores) -> Result<State> {
    let state: State = generation::read(&stores.generations.join("active.json"))?;
    state.validate()?;
    Ok(state)
}

/// Select only after complete verification and a final offroad check. A failed
/// validation, cancellation, or partial write leaves the old atomic pointer.
pub fn activate(stores: &Stores, id: &str) -> Result<Selected> {
    generation::directory(&stores.generations)?;
    let _lock = generation::lock(&stores.generations)?;
    activate_locked(stores, id)
}

pub(crate) fn activate_locked(stores: &Stores, id: &str) -> Result<Selected> {
    generation::offroad(&stores.params)?;
    let sealed = generation::load(stores, id)?;
    let mut candidates = vec![id.to_owned()];
    match fs::symlink_metadata(stores.generations.join("active.json")) {
        Ok(_) => {
            let previous = state(stores)?;
            for candidate in previous.candidates {
                if candidate != id && generation::load(stores, &candidate).is_ok() {
                    candidates.push(candidate);
                    if candidates.len() == 3 {
                        break;
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    let state = State {
        schema: 1,
        candidates,
    };
    state.validate()?;
    generation::offroad(&stores.params)?;
    generation::write(&stores.generations.join("active.json"), &state, true)?;
    Ok(selected(id.to_owned(), sealed, Vec::new()))
}

fn selected(id: String, sealed: Sealed, rejected: Vec<Rejected>) -> Selected {
    Selected {
        id,
        source_commit: sealed.runtime.source.commit,
        source_root: sealed.runtime.root,
        recovered_previous: !rejected.is_empty(),
        rejected,
    }
}

/// Revalidate the runtime that will actually execute, including its own source.
/// Corruption of the newest candidate may select a recorded, verified prior one;
/// foreign binaries are never relabeled as the requested checkout's result.
pub fn resolve(stores: &Stores) -> Result<Selected> {
    Ok(resolve_verified(stores)?.0)
}

/// Retain the verified descriptor for a caller which must construct its loader
/// environment. Do not hash the entire same generation a second time for that.
pub fn resolve_verified(stores: &Stores) -> Result<(Selected, Sealed)> {
    let state = state(stores)?;
    let mut rejected = Vec::new();
    for candidate in state.candidates {
        match generation::load(stores, &candidate) {
            Ok(sealed) => {
                let selection = Selected {
                    id: candidate,
                    source_commit: sealed.runtime.source.commit.clone(),
                    source_root: sealed.runtime.root.clone(),
                    recovered_previous: !rejected.is_empty(),
                    rejected,
                };
                return Ok((selection, sealed));
            }
            Err(e) => rejected.push(Rejected {
                id: candidate,
                reason: e.to_string(),
            }),
        }
    }
    Err("no recorded source-built generation passed complete verification; state and logs preserved".into())
}

/// Called only by the launcher after taking its normal boot lock. An initial
/// request must never overwrite a generation another boot has already chosen.
pub fn initialize(stores: &Stores, id: &str) -> Result<Selected> {
    generation::directory(&stores.generations)?;
    let _lock = generation::lock(&stores.generations)?;
    match fs::symlink_metadata(stores.generations.join("active.json")) {
        Ok(_) => {
            let active = resolve(stores)?;
            if active.id != id {
                return Err(
                    "native initial selection changed before the boot lock was acquired".into(),
                );
            }
            Ok(active)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => activate_locked(stores, id),
        Err(e) => Err(e.into()),
    }
}

/// Set before exec: the dynamic loader must never borrow an older generation's
/// provider merely because that generation launched the update or bootstrap.
pub fn library_path(stores: &Stores, id: &str) -> Result<std::ffi::OsString> {
    let sealed = generation::load(stores, id)?;
    let mut libraries = vec![sealed.runtime.root.join("rust/bin/lib")];
    libraries.extend(sealed.layout.libraries);
    Ok(std::env::join_paths(libraries)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_or_busy_activation_keeps_previous_state_and_logs() {
        let temp = tempfile::tempdir().unwrap();
        let generations = temp.path().join("generations");
        fs::create_dir(&generations).unwrap();
        let params = temp.path().join("params");
        fs::create_dir_all(params.join("d")).unwrap();
        fs::write(params.join("d/IsOnroad"), b"0").unwrap();
        let stores = Stores {
            sources: temp.path().join("sources"),
            objects: temp.path().join("objects"),
            generations,
            params,
        };
        let previous = State {
            schema: 1,
            candidates: vec!["a".repeat(64)],
        };
        let pointer = stores.generations.join("active.json");
        generation::write(&pointer, &previous, false).unwrap();
        let bytes = fs::read(&pointer).unwrap();
        let log = stores.generations.join("normal.log");
        fs::write(&log, b"previous normal runtime log").unwrap();
        assert!(activate(&stores, &"b".repeat(64)).is_err());
        assert!(initialize(&stores, &"b".repeat(64)).is_err());
        let lock = generation::lock(&stores.generations).unwrap();
        assert!(activate(&stores, &"b".repeat(64)).is_err());
        drop(lock);
        fs::write(stores.params.join("d/IsOnroad"), b"1").unwrap();
        assert!(activate(&stores, &"b".repeat(64)).is_err());
        assert!(resolve(&stores).is_err());
        fs::write(
            &pointer,
            b"corrupt state must never be reset by initial startup",
        )
        .unwrap();
        let corrupt = fs::read(&pointer).unwrap();
        assert!(initialize(&stores, &"b".repeat(64)).is_err());
        assert_eq!(fs::read(&pointer).unwrap(), corrupt);
        fs::write(&pointer, &bytes).unwrap();
        assert_eq!(fs::read(pointer).unwrap(), bytes);
        assert_eq!(fs::read(log).unwrap(), b"previous normal runtime log");
    }
}
