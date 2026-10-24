//! Historical helper pins stay exact while a closed uploader helper set must
//! be absent. Every source outside that set still requires its current bytes.
//! Apply with the coordinated source retirement, never before caller closure.
use crate::seal::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path},
};
/// Exact captures that established the original helper observations. These are
/// source records, never outputs computed from the candidate Rust daemon.
#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    pub scope: &'static str,
    pub run: u64,
    pub source_commit: &'static str,
    pub artifact: u64,
    pub artifact_sha256: &'static str,
}
pub const ORIGINAL_CAPTURES: &[Evidence] = &[
    Evidence {
        scope: "ordinary and logging original source",
        run: 39758140178,
        source_commit: "06956b79d1d3ebacfa6e8a09616e9cdc1c358ed3",
        artifact: 12417680413,
        artifact_sha256: "7be58982b6ad420fb97a2085e1e782bec5edb7ca3c77d8e2019eab731c8a899e",
    },
    Evidence {
        scope: "sibling original source only",
        run: 39770735490,
        source_commit: "67f099bffbdda2396ea2f64e1f168a76ce19d560",
        artifact: 12422761878,
        artifact_sha256: "69f9c16983a0699d7478455016b94dade68374e346b72a12a1e669354ad8abee",
    },
];
/// Accepted scope prefixes. The R5 enclosing job failed later in logging; its
/// complete ordinary 212/10000/32 prefix was audited independently, not relabeled
/// as a successful whole-job run. R6 completed logging successfully.
pub const ACCEPTED_PREFIX_PROOFS: &[Evidence] = &[
    Evidence {
        scope: "ordinary accepted prefix:212filesystem/10000backoff/32HTTP",
        run: 39767931037,
        source_commit: "67f099bffbdda2396ea2f64e1f168a76ce19d560",
        artifact: 12421835922,
        artifact_sha256: "536ead05a4deffced043f2b1a6fbf9ed3c849444e927e9c33c1d0f972158424a",
    },
    Evidence {
        scope: "logging accepted:18cases/36collector rows",
        run: 39770418874,
        source_commit: "40f37bea345009bfcadf2275916f4322af45ffba",
        artifact: 12422391683,
        artifact_sha256: "23ac056a8d725868214bcf03aa9d8e99d9929ea99f6388af7bd504ecbc19df16",
    },
];
/// Complete successful sibling and affected-logging rerun after the #492
/// ownership correction. All fifteen tests, final eleven provider/input seals,
/// all thirty fault/fourteen Params/thirty-six logging rows were audited. The
/// original ordinary 212/10000/32 traffic was reused without repeating it.
pub const ACCEPTED_GROUP_PROOFS: &[Evidence] = &[Evidence {
    scope: "sibling and affected logging:#492/15tests/80collector rows",
    run: 39783963192,
    source_commit: "e5dfb68a41343ac2f51c8b9f002111656cf303c5",
    artifact: 12427719093,
    artifact_sha256: "ec4a5f238befa9b4ffbd546a887ec503276e3799869a00b5432a4f535ebddb27",
}];
// Apply only with both route consumer closures and the coordinated maintained
// caller/source-absence gate. Failed R1/R2 raw evidence is retained separately;
// neither failed enclosing run is relabeled successful by this continuation.
pub const RETIRED: &[(&str, &str)] = &[
    (
        "rust/tools/check_uploader.py",
        "b90a41b13ed1a9a99e23c771feaea3ad97ff9999c394e309e9d714f69f10a291",
    ),
    (
        "rust/tools/check_uploader_logging.py",
        "bdac9f00bbb0434a81f535aac7aa1b95007fd80a28aa52b4ede1060bb17ff637",
    ),
    (
        "rust/tools/check_uploader_daemon.py",
        "1276e801ea79cd68142d362c4e31eb6c3cd5820404c3e1503cab3bc19358601c",
    ),
    (
        "rust/tools/check_uploader_interrupted.py",
        "c08676ad7b68c063e427f8cc90e2d9fd6edb0c4369f8704b39f6e458bdb6bf23",
    ),
    (
        "rust/tools/check_uploader_http_interrupted.py",
        "94318a796d6f1b2bb75c8e8b5d5bf290fa90bf035efb2e541b7cb2edf9eddc77",
    ),
    (
        "rust/tools/check_uploader_timeouts.py",
        "10a447b0672becd978f0647bf18b335413b39d6826aed60966bf644c9d5d25e1",
    ),
    (
        "rust/tools/check_uploader_log_fault.py",
        "4daa7396db6927414ca4088705390e74a5f2264829386db4e74189ba546ef3fc",
    ),
    (
        "rust/tools/check_params_string_uploader.py",
        "6444445db87f1ea109eeb7cfe8c5da7f0e98e7d5e57934d201d8fb6b334fe580",
    ),
];
pub fn validate(root: &Path, name: &str, expected: &str) -> Result<()> {
    if !Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(format!("unsafe source pin: {name}").into());
    }
    let path = root.join(name);
    if let Some((_, original)) = RETIRED.iter().find(|(old, _)| *old == name) {
        if expected != *original {
            return Err(format!("historical uploader helper pin changed: {name}").into());
        }
        return match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(format!("retired uploader helper must be absent: {name}").into()),
        };
    }
    let meta =
        fs::symlink_metadata(&path).map_err(|e| format!("retained source missing: {name}: {e}"))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!("retained source is not a regular file: {name}").into());
    }
    let mut bytes = Vec::new();
    File::open(&path)?
        .take((8 << 20) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 8 << 20 || format!("{:x}", Sha256::digest(&bytes)) != expected {
        return Err(format!("retained source changed: {name}").into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn retired_pins_require_exact_historical_hash_and_complete_path_absence() {
        let root = tempfile::tempdir().unwrap();
        for &(name, pin) in RETIRED {
            assert!(validate(root.path(), name, pin).is_ok());
            assert!(validate(root.path(), name, &"0".repeat(64)).is_err());
            let path = root.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"reintroduced helper").unwrap();
            assert!(validate(root.path(), name, pin).is_err());
            fs::remove_file(&path).unwrap();
            symlink("nonexistent-original-helper", &path).unwrap();
            assert!(validate(root.path(), name, pin).is_err());
            fs::remove_file(&path).unwrap();
        }
    }
    #[test]
    fn unlisted_retained_sources_never_pass_by_missing_or_changed_bytes() {
        let root = tempfile::tempdir().unwrap();
        let name = "retained/runtime.rs";
        let path = root.path().join(name);
        let expected = format!("{:x}", Sha256::digest(b"retained original"));
        assert!(validate(root.path(), name, &expected).is_err());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"retained original").unwrap();
        assert!(validate(root.path(), name, &expected).is_ok());
        fs::write(&path, b"changed").unwrap();
        assert!(validate(root.path(), name, &expected).is_err());
        fs::remove_file(&path).unwrap();
        let target = root.path().join("target");
        fs::write(&target, b"retained original").unwrap();
        symlink(&target, &path).unwrap();
        assert!(validate(root.path(), name, &expected).is_err());
        assert!(validate(root.path(), "../outside.rs", &expected).is_err());
        assert!(validate(root.path(), "/absolute.rs", &expected).is_err());
    }
}
