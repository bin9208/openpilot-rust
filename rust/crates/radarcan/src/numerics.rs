use crate::Error;
use libloading::Library;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::{c_char, CStr},
    fs,
    path::Path,
};

pub(crate) type Dgesdd = unsafe extern "C" fn(
    *const c_char,
    *const i64,
    *const i64,
    *mut f64,
    *const i64,
    *mut f64,
    *mut f64,
    *const i64,
    *mut f64,
    *const i64,
    *mut f64,
    *const i64,
    *mut i64,
    *mut i64,
);
pub(crate) type Dgemm = unsafe extern "C" fn(
    i32,
    i32,
    i32,
    i64,
    i64,
    i64,
    f64,
    *const f64,
    i64,
    *const f64,
    i64,
    f64,
    *mut f64,
    i64,
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    numpy: String,
    abi: String,
    library: String,
    files: Vec<Artifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    name: String,
    sha256: String,
}

pub struct Numerics {
    _library: Library,
    gesdd: Dgesdd,
    pub(crate) gemm: Dgemm,
    pub(crate) point_dot: crate::numerics_dot::Dot,
    weights: BTreeMap<usize, Vec<f64>>,
}

impl Numerics {
    pub fn load(directory: &Path) -> Result<Self, Error> {
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        if manifest.format != 1
            || manifest.numpy != "2.5.3"
            || manifest.abi != "scipy_dgesdd_64_"
            || !manifest
                .files
                .iter()
                .any(|file| file.name == manifest.library)
        {
            return Err(Error::Contract(
                "NumPy 2.5.3 ILP64 numerical artifact required",
            ));
        }
        for file in &manifest.files {
            if Path::new(&file.name).components().count() != 1
                || file.name == "."
                || file.name == ".."
            {
                return Err(Error::Contract("numerical artifact filename invalid"));
            }
            if format!(
                "{:x}",
                Sha256::digest(fs::read(directory.join(&file.name))?)
            ) != file.sha256
            {
                return Err(Error::Contract("numerical artifact hash mismatch"));
            }
        }
        // SAFETY: the caller selects a trusted native artifact whose manifest fixes
        // the ILP64 ABI; the owned library outlives all copied function pointers.
        unsafe {
            let library = Library::new(directory.join(&manifest.library))?;
            let config = library.get::<unsafe extern "C" fn() -> *const c_char>(
                b"scipy_openblas_get_config64_\0",
            )?;
            let config = CStr::from_ptr(config()).to_string_lossy();
            if !config.contains("OpenBLAS 0.3.34.106.0") || !config.contains("USE64BITINT") {
                return Err(Error::Contract("numerical artifact version/ABI mismatch"));
            }
            let gesdd = *library.get::<Dgesdd>(b"scipy_dgesdd_64_\0")?;
            let gemm = *library.get::<Dgemm>(b"scipy_cblas_dgemm64_\0")?;
            let gemv = *library.get::<crate::numerics_dot::Dgemv>(b"scipy_cblas_dgemv64_\0")?;
            let dot = *library.get::<crate::numerics_dot::Ddot>(b"scipy_cblas_ddot64_\0")?;
            Ok(Self {
                _library: library,
                gesdd,
                gemm,
                point_dot: crate::numerics_dot::Dot { gemm, gemv, dot },
                weights: BTreeMap::new(),
            })
        }
    }

    pub fn jerk_weights(&mut self, count: usize) -> Result<&[f64], Error> {
        if !self.weights.contains_key(&count) {
            let weights = self.quadratic_weights(count)?;
            self.weights.insert(count, weights);
        }
        Ok(&self.weights[&count])
    }

    fn quadratic_weights(&self, count: usize) -> Result<Vec<f64>, Error> {
        crate::numerics_weights::quadratic_weights(count, self.gesdd, self.gemm)
    }
}
