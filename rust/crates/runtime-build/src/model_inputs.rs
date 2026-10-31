//! Model inputs share the source/receipt verifier used at source-built startup.
use crate::Result;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub use openpilot_runtime_installer::model_inputs::{source_namespace, CONTRACT};

pub fn verify(source: &Path, provider: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let source = source.canonicalize()?;
    let before = crate::source::inspect(&source)?;
    let files = openpilot_runtime_installer::model_inputs::verify(&source, provider)?;
    if crate::source::inspect(&source)? != before {
        return Err("model input source changed during verification".into());
    }
    Ok(files)
}

pub fn verify_mapped(
    source: &Path,
    provider: &Path,
    files: &BTreeMap<PathBuf, PathBuf>,
) -> Result<()> {
    let expected = verify(source, provider)?;
    for path in expected.keys() {
        if files.get(&Path::new("rust/bin").join(path)) != Some(path) {
            return Err(
                "native model output is missing or redirected in the runtime layout".into(),
            );
        }
    }
    for (target, path) in files {
        if let Ok(relative) = target.strip_prefix("rust/bin") {
            if ["driver-models", "driving-models", "model-inputs"]
                .iter()
                .any(|p| relative.starts_with(p))
                && (!expected.contains_key(relative) || path.as_path() != relative)
            {
                return Err("runtime layout adds an unverified model output".into());
            }
        }
    }
    Ok(())
}
