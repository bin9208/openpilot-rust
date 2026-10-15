//! Immutable message values, with validity and publication stamps retained.
use super::{read, Error};
use crate::decoder::hyundai::bosch::{
    birth_b5::{Line, Model, Number},
    context::{Angular, Message, Pose},
    model_snapshot::{Input, Lead},
};
use openpilot_cereal::log_capnp::event;
use std::sync::Arc;
fn floats(values: capnp::primitive_list::Reader<'_, f32>) -> Vec<Number> {
    values.iter().map(|v| Number(f64::from(v))).collect()
}
pub fn pose(bytes: &[u8]) -> Result<Message<Pose>, Error> {
    let message = read(bytes)?;
    let event = message.get_root::<event::Reader>()?;
    let event::Which::LivePose(value) = event.which()? else {
        return Err(Error::Event("livePose"));
    };
    let timestamp = i64::try_from(event.get_log_mono_time())?;
    let value = if event.get_valid() {
        let pose = value?;
        let angular = pose.get_angular_velocity_device()?;
        Some(Arc::new(Pose {
            inputs_ok: pose.get_inputs_o_k(),
            sensors_ok: pose.get_sensors_o_k(),
            angular_velocity_device: Some(Angular {
                valid: angular.get_valid(),
                z: Number(f64::from(angular.get_z())),
            }),
        }))
    } else {
        None
    };
    Ok(Message { value, timestamp })
}
pub fn model(bytes: &[u8]) -> Result<Message<Input>, Error> {
    let message = read(bytes)?;
    let event = message.get_root::<event::Reader>()?;
    let event::Which::ModelV2(value) = event.which()? else {
        return Err(Error::Event("modelV2"));
    };
    let timestamp = i64::try_from(event.get_log_mono_time())?;
    if !event.get_valid() {
        return Ok(Message {
            value: None,
            timestamp,
        });
    }
    let model = value?;
    let position = model.get_position()?;
    let geometry = Model {
        source_ns: i64::try_from(model.get_timestamp_eof())?,
        edges: model
            .get_road_edges()?
            .iter()
            .map(|line| {
                Ok(Line {
                    x: floats(line.get_x()?),
                    y: floats(line.get_y()?),
                })
            })
            .collect::<Result<_, Error>>()?,
        lanes: model
            .get_lane_lines()?
            .iter()
            .map(|line| {
                Ok(Line {
                    x: floats(line.get_x()?),
                    y: floats(line.get_y()?),
                })
            })
            .collect::<Result<_, Error>>()?,
        position: Some(Line {
            x: floats(position.get_x()?),
            y: floats(position.get_y()?),
        }),
        edge_stds: floats(model.get_road_edge_stds()?),
        lane_probs: floats(model.get_lane_line_probs()?),
        lane_stds: floats(model.get_lane_line_stds()?),
    };
    let leads = model
        .get_leads_v3()?
        .iter()
        .map(|lead| {
            Ok(Lead {
                x: floats(lead.get_x()?),
                y: floats(lead.get_y()?),
                prob: Number(f64::from(lead.get_prob())),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Message {
        timestamp,
        value: Some(Arc::new(Input { geometry, leads })),
    })
}
