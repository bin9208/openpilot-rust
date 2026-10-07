use crate::{
    model::{Model, ModelLead, Position},
    point::{Point, TrackId},
    Error,
};
use openpilot_cereal::{
    car_capnp::radar_data,
    log_capnp::{live_pose, model_data_v2},
};
fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    values.iter().map(f64::from).collect()
}

pub fn model(value: model_data_v2::Reader<'_>) -> Result<Model, Error> {
    let position = value.get_position()?;
    Ok(Model {
        position: Position {
            x: floats(position.get_x()?),
            y: floats(position.get_y()?),
        },
        velocity: floats(value.get_velocity()?.get_x()?),
        lane_probabilities: floats(value.get_lane_line_probs()?),
        leads: value
            .get_leads_v3()?
            .iter()
            .map(|lead| {
                Ok(ModelLead {
                    probability: f64::from(lead.get_prob()),
                    x: Some(floats(lead.get_x()?)),
                    y: Some(floats(lead.get_y()?)),
                    v: Some(floats(lead.get_v()?)),
                    a: Some(floats(lead.get_a()?)),
                    x_std: Some(floats(lead.get_x_std()?)),
                    y_std: Some(floats(lead.get_y_std()?)),
                    v_std: Some(floats(lead.get_v_std()?)),
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}

pub fn points(value: radar_data::Reader<'_>) -> Result<Vec<Point>, Error> {
    value
        .get_points()?
        .iter()
        .map(|point| {
            use radar_data::radar_point::RadarSource;
            let source = match point.get_radar_source()? {
                RadarSource::FrontRadar => "frontRadar",
                RadarSource::Scc => "scc",
                RadarSource::Corner235 => "corner235",
                RadarSource::Corner180 => "corner180",
                RadarSource::Corner430 => "corner430",
            };
            Ok(Point {
                track_id: TrackId(i128::from(point.get_track_id())),
                source: source.to_owned(),
                d_rel: f64::from(point.get_d_rel()),
                y_rel: f64::from(point.get_y_rel()),
                v_rel: f64::from(point.get_v_rel()),
                a_rel: f64::from(point.get_a_rel()),
                yv_rel: f64::from(point.get_yv_rel()),
                v_lead: f64::from(point.get_v_lead()),
                a_lead: f64::from(point.get_a_lead()),
                j_lead: f64::from(point.get_j_lead()),
                measured: point.get_measured(),
                radar_track_state: i32::from(point.get_track_state()),
                kinematics_source: None,
                kinematics_track_id: None,
            })
        })
        .collect()
}
pub fn yaw(value: live_pose::Reader<'_>) -> Result<f64, Error> {
    let angular = value.get_angular_velocity_device()?;
    let value_yaw = f64::from(angular.get_z());
    Ok(
        if angular.get_valid()
            && value.get_inputs_o_k()
            && value.get_sensors_o_k()
            && value_yaw.is_finite()
        {
            -value_yaw
        } else {
            0.
        },
    )
}
