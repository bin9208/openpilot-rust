use super::decoder_parser;
use openpilot_radarcan::{decoder::hyundai::Hyundai, Error};
use serde_json::{json, Value};

pub fn snapshot(state: &mut Hyundai) -> Result<(Value, Vec<String>), Error> {
    let mut parsers = serde_json::Map::new();
    let mut warnings = Vec::new();
    for (role, reader) in [
        ("rcp_tracks", state.rcp_tracks.as_mut()),
        ("rcp_scc", state.rcp_scc.as_mut()),
        ("rcp_corner_objects", state.rcp_corner_objects.as_mut()),
        (
            "rcp_corner_objects_180",
            state.rcp_corner_objects_180.as_mut(),
        ),
        ("rcp_corner_objects_430", None),
    ] {
        let (parser, diagnostics) = decoder_parser::snapshot(reader)?;
        parsers.insert(role.to_owned(), parser);
        warnings.extend(diagnostics);
    }
    let manager = &state.corner_object_track_ids;
    let previous = state
        .group3_track_ids
        .previous
        .iter()
        .map(|(id, value)| (id.to_string(), json!(value)))
        .collect::<serde_json::Map<String, Value>>();
    let mut fields = serde_json::from_value::<serde_json::Map<String, Value>>(
        json!({"track_id":state.track_id,"canfd":state.canfd,"radar_group1":state.radar_group1,
        "radar_group3":state.radar_group3,"radar_group4":state.radar_group4,"radar_start_addr":state.radar_start_addr,
        "radar_msg_count":state.radar_msg_count,"radar_required_msg_count":state.radar_required_msg_count,
        "radar_tracks":state.radar_tracks,"corner_object_tracks":state.corner_object_tracks,
        "corner_object_180_tracks":state.corner_object_180_tracks,"corner_object_430_tracks":state.corner_object_430_tracks,
        "corner_object_missed_updates":state.corner_object_missed_updates,"corner_object_180_missed_updates":state.corner_object_180_missed_updates,
        "corner_object_430_missed_updates":state.corner_object_430_missed_updates,"trigger_msg_scc":state.trigger_msg_scc,
        "trigger_msg_tracks":state.trigger_msg_tracks,"trigger_msg_corner_objects":state.trigger_msg_corner_objects,
        "trigger_msg_corner_objects_180":state.trigger_msg_corner_objects_180,"trigger_msg_corner_objects_430":state.trigger_msg_corner_objects_430,
        "corner_objects_available":state.corner_objects_available,"radar_off_can":state.radar_off_can,
        "vRel_last":state.v_rel_last,"dRel_last":state.d_rel_last}),
    )?;
    fields.extend(serde_json::from_value::<serde_json::Map<String,Value>>(json!({
        "updated_tracks":state.updated_tracks.iter().collect::<Vec<_>>(),"updated_scc":state.updated_scc.iter().collect::<Vec<_>>(),
        "updated_corner_objects":state.updated_corner_objects.iter().collect::<Vec<_>>(),"updated_corner_objects_180":state.updated_corner_objects_180.iter().collect::<Vec<_>>(),
        "updated_corner_objects_430":state.updated_corner_objects_430.iter().collect::<Vec<_>>(),
        "corner_object_430_prev_d_rel":{},"corner_object_430_prev_v_rel":{},"corner_object_430_prev_y_rel":{},
        "corner_object_430_prev_yv_rel":{},"corner_object_430_prev_code":{},"corner_object_430_history":{},"corner_object_430_noncenter_inward_frames":{},
        "corner_object_track_ids":{"next_track_id":manager.next_id,"source_cycles":manager.cycles,
            "track_states":manager.states.iter().map(|((source,id),previous)| json!([[source,id],
                [previous.slot,previous.object_id,previous.age,previous.distance,previous.lateral,previous.cycle]])).collect::<Vec<_>>()},
        "group3_track_ids":{"next_id":state.group3_track_ids.next_id,"previous":previous},"parsers":parsers}))?);
    Ok((Value::Object(fields), warnings))
}
