use super::quadratic_weights;
use crate::Error;
use std::{ffi::c_char, slice};

unsafe extern "C" fn gesdd(
    job: *const c_char,
    m: *const i64,
    n: *const i64,
    a: *mut f64,
    lda: *const i64,
    s: *mut f64,
    u: *mut f64,
    ldu: *const i64,
    vt: *mut f64,
    ldvt: *const i64,
    work: *mut f64,
    lwork: *const i64,
    iwork: *mut i64,
    info: *mut i64,
) {
    // SAFETY: the caller supplies initialized scalar dimensions and disjoint
    // A/U3*m, S3, VT9, IWORK24 and query1/work17 allocations for synchronous use.
    unsafe {
        let count = usize::try_from(*m).unwrap();
        assert_eq!(
            (*job, *n, *lda, *ldu, *ldvt),
            (c_char::try_from(b'S').unwrap(), 3, *m, *m, 3)
        );
        for (index, value) in slice::from_raw_parts(a, count * 3).iter().enumerate() {
            let time = (index % count) as f64 - (count - 1) as f64;
            let expected = match index / count {
                0 => 1.,
                1 => time,
                2 => time * time,
                _ => unreachable!(),
            };
            assert_eq!(*value, expected);
        }
        *info = 0;
        if *lwork == -1 {
            *work = 17.;
            return;
        }
        assert_eq!(*lwork, 17);
        slice::from_raw_parts_mut(work, 17).fill(0.);
        slice::from_raw_parts_mut(iwork, 24).fill(0);
        slice::from_raw_parts_mut(s, 3).copy_from_slice(&[1., 2., 4.]);
        for (index, value) in slice::from_raw_parts_mut(u, count * 3)
            .iter_mut()
            .enumerate()
        {
            *value = (index % count + index / count + 1) as f64;
        }
        slice::from_raw_parts_mut(vt, 9).copy_from_slice(&[1., 0., 0., 0., 1., 0., 0., 0., 1.]);
    }
}

unsafe extern "C" fn gemm(
    order: i32,
    trans_a: i32,
    trans_b: i32,
    rows: i64,
    columns: i64,
    inner: i64,
    alpha: f64,
    a: *const f64,
    lda: i64,
    b: *const f64,
    ldb: i64,
    beta: f64,
    output: *mut f64,
    ldc: i64,
) {
    assert_eq!(
        (order, trans_a, trans_b, rows, inner, lda, ldb, ldc),
        (102, 111, 111, 3, 3, 3, 3, 3)
    );
    assert_eq!((alpha, beta), (1., 0.));
    let columns = usize::try_from(columns).unwrap();
    // SAFETY: the caller provides initialized column-major A3x3 and B3xm,
    // plus a disjoint initialized output3xm; no pointer escapes the mock call.
    unsafe {
        let a = slice::from_raw_parts(a, 9);
        let b = slice::from_raw_parts(b, 3 * columns);
        let output = slice::from_raw_parts_mut(output, 3 * columns);
        for (index, value) in output.iter_mut().enumerate() {
            let row = index % 3;
            let column = index / 3;
            *value = (0..3).map(|k| a[row + 3 * k] * b[k + 3 * column]).sum();
        }
    }
}

unsafe extern "C" fn failure<const CASE: u8>(
    _job: *const c_char,
    _m: *const i64,
    _n: *const i64,
    _a: *mut f64,
    _lda: *const i64,
    _s: *mut f64,
    _u: *mut f64,
    _ldu: *const i64,
    _vt: *mut f64,
    _ldvt: *const i64,
    work: *mut f64,
    lwork: *const i64,
    _iwork: *mut i64,
    info: *mut i64,
) {
    // SAFETY: query uses one initialized workspace element; info/lwork point
    // to the caller's initialized, aligned scalars for the entire synchronous call.
    unsafe {
        if *lwork == -1 {
            *work = if CASE == 3 { f64::NAN } else { 17. };
            *info = if CASE == 0 { -1 } else { 0 };
        } else {
            *info = if CASE == 1 { 1 } else { -1 };
        }
    }
}

#[test]
fn svd_workspace_buffers_and_output_ownership_cover_runtime_sample_counts() {
    for count in 7..=52 {
        let expected: Vec<_> = (0..count).map(|column| 0.5 * (column + 3) as f64).collect();
        assert_eq!(quadratic_weights(count, gesdd, gemm).unwrap(), expected);
    }
}

#[test]
fn svd_query_and_computation_failures_stop_before_result_consumption() {
    assert!(matches!(
        quadratic_weights(7, failure::<0>, gemm),
        Err(Error::Contract("SVD workspace query failed"))
    ));
    assert!(matches!(
        quadratic_weights(7, failure::<3>, gemm),
        Err(Error::Contract("SVD workspace query failed"))
    ));
    assert!(matches!(
        quadratic_weights(7, failure::<1>, gemm),
        Err(Error::SvdDidNotConverge)
    ));
    assert!(matches!(
        quadratic_weights(7, failure::<2>, gemm),
        Err(Error::Contract("SVD argument rejected"))
    ));
}
