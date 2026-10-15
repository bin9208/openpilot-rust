//! Lazy pinned external CPUExecutionProvider; model interpretation stays native.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
#[cfg(feature = "native-skip-miri")]
#[expect(
    unsafe_code,
    reason = "audited CXX boundary to verified external ORT ABI"
)]
mod bridge;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("ORT contract: {0}")]
    Contract(&'static str),
    #[error("ORT native provider: {0}")]
    Native(String),
}
fn hash(path: &Path) -> Result<String, Error> {
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub struct Provider {
    library: PathBuf,
}
impl Provider {
    pub fn verified(root: &Path) -> Result<Self, Error> {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../upstream/signal-runtime.json"))?;
        let architecture = if cfg!(target_arch = "x86_64") {
            "x64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            return Err(Error::Contract("unsupported provider architecture"));
        };
        let files = fixture["onnxruntime"]["providers"][architecture]["files"]
            .as_object()
            .ok_or(Error::Contract("missing provider identity"))?;
        for (name, expected) in files {
            if hash(&root.join(name))?
                != expected
                    .as_str()
                    .ok_or(Error::Contract("invalid provider hash"))?
            {
                return Err(Error::Contract("provider identity mismatch"));
            }
        }
        Ok(Self {
            library: fs::canonicalize(root.join("lib/libonnxruntime.so.1.23.2"))?,
        })
    }
    pub fn library_path(&self) -> &Path {
        &self.library
    }
}
#[derive(Debug)]
pub struct Tensor {
    pub shape: Vec<i64>,
    pub values: Vec<f32>,
}
#[cfg(feature = "native-skip-miri")]
pub struct Session {
    native: cxx::UniquePtr<bridge::ffi::Session>,
    _owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[cfg(feature = "native-skip-miri")]
impl Session {
    pub fn load(provider: &Provider, model: &Path, expected_sha256: &str) -> Result<Self, Error> {
        if hash(model)? != expected_sha256 {
            return Err(Error::Contract("ONNX model identity mismatch"));
        }
        let library = provider
            .library
            .to_str()
            .ok_or(Error::Contract("provider path is not UTF-8"))?;
        let path = model
            .to_str()
            .ok_or(Error::Contract("model path is not UTF-8"))?;
        if library.contains('\0') || path.contains('\0') {
            return Err(Error::Contract("NUL in model/provider path"));
        }
        let native = bridge::ffi::load(library, path).map_err(|e| Error::Native(e.to_string()))?;
        Ok(Self {
            native,
            _owner: std::marker::PhantomData,
        })
    }
    pub fn run(&mut self, name: &str, input: &[f32], shape: &[i64]) -> Result<Vec<Tensor>, Error> {
        let length = shape
            .iter()
            .try_fold(1_usize, |n, &d| {
                usize::try_from(d)
                    .ok()
                    .filter(|&d| d > 0)
                    .and_then(|d| n.checked_mul(d))
            })
            .ok_or(Error::Contract("invalid tensor shape"))?;
        if shape.is_empty() || length != input.len() || name.contains('\0') {
            return Err(Error::Contract("input name/shape mismatch"));
        }
        let outputs = bridge::ffi::run(self.native.pin_mut(), name, input, shape)
            .map_err(|e| Error::Native(e.to_string()))?;
        Ok(outputs
            .into_iter()
            .map(|t| Tensor {
                shape: t.shape,
                values: t.values,
            })
            .collect())
    }
}
