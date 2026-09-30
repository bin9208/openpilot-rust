use crate::{
    action::Action, geometry, prediction::DrivingPrediction, publication::PublishState,
    wire_helpers as wire,
};
use openpilot_cereal::log_capnp::{event, model_data_v2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct ModelTiming {
    pub log_mono_time: u64,
    pub frame_id: u32,
    pub frame_id_extra: u32,
    pub camera_state_frame_id: u32,
    pub frame_drop: f64,
    pub timestamp_eof: u64,
    pub model_execution_time: f64,
    pub valid: bool,
}

pub struct ModelFrame<'a> {
    pub prediction: &'a DrivingPrediction,
    pub timing: ModelTiming,
    pub action: Action,
    pub raw_predictions: Option<&'a [u8]>,
}

pub fn build(
    frame: ModelFrame<'_>,
    state: &mut PublishState,
) -> capnp::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_log_mono_time(frame.timing.log_mono_time);
    event.set_valid(frame.timing.valid);
    let mut model = event.init_model_v2();
    let data = frame.prediction;
    model.set_frame_id(frame.timing.frame_id);
    model.set_frame_id_extra(frame.timing.frame_id_extra);
    model.set_frame_age(
        frame
            .timing
            .camera_state_frame_id
            .saturating_sub(frame.timing.frame_id),
    );
    model.set_frame_drop_perc(wire::float(frame.timing.frame_drop * 100.0));
    model.set_timestamp_eof(frame.timing.timestamp_eof);
    model.set_model_execution_time(wire::float(frame.timing.model_execution_time));
    wire::action(model.reborrow().init_action(), frame.action);
    let times = geometry::TIME.map(wire::float);
    wire::xyzt(
        model.reborrow().init_position(),
        &times,
        &data.plan.map(|row| [row[0], row[1], row[2]]),
    )?;
    wire::xyz_std(
        model.reborrow().get_position()?,
        &data.plan_std.map(|row| [row[0], row[1], row[2]]),
    )?;
    wire::xyzt(
        model.reborrow().init_velocity(),
        &times,
        &data.plan.map(|r| [r[3], r[4], r[5]]),
    )?;
    wire::xyzt(
        model.reborrow().init_acceleration(),
        &times,
        &data.plan.map(|r| [r[6], r[7], r[8]]),
    )?;
    wire::xyzt(
        model.reborrow().init_orientation(),
        &times,
        &data.plan.map(|r| [r[9], r[10], r[11]]),
    )?;
    wire::xyzt(
        model.reborrow().init_orientation_rate(),
        &times,
        &data.plan.map(|r| [r[12], r[13], r[14]]),
    )?;
    lines(model.reborrow(), data)?;
    leads(model.reborrow(), data)?;
    let status = state.update(&data.meta, frame.timing.frame_id);
    model.set_confidence(status.confidence);
    meta(model.reborrow().init_meta(), data, status.hard_brake)?;
    if let Some(raw) = frame.raw_predictions {
        model.set_raw_predictions(raw);
    }
    Ok(message)
}

fn lines(mut model: model_data_v2::Builder<'_>, data: &DrivingPrediction) -> capnp::Result<()> {
    let times = geometry::line_times(&data.plan).map(wire::float);
    let mut lanes = model.reborrow().init_lane_lines(4);
    for i in 0_u32..4 {
        let base = usize::try_from(i).map_err(|e| capnp::Error::failed(e.to_string()))? * 66;
        let points = std::array::from_fn(|j| {
            [
                wire::float(geometry::DISTANCE[j]),
                data.lanes.mean[base + j * 2],
                data.lanes.mean[base + j * 2 + 1],
            ]
        });
        wire::xyzt(lanes.reborrow().get(i), &times, &points)?;
    }
    model.set_lane_line_probs(
        &[
            data.lane_prob[1],
            data.lane_prob[3],
            data.lane_prob[5],
            data.lane_prob[7],
        ][..],
    )?;
    model.set_lane_line_stds(
        &[
            data.lanes.std[0],
            data.lanes.std[66],
            data.lanes.std[132],
            data.lanes.std[198],
        ][..],
    )?;
    let mut edges = model.reborrow().init_road_edges(2);
    for i in 0_u32..2 {
        let base = usize::try_from(i).map_err(|e| capnp::Error::failed(e.to_string()))? * 66;
        let points = std::array::from_fn(|j| {
            [
                wire::float(geometry::DISTANCE[j]),
                data.edges.mean[base + j * 2],
                data.edges.mean[base + j * 2 + 1],
            ]
        });
        wire::xyzt(edges.reborrow().get(i), &times, &points)?;
    }
    model.set_road_edge_stds(&[data.edges.std[0], data.edges.std[66]][..])
}

fn leads(mut model: model_data_v2::Builder<'_>, data: &DrivingPrediction) -> capnp::Result<()> {
    let mut leads = model.reborrow().init_leads_v3(3);
    for i in 0_u32..3 {
        let index = usize::try_from(i).map_err(|e| capnp::Error::failed(e.to_string()))?;
        let base = index * 24;
        let mut lead = leads.reborrow().get(i);
        let mean: [[f32; 6]; 4] = std::array::from_fn(|col| {
            std::array::from_fn(|row| data.leads.mean[base + row * 4 + col])
        });
        let std: [[f32; 6]; 4] = std::array::from_fn(|col| {
            std::array::from_fn(|row| data.leads.std[base + row * 4 + col])
        });
        lead.set_t(&[0.0, 2.0, 4.0, 6.0, 8.0, 10.0][..])?;
        lead.set_x(&mean[0][..])?;
        lead.set_y(&mean[1][..])?;
        lead.set_v(&mean[2][..])?;
        lead.set_a(&mean[3][..])?;
        lead.set_x_std(&std[0][..])?;
        lead.set_y_std(&std[1][..])?;
        lead.set_v_std(&std[2][..])?;
        lead.set_a_std(&std[3][..])?;
        lead.set_prob(data.lead_prob[index]);
        lead.set_prob_time([0.0, 2.0, 4.0][index]);
    }
    Ok(())
}

fn meta(
    mut meta: model_data_v2::meta_data::Builder<'_>,
    data: &DrivingPrediction,
    hard_brake: bool,
) -> capnp::Result<()> {
    meta.set_desire_state(&data.desire_state[..])?;
    meta.set_desire_prediction(&data.desire_prediction[..])?;
    meta.set_engaged_prob(data.meta[0]);
    meta.set_hard_brake_predicted(hard_brake);
    let mut disengage = meta.init_disengage_predictions();
    disengage.set_t(&[2.0, 4.0, 6.0, 8.0, 10.0][..])?;
    let probability = |start| -> [f32; 5] { std::array::from_fn(|i| data.meta[start + i * 6]) };
    disengage.set_brake_disengage_probs(&probability(2)[..])?;
    disengage.set_gas_disengage_probs(&probability(1)[..])?;
    disengage.set_steer_override_probs(&probability(3)[..])?;
    disengage.set_brake3_meters_per_second_squared_probs(&probability(4)[..])?;
    disengage.set_brake4_meters_per_second_squared_probs(&probability(5)[..])?;
    disengage.set_brake5_meters_per_second_squared_probs(&probability(6)[..])?;
    let gas: [f32; 6] = std::array::from_fn(|i| data.meta[31 + i * 4]);
    let brake: [f32; 6] = std::array::from_fn(|i| data.meta[32 + i * 4]);
    disengage.set_gas_press_probs(&gas[..])?;
    disengage.set_brake_press_probs(&brake[..])
}
