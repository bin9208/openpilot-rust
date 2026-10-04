use crate::{
    model::{Model, ModelAction, ModelLead, ModelMeta, Trajectory},
    Error,
};
use openpilot_cereal::log_capnp::{model_data_v2, x_y_z_t_data};

fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<f64> {
    values.iter().map(f64::from).collect()
}

pub fn trajectory(value: x_y_z_t_data::Reader<'_>) -> Result<Trajectory, Error> {
    Ok(Trajectory {
        t: floats(value.get_t()?),
        x: floats(value.get_x()?),
        y: floats(value.get_y()?),
        z: floats(value.get_z()?),
    })
}

pub fn model(value: model_data_v2::Reader<'_>) -> Result<Model, Error> {
    let meta = value.get_meta()?;
    let action = value.get_action()?;
    let lanes = value
        .get_lane_lines()?
        .iter()
        .map(trajectory)
        .collect::<Result<Vec<_>, _>>()?;
    let edges = value
        .get_road_edges()?
        .iter()
        .map(trajectory)
        .collect::<Result<Vec<_>, _>>()?;
    let leads = value
        .get_leads_v3()?
        .iter()
        .map(|lead| {
            Ok(ModelLead {
                prob: f64::from(lead.get_prob()),
                x: floats(lead.get_x()?),
                v: floats(lead.get_v()?),
                x_std: floats(lead.get_x_std()?),
                y_std: floats(lead.get_y_std()?),
                v_std: floats(lead.get_v_std()?),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Model {
        frame_id: value.get_frame_id(),
        position: trajectory(value.get_position()?)?,
        velocity: trajectory(value.get_velocity()?)?,
        acceleration: trajectory(value.get_acceleration()?)?,
        orientation: trajectory(value.get_orientation()?)?,
        orientation_rate: trajectory(value.get_orientation_rate()?)?,
        lane_lines: lanes,
        road_edges: edges,
        lane_line_probs: floats(value.get_lane_line_probs()?),
        lane_line_stds: floats(value.get_lane_line_stds()?),
        road_edge_stds: floats(value.get_road_edge_stds()?),
        leads_v3: leads,
        meta: ModelMeta {
            desire: meta.get_desire()?,
            lane_change_state: meta.get_lane_change_state()?,
            lane_change_direction: meta.get_lane_change_direction()?,
            desire_state: floats(meta.get_desire_state()?),
            desire_prediction: floats(meta.get_desire_prediction()?),
            lane_width_left: f64::from(meta.get_lane_width_left()),
            lane_width_right: f64::from(meta.get_lane_width_right()),
            gas_press_probs: floats(meta.get_disengage_predictions()?.get_gas_press_probs()?),
        },
        action: ModelAction {
            desired_acceleration: f64::from(action.get_desired_acceleration()),
            desired_velocity: f64::from(action.get_desired_velocity()),
            should_stop: action.get_should_stop(),
        },
    })
}
