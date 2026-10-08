use crate::{navigation::v2::*, Error};
use openpilot_cereal::custom_capnp::carrot_navi_state::{self, guidance, item_meta};

fn text(value: capnp::text::Reader<'_>) -> String {
    value.to_str().unwrap_or_default().to_owned()
}
fn meta(value: item_meta::Reader<'_>) -> Meta {
    Meta {
        present: value.get_present(),
        sequence: value.get_sequence(),
        received_mono_time_nanos: value.get_received_mono_time_nanos(),
    }
}
fn guidance(value: guidance::Reader<'_>) -> Result<Guidance, Error> {
    Ok(Guidance {
        meta: meta(value.get_meta()?),
        distance_m: i64::from(value.get_distance_m()),
        turn_type: i64::from(value.get_turn_type()),
        main_text: text(value.get_main_text()?),
        near_direction: text(value.get_near_direction()?),
        far_direction: text(value.get_far_direction()?),
    })
}
pub fn read(value: carrot_navi_state::Reader<'_>) -> Result<Payload, Error> {
    let v = value.get_vehicle()?;
    let s = value.get_speed()?;
    let r = value.get_route()?;
    let t = value.get_traffic_signal()?;
    let status = value.get_navigation_status()?;
    let lane = value.get_lane_current()?;
    Ok(Payload {
        schema_version: i64::from(value.get_schema_version()),
        connected: value.get_connected(),
        session_id: text(value.get_session_id()?),
        guidance_current: guidance(value.get_guidance_current()?)?,
        guidance_next: guidance(value.get_guidance_next()?)?,
        vehicle: Vehicle {
            meta: meta(v.get_meta()?),
            latitude: v.get_latitude(),
            longitude: v.get_longitude(),
            heading_deg: f64::from(v.get_heading_deg()),
            speed_kph: f64::from(v.get_speed_kph()),
            road_name: text(v.get_road_name()?),
        },
        speed: Speed {
            meta: meta(s.get_meta()?),
            road_limit_valid: s.get_road_limit_valid(),
            road_limit_kph: i64::from(s.get_road_limit_kph()),
            sdi_present: s.get_sdi_present(),
            sdi_type: i64::from(s.get_sdi_type()),
            sdi_distance_m: i64::from(s.get_sdi_distance_m()),
            sdi_speed_limit_kph: i64::from(s.get_sdi_speed_limit_kph()),
            sdi_section_type: i64::from(s.get_sdi_section_type()),
            sdi_block_type: i64::from(s.get_sdi_block_type()),
            sdi_block_speed_kph: i64::from(s.get_sdi_block_speed_kph()),
            sdi_block_distance_m: i64::from(s.get_sdi_block_distance_m()),
            secondary_sdi_present: s.get_secondary_sdi_present(),
            secondary_sdi_type: i64::from(s.get_secondary_sdi_type()),
            secondary_sdi_distance_m: i64::from(s.get_secondary_sdi_distance_m()),
            secondary_sdi_speed_limit_kph: i64::from(s.get_secondary_sdi_speed_limit_kph()),
            secondary_sdi_section_type: i64::from(s.get_secondary_sdi_section_type()),
            secondary_sdi_block_type: i64::from(s.get_secondary_sdi_block_type()),
            secondary_sdi_block_speed_kph: i64::from(s.get_secondary_sdi_block_speed_kph()),
            secondary_sdi_block_distance_m: i64::from(s.get_secondary_sdi_block_distance_m()),
            section_present: s.get_section_present(),
            section_active: s.get_section_active(),
            section_suspended: s.get_section_suspended(),
            section_off_route: s.get_section_off_route(),
            section_speed_limit_kph: i64::from(s.get_section_speed_limit_kph()),
            section_remaining_distance_m: f64::from(s.get_section_remaining_distance_m()),
        },
        route: Route {
            meta: meta(r.get_meta()?),
            remaining_distance_m: i64::from(r.get_remaining_distance_m()),
            remaining_time_sec: i64::from(r.get_remaining_time_sec()),
            polyline: r
                .get_polyline()?
                .iter()
                .take(256)
                .map(|p| Coordinate {
                    latitude: p.get_latitude(),
                    longitude: p.get_longitude(),
                })
                .collect(),
        },
        navigation_status: Status {
            meta: meta(status.get_meta()?),
            off_route: status.get_off_route(),
            guidance_active: status.get_guidance_active(),
        },
        lane_current: Lane {
            meta: meta(lane.get_meta()?),
            road_category: i64::from(lane.get_road_category()),
        },
        traffic_signal: Traffic {
            meta: meta(t.get_meta()?),
            visible: t.get_visible(),
            distance_m: i64::from(t.get_distance_m()),
            source: text(t.get_source()?),
            red_valid: t.get_red_valid(),
            red_on: t.get_red_on(),
            red_remain_sec: i64::from(t.get_red_remain_sec()),
            left_valid: t.get_left_valid(),
            left_on: t.get_left_on(),
            left_remain_sec: i64::from(t.get_left_remain_sec()),
            green_valid: t.get_green_valid(),
            green_on: t.get_green_on(),
            green_remain_sec: i64::from(t.get_green_remain_sec()),
            right_valid: t.get_right_valid(),
            right_on: t.get_right_on(),
            right_remain_sec: i64::from(t.get_right_remain_sec()),
            uturn_valid: t.get_uturn_valid(),
            uturn_on: t.get_uturn_on(),
            uturn_remain_sec: i64::from(t.get_uturn_remain_sec()),
            ui_counter_valid: t.get_ui_counter_valid(),
            ui_counter_remain_sec: i64::from(t.get_ui_counter_remain_sec()),
        },
    })
}
