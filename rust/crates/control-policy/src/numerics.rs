use crate::Error;
use libloading::Library;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr},
    fs,
    path::Path,
};

type Sgemv = unsafe extern "C" fn(
    i32,
    i32,
    i64,
    i64,
    f32,
    *const f32,
    i64,
    *const f32,
    i64,
    f32,
    *mut f32,
    i64,
);
type Dgemv = unsafe extern "C" fn(
    i32,
    i32,
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
type Sdot = unsafe extern "C" fn(i64, *const f32, i64, *const f32, i64) -> f32;
type Ddot = unsafe extern "C" fn(i64, *const f64, i64, *const f64, i64) -> f64;

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
    sgemv: Sgemv,
    dgemv: Dgemv,
    sdot: Sdot,
    ddot: Ddot,
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
                return Err(Error::Contract("numerical artifact filename"));
            }
            if format!(
                "{:x}",
                Sha256::digest(fs::read(directory.join(&file.name))?)
            ) != file.sha256
            {
                return Err(Error::Contract("numerical artifact hash mismatch"));
            }
        }
        // SAFETY: selected artifact is trusted native code; the owned library outlives
        // all function pointers, whose signatures match the pinned OpenBLAS ILP64 ABI.
        unsafe {
            let library = Library::new(directory.join(&manifest.library))?;
            let config = library.get::<unsafe extern "C" fn() -> *const c_char>(
                b"scipy_openblas_get_config64_\0",
            )?;
            let config = CStr::from_ptr(config()).to_string_lossy();
            if !config.contains("OpenBLAS 0.3.34.106.0") || !config.contains("USE64BITINT") {
                return Err(Error::Contract("numerical artifact version/ABI mismatch"));
            }
            let sgemv = *library.get::<Sgemv>(b"scipy_cblas_sgemv64_\0")?;
            let dgemv = *library.get::<Dgemv>(b"scipy_cblas_dgemv64_\0")?;
            let sdot = *library.get::<Sdot>(b"scipy_cblas_sdot64_\0")?;
            let ddot = *library.get::<Ddot>(b"scipy_cblas_ddot64_\0")?;
            Ok(Self {
                _library: library,
                sgemv,
                dgemv,
                sdot,
                ddot,
            })
        }
    }
    pub fn float_matrix(
        &self,
        input: &[f32],
        columns: usize,
        weights: &[f32],
    ) -> Result<Vec<f32>, Error> {
        let (rows, columns_i64) = dimensions(input.len(), columns, weights.len())?;
        let mut output = vec![0.; columns];
        // SAFETY: checked weights have rows*columns elements in Fortran order;
        // input rows and output columns are distinct owned buffers, unit strides.
        unsafe {
            if columns == 1 {
                output[0] = (self.sdot)(rows, input.as_ptr(), 1, weights.as_ptr(), 1);
            } else {
                (self.sgemv)(
                    102,
                    112,
                    rows,
                    columns_i64,
                    1.,
                    weights.as_ptr(),
                    rows,
                    input.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                );
            }
        }
        Ok(output)
    }
    pub fn double_matrix(
        &self,
        input: &[f64],
        columns: usize,
        weights: &[f64],
    ) -> Result<Vec<f64>, Error> {
        let (rows, columns_i64) = dimensions(input.len(), columns, weights.len())?;
        let mut output = vec![0.; columns];
        // SAFETY: checked weights have rows*columns elements in C order;
        // input rows and output columns are distinct owned buffers, unit strides.
        unsafe {
            if columns == 1 {
                output[0] = (self.ddot)(rows, input.as_ptr(), 1, weights.as_ptr(), 1);
            } else {
                (self.dgemv)(
                    101,
                    112,
                    rows,
                    columns_i64,
                    1.,
                    weights.as_ptr(),
                    columns_i64,
                    input.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                );
            }
        }
        Ok(output)
    }
}
fn dimensions(rows: usize, columns: usize, size: usize) -> Result<(i64, i64), Error> {
    if rows == 0
        || columns == 0
        || rows > 4096
        || columns > 4096
        || rows.checked_mul(columns) != Some(size)
    {
        return Err(Error::Contract("neural matrix dimensions"));
    }
    Ok((
        i64::try_from(rows).map_err(|_| Error::Contract("matrix rows"))?,
        i64::try_from(columns).map_err(|_| Error::Contract("matrix columns"))?,
    ))
}
