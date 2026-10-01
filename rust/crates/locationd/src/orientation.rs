pub type Matrix = [[f64; 3]; 3];
pub const IDENTITY: Matrix = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];

fn product(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j])
    })
}
pub fn rotation([roll, pitch, yaw]: [f64; 3]) -> Matrix {
    let (cx, sx) = (roll.cos(), roll.sin());
    let (cy, sy) = (pitch.cos(), pitch.sin());
    let (cz, sz) = (yaw.cos(), yaw.sin());
    let rx = [[1., 0., 0.], [0., cx, -sx], [0., sx, cx]];
    let ry = [[cy, 0., sy], [0., 1., 0.], [-sy, 0., cy]];
    let rz = [[cz, -sz, 0.], [sz, cz, 0.], [0., 0., 1.]];
    product(product(rz, ry), rx)
}
pub fn apply(matrix: Matrix, vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}
pub fn rotate_std(matrix: Matrix, std: [f64; 3]) -> [f64; 3] {
    let covariance = [
        [std[0] * std[0], 0., 0.],
        [0., std[1] * std[1], 0.],
        [0., 0., std[2] * std[2]],
    ];
    let transpose = std::array::from_fn(|i| std::array::from_fn(|j| matrix[j][i]));
    let result = product(product(matrix, covariance), transpose);
    std::array::from_fn(|i| result[i][i].sqrt())
}
pub fn norm(values: [f64; 3]) -> f64 {
    (values[0] * values[0] + values[1] * values[1] + values[2] * values[2]).sqrt()
}
pub fn minimum(values: [f64; 3]) -> f64 {
    if values.iter().any(|v| v.is_nan()) {
        f64::NAN
    } else {
        values.into_iter().fold(f64::INFINITY, f64::min)
    }
}
pub fn maximum(values: [f64; 3]) -> f64 {
    if values.iter().any(|v| v.is_nan()) {
        f64::NAN
    } else {
        values.into_iter().fold(f64::NEG_INFINITY, f64::max)
    }
}
