//! Native development-time regeneration of the project-owned PoseKalman and
//! CarKalman equations. Model definitions are independent Rust expressions;
//! checked-in generated runtime source is never read by this generator.

mod expr;

use crate::argparse_scan::{scan_os, ArgumentError, OptionSpec};
use expr::{Expressions, Id};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Model {
    Pose,
    Car,
}

const POSE_NAMES: [&str; 10] = [
    "transition",
    "transition_jacobian",
    "observe_4",
    "jacobian_4",
    "observe_10",
    "jacobian_10",
    "observe_13",
    "jacobian_13",
    "observe_14",
    "jacobian_14",
];
const CAR_NAMES: [&str; 18] = [
    "transition",
    "transition_jacobian",
    "observe_25",
    "jacobian_25",
    "observe_24",
    "jacobian_24",
    "observe_30",
    "jacobian_30",
    "observe_26",
    "jacobian_26",
    "observe_27",
    "jacobian_27",
    "observe_29",
    "jacobian_29",
    "observe_28",
    "jacobian_28",
    "observe_31",
    "jacobian_31",
];

pub fn function_names(model: Model) -> &'static [&'static str] {
    match model {
        Model::Pose => &POSE_NAMES,
        Model::Car => &CAR_NAMES,
    }
}

/// The original development tools accept one optional output path; a repeated
/// option replaces the earlier value. Keep the path as OS bytes.
pub fn output_argument(
    arguments: impl IntoIterator<Item = OsString>,
    default: &str,
) -> Result<Option<PathBuf>, ArgumentError> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let parsed = scan_os(&arguments, &[OptionSpec::value("--output", false)])?;
    if parsed.help {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(
        parsed
            .values
            .get("--output")
            .cloned()
            .unwrap_or_else(|| OsString::from(default)),
    )))
}

struct Function {
    name: &'static str,
    values: Vec<Id>,
    timed: bool,
}

struct Equations {
    expressions: Expressions,
    functions: Vec<Function>,
    width: usize,
}

fn jacobian(expressions: &mut Expressions, values: &[Id], width: usize) -> Vec<Id> {
    let mut entries = Vec::with_capacity(values.len() * width);
    for &value in values {
        for state in 0..width {
            entries.push(expressions.derivative(value, state));
        }
    }
    entries
}

fn append_observation(
    expressions: &mut Expressions,
    functions: &mut Vec<Function>,
    kind: usize,
    values: Vec<Id>,
    width: usize,
) {
    let derivative = jacobian(expressions, &values, width);
    let name = match kind {
        4 => "observe_4",
        10 => "observe_10",
        13 => "observe_13",
        14 => "observe_14",
        24 => "observe_24",
        25 => "observe_25",
        26 => "observe_26",
        27 => "observe_27",
        28 => "observe_28",
        29 => "observe_29",
        30 => "observe_30",
        31 => "observe_31",
        _ => unreachable!("fixed model observation kind"),
    };
    let jacobian_name = match kind {
        4 => "jacobian_4",
        10 => "jacobian_10",
        13 => "jacobian_13",
        14 => "jacobian_14",
        24 => "jacobian_24",
        25 => "jacobian_25",
        26 => "jacobian_26",
        27 => "jacobian_27",
        28 => "jacobian_28",
        29 => "jacobian_29",
        30 => "jacobian_30",
        31 => "jacobian_31",
        _ => unreachable!("fixed model observation kind"),
    };
    functions.push(Function {
        name,
        values,
        timed: false,
    });
    functions.push(Function {
        name: jacobian_name,
        values: derivative,
        timed: false,
    });
}

type Matrix3 = [[Id; 3]; 3];

fn euler_rotation(expressions: &mut Expressions, roll: Id, pitch: Id, yaw: Id) -> Matrix3 {
    let zero = expressions.constant(0.0);
    let one = expressions.constant(1.0);
    let cr = expressions.cos(roll);
    let sr = expressions.sin(roll);
    let cp = expressions.cos(pitch);
    let sp = expressions.sin(pitch);
    let cy = expressions.cos(yaw);
    let sy = expressions.sin(yaw);
    let minus_sr = expressions.neg(sr);
    let minus_sp = expressions.neg(sp);
    let minus_sy = expressions.neg(sy);
    let roll_matrix = [[one, zero, zero], [zero, cr, minus_sr], [zero, sr, cr]];
    let pitch_matrix = [[cp, zero, sp], [zero, one, zero], [minus_sp, zero, cp]];
    let yaw_matrix = [[cy, minus_sy, zero], [sy, cy, zero], [zero, zero, one]];
    let pitch_roll = multiply_matrix(expressions, pitch_matrix, roll_matrix);
    multiply_matrix(expressions, yaw_matrix, pitch_roll)
}

