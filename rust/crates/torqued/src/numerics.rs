//! Checked native NumPy/OpenBLAS ILP64 dgesdd boundary; no Python runtime.
use crate::{Error, Fit};
use libloading::Library;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr},
    fs,
    path::Path,
};

type Dgesdd = unsafe extern "C" fn(
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
}
impl Numerics {
    /// Loads a trusted build artifact; hashes protect packaging consistency, not hostile code.
    pub fn load(directory: &Path) -> Result<Self, Error> {
        let manifest: Manifest = serde_json::from_slice(
            &fs::read(directory.join("manifest.json")).map_err(|source| Error::Artifact {
                path: directory.to_owned(),
                source,
            })?,
        )?;
        if manifest.format != 1
            || manifest.numpy != "2.4.6"
            || manifest.abi != "scipy_dgesdd_64_"
            || !manifest.files.iter().any(|f| f.name == manifest.library)
        {
            return Err(Error::Contract(
                "unsupported numerical manifest; NumPy2.4.6 ILP64 artifact required",
            ));
        }
        for file in &manifest.files {
            if Path::new(&file.name).components().count() != 1
                || file.name == "."
                || file.name == ".."
            {
                return Err(Error::Contract("numerical artifact filename invalid"));
            }
            let digest = format!(
                "{:x}",
                Sha256::digest(fs::read(directory.join(&file.name))?)
            );
            if digest != file.sha256 {
                return Err(Error::Contract("numerical artifact hash mismatch"));
            }
        }
        // SAFETY: caller explicitly selects a trusted native artifact. The manifest fixes the
        // NumPy OpenBLAS ILP64 symbols; library lifetime covers every copied function pointer.
        let library = unsafe { Library::new(directory.join(&manifest.library))? };
        // SAFETY: pinned OpenBLAS returns a static NUL-terminated config string with this ABI.
        let config = unsafe {
            let function = library.get::<unsafe extern "C" fn() -> *const c_char>(
                b"scipy_openblas_get_config64_\0",
            )?;
            CStr::from_ptr(function()).to_string_lossy().into_owned()
        };
        if !config.contains("OpenBLAS 0.3.31") || !config.contains("USE64BITINT") {
            return Err(Error::Contract(
                "numerical artifact OpenBLAS version/ABI mismatch",
            ));
        }
        // SAFETY: Fortran ABI and argument widths match the pinned ILP64 dgesdd declaration.
        let gesdd = unsafe { *library.get::<Dgesdd>(b"scipy_dgesdd_64_\0")? };
        Ok(Self {
            _library: library,
            gesdd,
        })
    }
    fn right_vectors(&mut self, points: &[[f64; 3]]) -> Result<Option<[f64; 9]>, Error> {
        if !(3..=12000).contains(&points.len()) {
            return Err(Error::Contract("SVD point count out of bounds"));
        }
        let m =
            i64::try_from(points.len()).map_err(|_| Error::Contract("SVD point count overflow"))?;
        let n = 3_i64;
        let mut a: Vec<f64> = (0..3)
            .flat_map(|column| points.iter().map(move |point| point[column]))
            .collect();
        let mut s = [0.; 3];
        let mut u = vec![0.; points.len() * 3];
        let mut vt = [0.; 9];
        let mut iwork = [0_i64; 24];
        let mut info = 0_i64;
        let mut query = [0.];
        let job = b'S' as c_char;
        // SAFETY: all arrays have dgesdd's required lengths: A/U m*3, S3, VT3*3,
        // IWORK8*min(m,n). Workspace query uses one writable element and lwork=-1.
        unsafe {
            (self.gesdd)(
                &job,
                &m,
                &n,
                a.as_mut_ptr(),
                &m,
                s.as_mut_ptr(),
                u.as_mut_ptr(),
                &m,
                vt.as_mut_ptr(),
                &n,
                query.as_mut_ptr(),
                &-1,
                iwork.as_mut_ptr(),
                &mut info,
            );
        }
        if info != 0 || !query[0].is_finite() || !(1. ..=1e8).contains(&query[0]) {
            return Err(Error::Contract("SVD workspace query failed"));
        }
        let lwork = query[0] as i64;
        let mut work = vec![0.; lwork as usize];
        // SAFETY: same bounded arrays as query; WORK length is the LAPACK-reported optimum.
        unsafe {
            (self.gesdd)(
                &job,
                &m,
                &n,
                a.as_mut_ptr(),
                &m,
                s.as_mut_ptr(),
                u.as_mut_ptr(),
                &m,
                vt.as_mut_ptr(),
                &n,
                work.as_mut_ptr(),
                &lwork,
                iwork.as_mut_ptr(),
                &mut info,
            );
        }
        Ok((info == 0).then_some(vt))
    }
}
impl Fit for Numerics {
    fn estimate(&mut self, points: &[[f64; 3]]) -> Result<[f64; 3], Error> {
        let Some(vt) = self.right_vectors(points)? else {
            return Ok([f64::NAN; 3]);
        };
        let slope = -vt[2] / vt[8];
        let offset = -vt[5] / vt[8];
        let squared = slope * slope;
        let sin = (squared / (squared + 1.)).sqrt();
        let cos = (1. / (squared + 1.)).sqrt();
        let spread: Vec<f64> = points.iter().map(|p| -p[0] * sin + p[2] * cos).collect();
        let mean = pairwise(&spread) / spread.len() as f64;
        let variance: Vec<f64> = spread
            .iter()
            .map(|value| (value - mean) * (value - mean))
            .collect();
        Ok([
            slope,
            offset,
            (pairwise(&variance) / variance.len() as f64).sqrt() * 1.5,
        ])
    }
}
// NumPy's pairwise summation order for contiguous doubles (PW_BLOCKSIZE=128).
fn pairwise(values: &[f64]) -> f64 {
    if values.len() < 8 {
        return values.iter().fold(-0., |a, b| a + b);
    }
    if values.len() <= 128 {
        let mut sums = [0.; 8];
        sums.copy_from_slice(&values[..8]);
        let mut i = 8;
        while i + 8 <= values.len() {
            for j in 0..8 {
                sums[j] += values[i + j];
            }
            i += 8;
        }
        let result = ((sums[0] + sums[1]) + (sums[2] + sums[3]))
            + ((sums[4] + sums[5]) + (sums[6] + sums[7]));
        return values[i..].iter().fold(result, |a, b| a + b);
    }
    let middle = (values.len() / 2) / 8 * 8;
    pairwise(&values[..middle]) + pairwise(&values[middle..])
}
