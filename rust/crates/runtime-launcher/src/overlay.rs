use crate::Error;
use std::{fs, path::Path, time::SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Unchanged,
    Activated,
}

fn newer(path: &Path, threshold: SystemTime) -> Result<bool, Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.modified()? > threshold {
        return Ok(true);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            if newer(&entry?.path(), threshold)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn activate(repository: &Path, staging: &Path) -> Result<Outcome, Error> {
    let marker = repository.join(".overlay_init");
    if !marker.is_file() {
        return Ok(Outcome::Unchanged);
    }
    if newer(&repository.join(".git"), fs::metadata(marker)?.modified()?)? {
        println!(
            "{} has been modified, skipping overlay update installation",
            repository.display()
        );
        return Ok(Outcome::Unchanged);
    }
    let finalized = staging.join("finalized");
    if !finalized.join(".overlay_consistent").is_file() {
        return Ok(Outcome::Unchanged);
    }
    let backup = staging.join("old_openpilot");
    if backup.exists() {
        println!("openpilot backup found, not updating");
        return Ok(Outcome::Unchanged);
    }
    fs::rename(repository, &backup)?;
    if let Err(error) = fs::rename(&finalized, repository) {
        if let Err(rollback) = fs::rename(&backup, repository) {
            return Err(Error::Configuration(format!("update activation failed: {error}; original checkout remains at {} because rollback failed: {rollback}", backup.display())));
        }
        return Err(error.into());
    }
    println!("Valid overlay update installed; restarting native launcher");
    Ok(Outcome::Activated)
}