fn multiply_matrix(expressions: &mut Expressions, a: Matrix3, b: Matrix3) -> Matrix3 {
    let zero = expressions.constant(0.0);
    let mut output = [[zero; 3]; 3];
    for (row, output_row) in output.iter_mut().enumerate() {
        for (col, slot) in output_row.iter_mut().enumerate() {
            let terms = (0..3)
                .map(|axis| expressions.mul([a[row][axis], b[axis][col]]))
                .collect::<Vec<_>>();
            *slot = expressions.add(terms);
        }
    }
    output
}

fn matrix_vector(expressions: &mut Expressions, matrix: Matrix3, vector: [Id; 3]) -> [Id; 3] {
    let zero = expressions.constant(0.0);
    let mut output = [zero; 3];
    for (row, slot) in output.iter_mut().enumerate() {
        let terms = (0..3)
            .map(|col| expressions.mul([matrix[row][col], vector[col]]))
            .collect::<Vec<_>>();
        *slot = expressions.add(terms);
    }
    output
}

fn cross(expressions: &mut Expressions, a: [Id; 3], b: [Id; 3]) -> [Id; 3] {
    let a1b2 = expressions.mul([a[1], b[2]]);
    let a2b1 = expressions.mul([a[2], b[1]]);
    let a2b0 = expressions.mul([a[2], b[0]]);
    let a0b2 = expressions.mul([a[0], b[2]]);
    let a0b1 = expressions.mul([a[0], b[1]]);
    let a1b0 = expressions.mul([a[1], b[0]]);
    [
        expressions.sub(a1b2, a2b1),
        expressions.sub(a2b0, a0b2),
        expressions.sub(a0b1, a1b0),
    ]
}

fn pose_equations() -> Equations {
    let mut e = Expressions::default();
    let state = (0..18).map(|index| e.state(index)).collect::<Vec<_>>();
    let dt = e.dt();
    let base = euler_rotation(&mut e, state[0], state[1], state[2]);
    let increments = [6, 7, 8].map(|index| e.mul([dt, state[index]]));
    let rotation = euler_rotation(&mut e, increments[0], increments[1], increments[2]);
    let next = multiply_matrix(&mut e, base, rotation);
    let mut transition = state.clone();
    transition[0] = e.atan2(next[2][1], next[2][2]);
    let minus_20 = e.neg(next[2][0]);
    transition[1] = e.asin(minus_20);
    transition[2] = e.atan2(next[1][0], next[0][0]);
    for (index, slot) in transition.iter_mut().enumerate().take(6).skip(3) {
        let delta = e.mul([dt, state[index + 9]]);
        *slot = e.add([state[index], delta]);
    }
    let transition_derivative = jacobian(&mut e, &transition, 18);
    let mut functions = vec![
        Function {
            name: "transition",
            values: transition,
            timed: true,
        },
        Function {
            name: "transition_jacobian",
            values: transition_derivative,
            timed: true,
        },
    ];

    let gyro = (0..3)
        .map(|axis| e.add([state[axis + 6], state[axis + 9]]))
        .collect();
    append_observation(&mut e, &mut functions, 4, gyro, 18);

    let zero = e.constant(0.0);
    let gravity = e.constant(-9.81);
    let device_from_ned = std::array::from_fn(|row| std::array::from_fn(|col| base[col][row]));
    let rotated_gravity = matrix_vector(&mut e, device_from_ned, [zero, zero, gravity]);
    let angular = [state[6], state[7], state[8]];
    let velocity = [state[3], state[4], state[5]];
    let centripetal = cross(&mut e, angular, velocity);
    let acceleration = (0..3)
        .map(|axis| {
            e.add([
                rotated_gravity[axis],
                state[axis + 12],
                centripetal[axis],
                state[axis + 15],
            ])
        })
        .collect();
    append_observation(&mut e, &mut functions, 10, acceleration, 18);
    append_observation(&mut e, &mut functions, 13, velocity.to_vec(), 18);
    append_observation(&mut e, &mut functions, 14, angular.to_vec(), 18);
    Equations {
        expressions: e,
        functions,
        width: 18,
    }
}

