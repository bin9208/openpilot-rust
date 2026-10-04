use crate::{
    numerics::{Dgemm, Dgesdd},
    Error,
};
use std::ffi::c_char;

pub(crate) fn quadratic_weights(
    count: usize,
    gesdd: Dgesdd,
    gemm: Dgemm,
) -> Result<Vec<f64>, Error> {
    if count < 7 {
        return Err(Error::Contract("quadratic jerk requires seven samples"));
    }
    let m = i64::try_from(count).map_err(|_| Error::IntegerOverflow)?;
    let n = 3_i64;
    let mut a: Vec<f64> = (0..3)
        .flat_map(|column| {
            (0..count).map(move |row| {
                let time = row as f64 - (count - 1) as f64;
                match column {
                    0 => 1.,
                    1 => time,
                    _ => time * time,
                }
            })
        })
        .collect();
    let mut s = [0.; 3];
    let mut u = vec![0.; count * 3];
    let mut vt = [0.; 9];
    let mut iwork = [0_i64; 24];
    let mut info = 0_i64;
    let mut query = [0.];
    let job = b'S' as c_char;
    // SAFETY: A/U have m*3 entries, S3, VT9, IWORK8*3. A one-element
    // workspace is sufficient for the dgesdd lwork=-1 query.
    unsafe {
        (gesdd)(
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
    if info != 0 || !query[0].is_finite() || query[0] < 1. {
        return Err(Error::Contract("SVD workspace query failed"));
    }
    let lwork = query[0] as i64;
    let mut work = vec![0.; lwork as usize];
    // SAFETY: same bounded matrix arrays as the query; WORK has the reported
    // optimal size, and all writable buffers are distinct owned allocations.
    unsafe {
        (gesdd)(
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
    if info > 0 {
        return Err(Error::SvdDidNotConverge);
    }
    if info < 0 {
        return Err(Error::Contract("SVD argument rejected"));
    }
    let cutoff = 1e-15 * s[0];
    for value in &mut s {
        *value = if *value > cutoff { 1. / *value } else { 0. };
    }
    let mut v = [0.; 9];
    for column in 0..3 {
        for row in 0..3 {
            v[row + 3 * column] = vt[column + 3 * row];
        }
    }
    let b: Vec<f64> = (0..count)
        .flat_map(|column| {
            let u = &u;
            let s = &s;
            (0..3).map(move |row| s[row] * u[column + count * row])
        })
        .collect();
    let mut output = vec![0.; count * 3];
    // SAFETY: column-major V3x3 and B3xm have checked contiguous lengths;
    // output3xm is disjoint. CBLAS enums and dimensions use the pinned ABI.
    unsafe {
        (gemm)(
            102,
            111,
            111,
            3,
            m,
            3,
            1.,
            v.as_ptr(),
            3,
            b.as_ptr(),
            3,
            0.,
            output.as_mut_ptr(),
            3,
        );
    }
    Ok((0..count)
        .map(|column| 2. * output[2 + 3 * column])
        .collect())
}

#[cfg(test)]
#[path = "numerics_weights_tests.rs"]
mod tests;
