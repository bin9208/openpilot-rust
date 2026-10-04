use crate::{
    numerics::{Dgemm, Numerics},
    Error,
};

pub(crate) type Dgemv = unsafe extern "C" fn(
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
pub(crate) type Ddot = unsafe extern "C" fn(i64, *const f64, i64, *const f64, i64) -> f64;

impl Numerics {
    pub fn dot_points(&self, left: &[[f64; 3]], right: &[[f64; 3]]) -> Result<Vec<f64>, Error> {
        self.point_dot.compute(left, right)
    }
}

pub(crate) struct Dot {
    pub(crate) gemm: Dgemm,
    pub(crate) gemv: Dgemv,
    pub(crate) dot: Ddot,
}

impl Dot {
    pub(crate) fn compute(&self, left: &[[f64; 3]], right: &[[f64; 3]]) -> Result<Vec<f64>, Error> {
        let rows = i64::try_from(left.len()).map_err(|_| Error::IntegerOverflow)?;
        let columns = i64::try_from(right.len()).map_err(|_| Error::IntegerOverflow)?;
        let size = left
            .len()
            .checked_mul(right.len())
            .ok_or(Error::IntegerOverflow)?;
        let mut output = vec![0.; size];
        if size == 0 {
            return Ok(output);
        }
        let a = left.as_flattened();
        let b = right.as_flattened();
        // SAFETY: the pinned ILP64 ABI receives contiguous Mx3/Nx3 inputs and
        // a distinct MxN output. Checked dimensions match all owned slice lengths.
        unsafe {
            match (rows, columns) {
                (1, 1) => output[0] = (self.dot)(3, a.as_ptr(), 1, b.as_ptr(), 1),
                (1, _) => (self.gemv)(
                    101,
                    111,
                    columns,
                    3,
                    1.,
                    b.as_ptr(),
                    3,
                    a.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                ),
                (_, 1) => (self.gemv)(
                    101,
                    111,
                    rows,
                    3,
                    1.,
                    a.as_ptr(),
                    3,
                    b.as_ptr(),
                    1,
                    0.,
                    output.as_mut_ptr(),
                    1,
                ),
                _ => (self.gemm)(
                    101,
                    111,
                    112,
                    rows,
                    columns,
                    3,
                    1.,
                    a.as_ptr(),
                    3,
                    b.as_ptr(),
                    3,
                    0.,
                    output.as_mut_ptr(),
                    columns,
                ),
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
#[path = "numerics_dot_tests.rs"]
mod tests;