fn car_equations() -> Equations {
    let mut e = Expressions::default();
    let state = (0..9).map(|index| e.state(index)).collect::<Vec<_>>();
    let globals = (0..6).map(|index| e.global(index)).collect::<Vec<_>>();
    let dt = e.dt();
    let (mass, inertia, front, rear, stiffness_front, stiffness_rear) = (
        globals[0], globals[1], globals[2], globals[3], globals[4], globals[5],
    );
    let (
        stiffness,
        steer_ratio,
        angle_offset,
        angle_offset_fast,
        u,
        v,
        yaw_rate,
        steer_angle,
        roll,
    ) = (
        state[0], state[1], state[2], state[3], state[4], state[5], state[6], state[7], state[8],
    );
    let cf = e.mul([stiffness, stiffness_front]);
    let cr = e.mul([stiffness, stiffness_rear]);
    let cf_plus_cr = e.add([cf, cr]);
    let minus_cf_plus_cr = e.neg(cf_plus_cr);
    let mass_u = e.mul([mass, u]);
    let inertia_u = e.mul([inertia, u]);
    let front_cf = e.mul([cf, front]);
    let rear_cr = e.mul([cr, rear]);
    let torque = e.sub(front_cf, rear_cr);
    let minus_torque = e.neg(torque);
    let a00 = e.div(minus_cf_plus_cr, mass_u);
    let a01_part = e.div(minus_torque, mass_u);
    let a01 = e.sub(a01_part, u);
    let a10 = e.div(minus_torque, inertia_u);
    let front2 = e.powi(front, 2);
    let rear2 = e.powi(rear, 2);
    let front_torque = e.mul([cf, front2]);
    let rear_torque = e.mul([cr, rear2]);
    let combined_torque = e.add([front_torque, rear_torque]);
    let minus_combined = e.neg(combined_torque);
    let a11 = e.div(minus_combined, inertia_u);
    let cf_mass = e.div(cf, mass);
    let b0 = e.div(cf_mass, steer_ratio);
    let cf_front = e.mul([cf, front]);
    let cf_front_inertia = e.div(cf_front, inertia);
    let b1 = e.div(cf_front_inertia, steer_ratio);
    let steer_error = e.sub(steer_angle, angle_offset);
    let steer_error = e.sub(steer_error, angle_offset_fast);
    let lateral_0 = e.mul([a00, v]);
    let lateral_1 = e.mul([a01, yaw_rate]);
    let lateral_2 = e.mul([b0, steer_error]);
    let gravity = e.constant(9.81);
    let gravity_roll = e.mul([gravity, roll]);
    let lateral_acceleration = e.add([lateral_0, lateral_1, lateral_2]);
    let lateral_acceleration = e.sub(lateral_acceleration, gravity_roll);
    let yaw_0 = e.mul([a10, v]);
    let yaw_1 = e.mul([a11, yaw_rate]);
    let yaw_2 = e.mul([b1, steer_error]);
    let yaw_acceleration = e.add([yaw_0, yaw_1, yaw_2]);
    let mut transition = state.clone();
    let lateral_step = e.mul([dt, lateral_acceleration]);
    transition[5] = e.add([v, lateral_step]);
    let yaw_step = e.mul([dt, yaw_acceleration]);
    transition[6] = e.add([yaw_rate, yaw_step]);
    let transition_derivative = jacobian(&mut e, &transition, 9);
    let mut functions = vec![
        Function {
            name: "transition",
            values: transition,
            timed: true,
        },
        Function {
            name: "transition_jacobian",
            values: transition_derivative,
            timed: true,
        },
    ];
    for (kind, values) in [
        (25, vec![yaw_rate]),
        (24, vec![u, v]),
        (30, vec![u]),
        (26, vec![steer_angle]),
        (27, vec![angle_offset_fast]),
        (29, vec![steer_ratio]),
        (28, vec![stiffness]),
        (31, vec![roll]),
    ] {
        append_observation(&mut e, &mut functions, kind, values, 9);
    }
    Equations {
        expressions: e,
        functions,
        width: 9,
    }
}

fn equations(model: Model) -> Equations {
    match model {
        Model::Pose => pose_equations(),
        Model::Car => car_equations(),
    }
}

/// Evaluates the independently defined symbolic functions against a held
/// original-source numerical corpus. The generated Rust is compiled in CI.
pub fn evaluate_all(
    model: Model,
    state: &[f64],
    globals: &[f64],
    dt: f64,
) -> Result<BTreeMap<&'static str, Vec<f64>>, String> {
    let definition = equations(model);
    if state.len() != definition.width || globals.len() != if model == Model::Car { 6 } else { 0 } {
        return Err("model input width differs from original ABI".to_owned());
    }
    let mut output = BTreeMap::new();
    for function in &definition.functions {
        output.insert(
            function.name,
            definition
                .expressions
                .evaluate_many(&function.values, state, globals, dt),
        );
    }
    Ok(output)
}

