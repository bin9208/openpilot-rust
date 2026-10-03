use crate::{
    reports::{DrReport, DrSatellite},
    status, wire, Error,
};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::qcom_gnss::dr_measurement_report;
pub fn fill(mut report: dr_measurement_report::Builder<'_>, bytes: &[u8]) -> Result<(), Error> {
    let data = DrReport::decode(bytes)?;
    if data.version != 2 {
        return Err(Error::Protocol("DR measurement version"));
    }
    report.set_reason(data.reason);
    report.set_seq_num(data.seq_num);
    report.set_seq_max(data.seq_max);
    report.set_rf_loss(data.rf_loss);
    report.set_system_rtc_valid(data.system_rtc_valid != 0);
    report.set_f_count(data.f_count);
    report.set_clock_resets(data.clock_resets);
    report.set_system_rtc_time(data.system_rtc_time);
    report.set_gps_leap_seconds(data.gps_leap_seconds);
    report.set_gps_leap_seconds_uncertainty(data.gps_leap_seconds_uncertainty);
    report.set_gps_to_glonass_time_bias_milliseconds(data.gps_to_glonass_time_bias_milliseconds);
    report.set_gps_to_glonass_time_bias_milliseconds_uncertainty(
        data.gps_to_glonass_time_bias_milliseconds_uncertainty,
    );
    report.set_gps_week(data.gps_week);
    report.set_gps_milliseconds(data.gps_milliseconds);
    report.set_gps_time_bias_ms(data.gps_time_bias);
    report.set_gps_clock_time_uncertainty_ms(data.gps_clock_time_uncertainty);
    report.set_gps_clock_source(data.gps_clock_source);
    report.set_glonass_clock_source(data.glonass_clock_source);
    report.set_glonass_year(data.glonass_year);
    report.set_glonass_day(data.glonass_day);
    report.set_glonass_milliseconds(data.glonass_milliseconds);
    report.set_glonass_time_bias(data.glonass_time_bias);
    report.set_glonass_clock_time_uncertainty(data.glonass_clock_time_uncertainty);
    report.set_clock_frequency_bias(data.clock_frequency_bias);
    report.set_clock_frequency_uncertainty(data.clock_frequency_uncertainty);
    report.set_frequency_source(data.frequency_source);
    wire::enum_field(report.reborrow(), "source", u32::from(data.source))?;
    let mut list = report.init_sv(u32::from(data.sv_count));
    for index in 0..u32::from(data.sv_count) {
        let start = DrReport::SIZE
            + usize::try_from(index).map_err(|_| Error::Protocol("satellite index"))?
                * DrSatellite::SIZE;
        let data = DrSatellite::decode(
            bytes
                .get(start..)
                .ok_or(Error::Protocol("truncated DR satellite"))?,
        )?;
        let mut target = list.reborrow().get(index);
        target.set_sv_id(data.sv_id);
        target.set_glonass_frequency_index(data.glonass_frequency_index);
        wire::enum_field(
            target.reborrow(),
            "observationState",
            data.observation_state,
        )?;
        target.set_observations(data.observations);
        target.set_good_observations(data.good_observations);
        target.set_filter_stages(data.filter_stages);
        target.set_predetect_interval(data.predetect_interval);
        target.set_cycle_slip_count(data.cycle_slip_count);
        target.set_postdetections(data.postdetections);
        status::fill(
            target.reborrow().init_measurement_status(),
            data.measurement_status,
            status::Source::Dr {
                multipath: data.multipath_estimate_valid,
                direction: data.direction_valid,
            },
        );
        target.set_carrier_noise(data.carrier_noise);
        target.set_rf_loss(data.rf_loss);
        target.set_latency(data.latency);
        target.set_filtered_measurement_fraction(data.filtered_measurement_fraction);
        target.set_filtered_measurement_integral(data.filtered_measurement_integral);
        target.set_filtered_time_uncertainty(data.filtered_time_uncertainty);
        target.set_filtered_speed(data.filtered_speed);
        target.set_filtered_speed_uncertainty(data.filtered_speed_uncertainty);
        target.set_unfiltered_measurement_fraction(data.unfiltered_measurement_fraction);
        target.set_unfiltered_measurement_integral(data.unfiltered_measurement_integral);
        target.set_unfiltered_time_uncertainty(data.unfiltered_time_uncertainty);
        target.set_unfiltered_speed(data.unfiltered_speed);
        target.set_unfiltered_speed_uncertainty(data.unfiltered_speed_uncertainty);
        target.set_multipath_estimate(data.multipath_estimate);
        target.set_azimuth(data.azimuth);
        target.set_elevation(data.elevation);
        target.set_doppler_acceleration(data.doppler_acceleration);
        target.set_fine_speed(data.fine_speed);
        target.set_fine_speed_uncertainty(data.fine_speed_uncertainty);
        target.set_carrier_phase(
            data.carrier_phase
                .to_f64()
                .ok_or(Error::Protocol("carrier phase conversion"))?,
        );
        target.set_f_count(data.f_count);
        target.set_parity_error_count(data.parity_error_count);
        target.set_good_parity(data.good_parity != 0);
    }
    Ok(())
}
