use super::Error;
use crate::{data::Data, point::Source};
use capnp::message::Builder;
use openpilot_cereal::{car_capnp::radar_data::radar_point::RadarSource, log_capnp::event};

pub fn encode(data: &Data, valid: bool, mono_time: u64) -> Result<Vec<u8>, Error> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    event.set_valid(valid);
    event.set_log_mono_time(mono_time);
    let mut tracks = event.init_live_tracks();
    tracks.set_radar_track_flipped(data.radar_track_flipped);
    let mut errors = tracks.reborrow().init_errors();
    errors.set_can_error(data.errors.can_error);
    errors.set_radar_fault(data.errors.radar_fault);
    errors.set_wrong_config(data.errors.wrong_config);
    errors.set_radar_unavailable_temporary(data.errors.radar_unavailable_temporary);
    if !data.points_present && data.points.is_empty() {
        return Ok(capnp::serialize::write_message_to_words(&message));
    }
    let mut points = tracks.init_points(u32::try_from(data.points.len())?);
    for (index, point) in data.points.iter().enumerate() {
        let mut output = points.reborrow().get(u32::try_from(index)?);
        output.set_track_id(point.track_id);
        output.set_d_rel(point.d_rel);
        output.set_y_rel(point.y_rel);
        output.set_v_rel(point.v_rel);
        output.set_a_rel(point.a_rel);
        output.set_yv_rel(point.yv_rel);
        output.set_measured(point.measured);
        output.set_v_lead(point.v_lead);
        output.set_a_lead(point.a_lead);
        output.set_j_lead(point.j_lead);
        output.set_track_state(point.track_state);
        output.set_radar_source(match point.radar_source {
            Source::FrontRadar => RadarSource::FrontRadar,
            Source::Scc => RadarSource::Scc,
            Source::Corner235 => RadarSource::Corner235,
            Source::Corner180 => RadarSource::Corner180,
            Source::Corner430 => RadarSource::Corner430,
        });
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
