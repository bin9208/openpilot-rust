use crate::{controller::Output, Error, Lead};
use capnp::{message::Builder, serialize, struct_list};
use num_traits::ToPrimitive;
use openpilot_cereal::{
    car_capnp::radar_data,
    log_capnp::{event, radar_state},
};
fn float(value: f64) -> Result<f32, Error> {
    value
        .to_f32()
        .ok_or(Error::Contract("radarState float32 conversion"))
}
fn lead(mut output: radar_state::lead_data::Builder<'_>, value: &Lead) -> Result<(), Error> {
    output.set_d_rel(float(value.d_rel)?);
    output.set_y_rel(float(value.y_rel)?);
    output.set_v_rel(float(value.v_rel)?);
    output.set_a_rel(float(value.a_rel)?);
    output.set_v_lead(float(value.v_lead)?);
    output.set_a_lead(float(value.a_lead)?);
    output.set_d_path(float(value.d_path)?);
    output.set_v_lat(float(value.v_lat)?);
    output.set_v_lead_k(float(value.v_lead_k)?);
    output.set_a_lead_k(float(value.a_lead_k)?);
    output.set_fcw(value.fcw);
    output.set_status(value.status);
    output.set_a_lead_tau(float(value.a_lead_tau)?);
    output.set_model_prob(float(value.model_prob)?);
    output.set_radar(value.radar);
    output.set_radar_track_id(value.radar_track_id);
    output.set_j_lead(float(value.j_lead)?);
    output.set_score(float(value.score)?);
    output.set_cut_out_time(float(value.cut_out_time)?);
    output.set_cut_out_confidence(float(value.cut_out_confidence)?);
    Ok(())
}
fn leads(
    mut output: struct_list::Builder<'_, radar_state::lead_data::Owned>,
    values: &[Lead],
) -> Result<(), Error> {
    for (index, value) in values.iter().enumerate() {
        lead(output.reborrow().get(length(index)?), value)?;
    }
    Ok(())
}
fn length(value: usize) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| Error::Contract("radarState list length"))
}
pub fn publication(
    output: &Output,
    errors: radar_data::error::Reader<'_>,
    model_ns: u64,
    car_ns: u64,
    valid: bool,
    now: u64,
) -> Result<Vec<u8>, Error> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(now);
    event.set_valid(valid);
    let mut radar = event.init_radar_state();
    radar.set_md_mono_time(model_ns);
    radar.set_car_state_mono_time(car_ns);
    radar.set_radar_errors(errors)?;
    lead(
        radar.reborrow().init_lead_one(),
        &output.lead_one.unwrap_or_default(),
    )?;
    lead(
        radar.reborrow().init_lead_two(),
        &output.lead_two.unwrap_or_default(),
    )?;
    lead(
        radar.reborrow().init_lead_left(),
        &output.lead_left.unwrap_or_default(),
    )?;
    lead(
        radar.reborrow().init_lead_right(),
        &output.lead_right.unwrap_or_default(),
    )?;
    lead(
        radar.reborrow().init_lead_cut_in_risk(),
        &output.lead_cutin_risk.unwrap_or_default(),
    )?;
    leads(
        radar
            .reborrow()
            .init_leads_left(length(output.leads_left.len())?),
        &output.leads_left,
    )?;
    leads(
        radar
            .reborrow()
            .init_leads_center(length(output.leads_center.len())?),
        &output.leads_center,
    )?;
    leads(
        radar
            .reborrow()
            .init_leads_right(length(output.leads_right.len())?),
        &output.leads_right,
    )?;
    leads(
        radar
            .reborrow()
            .init_leads_cut_in(length(output.leads_cutin.len())?),
        &output.leads_cutin,
    )?;
    leads(
        radar
            .reborrow()
            .init_leads_left2(length(output.leads_left2.len())?),
        &output.leads_left2,
    )?;
    leads(
        radar.init_leads_right2(length(output.leads_right2.len())?),
        &output.leads_right2,
    )?;
    Ok(serialize::write_message_to_words(&message))
}
