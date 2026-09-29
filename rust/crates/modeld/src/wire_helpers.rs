use crate::action::Action;
use openpilot_cereal::log_capnp::{model_data_v2, x_y_z_t_data as xyzt_data};

pub(crate) fn float(value: f64) -> f32 {
    // Match cereal's IEEE Float32 narrowing after the original f64 calculation.
    value as f32
}

pub(crate) fn action(mut target: model_data_v2::action::Builder<'_>, value: Action) {
    target.set_desired_curvature(float(value.desired_curvature));
    target.set_desired_acceleration(float(value.desired_acceleration));
    target.set_desired_velocity(float(value.desired_velocity));
    target.set_should_stop(value.should_stop);
}

pub(crate) fn xyzt(
    mut target: xyzt_data::Builder<'_>,
    time: &[f32],
    points: &[[f32; 3]; 33],
) -> capnp::Result<()> {
    target.set_t(time)?;
    target.set_x(&points.map(|point| point[0])[..])?;
    target.set_y(&points.map(|point| point[1])[..])?;
    target.set_z(&points.map(|point| point[2])[..])
}

pub(crate) fn xyz_std(
    mut target: xyzt_data::Builder<'_>,
    points: &[[f32; 3]; 33],
) -> capnp::Result<()> {
    target.set_x_std(&points.map(|point| point[0])[..])?;
    target.set_y_std(&points.map(|point| point[1])[..])?;
    target.set_z_std(&points.map(|point| point[2])[..])
}
