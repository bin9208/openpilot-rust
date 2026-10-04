use super::Dot;

unsafe extern "C" fn dot(
    count: i64,
    a: *const f64,
    stride_a: i64,
    b: *const f64,
    stride_b: i64,
) -> f64 {
    let mut result = 0.;
    for index in 0..count {
        // SAFETY: the tested caller provides two three-element input slices and
        // positive unit strides; Miri verifies every fixture pointer access.
        unsafe {
            result +=
                *a.offset((index * stride_a) as isize) * *b.offset((index * stride_b) as isize);
        }
    }
    result
}

unsafe extern "C" fn gemv(
    order: i32,
    transpose: i32,
    rows: i64,
    columns: i64,
    alpha: f64,
    a: *const f64,
    leading: i64,
    b: *const f64,
    stride_b: i64,
    beta: f64,
    output: *mut f64,
    stride_output: i64,
) {
    assert_eq!(
        (order, transpose, columns, leading, stride_b, stride_output),
        (101, 111, 3, 3, 1, 1)
    );
    assert_eq!((alpha, beta), (1., 0.));
    for row in 0..rows {
        // SAFETY: the tested caller supplies rows*3 A, three B, and rows disjoint
        // output elements. Fixture writes and reads are checked by Miri.
        unsafe {
            *output.offset(row as isize) = dot(3, a.offset((row * leading) as isize), 1, b, 1);
        }
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
    leading_a: i64,
    b: *const f64,
    leading_b: i64,
    beta: f64,
    output: *mut f64,
    leading_output: i64,
) {
    assert_eq!(
        (
            order,
            trans_a,
            trans_b,
            inner,
            leading_a,
            leading_b,
            leading_output
        ),
        (101, 111, 112, 3, 3, 3, columns)
    );
    assert_eq!((alpha, beta), (1., 0.));
    for row in 0..rows {
        for column in 0..columns {
            // SAFETY: the tested caller provides rows*3 A, columns*3 B, and
            // rows*columns disjoint output elements with checked dimensions.
            unsafe {
                *output.offset((row * columns + column) as isize) = dot(
                    3,
                    a.offset((row * leading_a) as isize),
                    1,
                    b.offset((column * leading_b) as isize),
                    1,
                );
            }
        }
    }
}

#[test]
fn ffi_dimensions_strides_and_output_ownership_cover_all_dispatch_shapes() {
    let kernel = Dot { dot, gemv, gemm };
    for rows in [0, 1, 2, 8] {
        for columns in [0, 1, 2, 8] {
            let a = (0..rows)
                .map(|row| [row as f64, 2., 3.])
                .collect::<Vec<_>>();
            let b = (0..columns)
                .map(|column| [column as f64, 5., 7.])
                .collect::<Vec<_>>();
            let expected = a
                .iter()
                .flat_map(|left| {
                    b.iter().map(move |right| {
                        (left[0] * right[0] + left[1] * right[1]) + left[2] * right[2]
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(kernel.compute(&a, &b).unwrap(), expected);
        }
    }
}
