//! Source PoseCalibrator and transformations.py matrix/quaternion operations.
use super::{field, normalized_array, numbers};
use crate::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};

type Matrix = [[f64; 3]; 3];
fn transpose(value: Matrix) -> Matrix {
    std::array::from_fn(|row| std::array::from_fn(|column| value[column][row]))
}
fn multiply(left: Matrix, right: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            (left[row][0] * right[0][column] + left[row][1] * right[1][column])
                + left[row][2] * right[2][column]
        })
    })
}
fn vector(matrix: Matrix, value: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        (matrix[row][0] * value[0] + matrix[row][1] * value[1]) + matrix[row][2] * value[2]
    })
}
fn triple(input: &JsonValue) -> Result<[f64; 3]> {
    let values = numbers(input)?;
    Ok(values
        .try_into()
        .map_err(|_| "pose vector must have length three")?)
}
fn rotation(euler: [f64; 3]) -> Matrix {
    let (phi, theta, psi) = (euler[0], euler[1], euler[2]);
    let (cx, sx, cy, sy, cz, sz) = (
        phi.cos(),
        phi.sin(),
        theta.cos(),
        theta.sin(),
        psi.cos(),
        psi.sin(),
    );
    let x = [[1., 0., 0.], [0., cx, -sx], [0., sx, cx]];
    let y = [[cy, 0., sy], [0., 1., 0.], [-sy, 0., cy]];
    let z = [[cz, -sz, 0.], [sz, cz, 0.], [0., 0., 1.]];
    multiply(multiply(z, y), x)
}
fn quaternion(rot: Matrix) -> [f64; 4] {
    let trace = (rot[0][0] + rot[1][1]) + rot[2][2];
    let mut value = if trace > 0. {
        let s = 0.5 / (trace + 1.).sqrt();
        [
            0.25 / s,
            (rot[2][1] - rot[1][2]) * s,
            (rot[0][2] - rot[2][0]) * s,
            (rot[1][0] - rot[0][1]) * s,
        ]
    } else if rot[0][0] > rot[1][1] && rot[0][0] > rot[2][2] {
        let s = 2. * (1. + rot[0][0] - rot[1][1] - rot[2][2]).sqrt();
        [
            (rot[2][1] - rot[1][2]) / s,
            0.25 * s,
            (rot[0][1] + rot[1][0]) / s,
            (rot[0][2] + rot[2][0]) / s,
        ]
    } else if rot[1][1] > rot[2][2] {
        let s = 2. * (1. + rot[1][1] - rot[0][0] - rot[2][2]).sqrt();
        [
            (rot[0][2] - rot[2][0]) / s,
            (rot[0][1] + rot[1][0]) / s,
            0.25 * s,
            (rot[1][2] + rot[2][1]) / s,
        ]
    } else {
        let s = 2. * (1. + rot[2][2] - rot[0][0] - rot[1][1]).sqrt();
        [
            (rot[1][0] - rot[0][1]) / s,
            (rot[0][2] + rot[2][0]) / s,
            (rot[1][2] + rot[2][1]) / s,
            0.25 * s,
        ]
    };
    if value[0] < 0. {
        for point in &mut value {
            *point = -*point;
        }
    }
    value
}
fn euler(rot: Matrix) -> [f64; 3] {
    let [w, x, y, z] = quaternion(rot);
    let gamma = (2. * (w * x + y * z)).atan2(1. - 2. * (x * x + y * y));
    let theta = (2. * (w * y - z * x)).clamp(-1., 1.).asin();
    let psi = (2. * (w * z + x * y)).atan2(1. - 2. * (y * y + z * z));
    [gamma, theta, psi]
}
fn rotated(matrix: Matrix, measurement: &JsonValue) -> Result<Value> {
    let xyz = triple(&field(measurement, "xyz")?)?;
    let std = triple(&field(measurement, "std")?)?;
    let diagonal = std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            if row == column {
                std[row] * std[row]
            } else {
                0.
            }
        })
    });
    let covariance = multiply(multiply(matrix, diagonal), transpose(matrix));
    let std: [f64; 3] = std::array::from_fn(|index| covariance[index][index].sqrt());
    Ok(json!({"xyz":normalized_array(&vector(matrix, xyz)),"std":normalized_array(&std)}))
}
pub(super) fn evaluate(input: &JsonValue) -> Result<Value> {
    let rpy = triple(&field(input, "rpy")?)?;
    let matrix = transpose(rotation(rpy));
    let pose = field(input, "pose")?;
    let _orientation_std = field(&field(&pose, "orientation")?, "std")?;
    let orientation = euler(multiply(
        rotation(triple(&field(&field(&pose, "orientation")?, "xyz")?)?),
        transpose(matrix),
    ));
    // The original constructs calStatus=int(row['valid']), then compares it with
    // the calibrated ordinal 1. Preserve integer conversion, not truthiness.
    let raw_valid = field(input, "valid")?;
    let calibrated = match raw_valid.view() {
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => {
            value.parse::<num_bigint::BigInt>()? == num_bigint::BigInt::from(1)
        }
        JsonView::Float(value) if value.is_finite() => value.trunc() == 1.,
        JsonView::Text(_) => {
            crate::stats_clock_check::integer(
                &raw_valid.to_utf8().ok_or("invalid calibration integer")?,
            )? == "1"
        }
        _ => return Err("invalid calibration integer".into()),
    };
    let points = vec![
        json!({"xyz":normalized_array(&orientation),"std":normalized_array(&[f64::NAN;3])}),
        rotated(matrix, &field(&pose, "velocity")?)?,
        rotated(matrix, &field(&pose, "acceleration")?)?,
        rotated(matrix, &field(&pose, "angular_velocity")?)?,
    ];
    Ok(
        json!({"valid":calibrated,"rotation":matrix.into_iter().map(|row| normalized_array(&row)).collect::<Vec<_>>(),"pose":points}),
    )
}