fn literal(value: f64) -> String {
    format!("{value:?}")
}

fn radians(degrees: f64) -> f64 {
    // CPython math.radians multiplies by its precomputed pi/180 factor.
    degrees * (std::f64::consts::PI / 180.0)
}

fn array_constant(name: &str, values: &[f64]) -> String {
    let length = values.len();
    let entries = values
        .iter()
        .map(|value| literal(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("pub const {name}: [f64; {length}] = [{entries}];\n")
}

fn model_constants(model: Model) -> String {
    let mut output = String::new();
    match model {
        Model::Pose => {
            output.push_str(&array_constant("INITIAL_X", &[0.0; 18]));
            let mut covariance = [1.0; 18];
            covariance[..3].fill(0.01_f64.powi(2));
            covariance[3..6].fill(10.0_f64.powi(2));
            covariance[12..15].fill(100.0_f64.powi(2));
            covariance[15..18].fill(0.01_f64.powi(2));
            output.push_str(&array_constant("INITIAL_P", &covariance));
            let process = [
                0.001_f64.powi(2),
                0.01_f64.powi(2),
                0.085_f64.powi(2),
                (0.005_f64 / 100.0).powi(2),
                3.0_f64.powi(2),
                0.005_f64.powi(2),
            ];
            let noise: [f64; 18] = std::array::from_fn(|index| process[index / 3]);
            output.push_str(&array_constant("PROCESS_NOISE", &noise));
            for (kind, sigma) in [(4, 0.025_f64), (10, 0.75), (13, 0.5), (14, 0.05)] {
                output.push_str(&array_constant(
                    &format!("NOISE_{kind}"),
                    &[sigma.powi(2); 3],
                ));
            }
        }
        Model::Car => {
            output.push_str(&array_constant(
                "INITIAL_X",
                &[1.0, 15.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 0.0],
            ));
            let noise = [
                (0.05_f64 / 100.0).powi(2),
                0.01_f64.powi(2),
                radians(0.02).powi(2),
                radians(0.25).powi(2),
                0.1_f64.powi(2),
                0.01_f64.powi(2),
                radians(0.1).powi(2),
                radians(0.1).powi(2),
                radians(1.0).powi(2),
            ];
            output.push_str(&array_constant("INITIAL_P", &noise));
            output.push_str(&array_constant("PROCESS_NOISE", &noise));
            for (kind, variance) in [
                (26, radians(0.05).powi(2)),
                (27, radians(10.0).powi(2)),
                (31, radians(1.0).powi(2)),
                (29, 5.0_f64.powi(2)),
                (28, 0.5_f64.powi(2)),
                (30, 0.1_f64.powi(2)),
            ] {
                output.push_str(&format!(
                    "pub const NOISE_{kind}: f64 = {};\n",
                    literal(variance)
                ));
            }
        }
    }
    output
}

/// Writes the original function names, signatures, row-major matrix entries,
/// and model constants without invoking Python or reading generated modules.
pub fn generate(model: Model, output: &Path) -> io::Result<()> {
    let definition = equations(model);
    fs::create_dir_all(output)?;
    let mut modules = String::new();
    for function in &definition.functions {
        let name = function.name;
        modules.push_str(&format!("mod {name};\npub use {name}::{name};\n"));
        let signature = match (model, function.timed) {
            (Model::Pose, true) => ", dt: f64",
            (Model::Car, true) => ", globals: &[f64; 6], dt: f64",
            _ => "",
        };
        let mut text = format!(
            "// Generated by the native symbolic Kalman model generator.\n\
             pub fn {name}(state: &[f64; {}]{signature}) -> [f64; {}] {{\n\
                 let _ = state;\n\
                 let mut out = [0.0; {}];\n",
            definition.width,
            function.values.len(),
            function.values.len()
        );
        for (index, &value) in function.values.iter().enumerate() {
            if !definition.expressions.is_zero(value) {
                text.push_str(&format!(
                    "    out[{index}] = {};\n",
                    definition.expressions.render(value)
                ));
            }
        }
        text.push_str("    out\n}\n");
        fs::write(output.join(format!("{name}.rs")), text)?;
    }
    modules.push_str(&model_constants(model));
    fs::write(output.join("mod.rs"), modules)
}
