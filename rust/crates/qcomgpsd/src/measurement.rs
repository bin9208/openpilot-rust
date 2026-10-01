use crate::{
    reports::{GlonassReport, GlonassSatellite, GpsReport, GpsSatellite},
    status, wire, Error,
};
use openpilot_cereal::log_capnp::qcom_gnss::{measurement_report, MeasurementSource};
macro_rules! common_report {
    ($report:expr, $data:expr) => {{
        $report.set_f_count($data.f_count);
        $report.set_milliseconds($data.milliseconds);
        $report.set_time_bias($data.time_bias);
        $report.set_clock_time_uncertainty($data.clock_time_uncertainty);
        $report.set_clock_frequency_bias($data.clock_frequency_bias);
        $report.set_clock_frequency_uncertainty($data.clock_frequency_uncertainty);
    }};
}
macro_rules! common_satellite {
    ($target:expr, $data:expr, $source:expr) => {{
        let data = $data;
        let mut target = $target;
        target.set_sv_id(data.sv_id);
        wire::enum_field(
            target.reborrow(),
            "observationState",
            u32::from(data.observation_state),
        )?;
        target.set_observations(data.observations);
        target.set_good_observations(data.good_observations);
        target.set_filter_stages(data.filter_stages);
        target.set_carrier_noise(data.carrier_noise);
        target.set_latency(data.latency);
        target.set_predetect_interval(data.predetect_interval);
        target.set_postdetections(data.postdetections);
        target.set_unfiltered_measurement_integral(data.unfiltered_measurement_integral);
        target.set_unfiltered_measurement_fraction(data.unfiltered_measurement_fraction);
        target.set_unfiltered_time_uncertainty(data.unfiltered_time_uncertainty);
        target.set_unfiltered_speed(data.unfiltered_speed);
        target.set_unfiltered_speed_uncertainty(data.unfiltered_speed_uncertainty);
        status::fill(
            target.reborrow().init_measurement_status(),
            data.measurement_status,
            $source,
        );
        target.set_multipath_estimate(data.multipath_estimate);
        target.set_azimuth(data.azimuth);
        target.set_elevation(data.elevation);
        target.set_carrier_phase_cycles_integral(data.carrier_phase_cycles_integral);
        target.set_carrier_phase_cycles_fraction(data.carrier_phase_cycles_fraction);
        target.set_fine_speed(data.fine_speed);
        target.set_fine_speed_uncertainty(data.fine_speed_uncertainty);
        target.set_cycle_slip_count(data.cycle_slip_count);
    }};
}
pub fn fill(
    mut report: measurement_report::Builder<'_>,
    bytes: &[u8],
    gps: bool,
) -> Result<(), Error> {
    let (count, start, size) = if gps {
        let data = GpsReport::decode(bytes)?;
        if data.version != 0 {
            return Err(Error::Protocol("GPS measurement version"));
        }
        report.set_source(MeasurementSource::Gps);
        report.set_gps_week(data.week);
        common_report!(report, data);
        (data.sv_count, GpsReport::SIZE, GpsSatellite::SIZE)
    } else {
        let data = GlonassReport::decode(bytes)?;
        if data.version != 0 {
            return Err(Error::Protocol("GLONASS measurement version"));
        }
        report.set_source(MeasurementSource::Glonass);
        report.set_glonass_cycle_number(data.glonass_cycle_number);
        report.set_glonass_number_of_days(data.glonass_number_of_days);
        common_report!(report, data);
        (data.sv_count, GlonassReport::SIZE, GlonassSatellite::SIZE)
    };
    let sats = &bytes[start..];
    let mut list = report.init_sv(u32::from(count));
    // Preserve the source floor-division check, including its permitted short tail.
    if count != 0 && sats.len() / usize::from(count) != size {
        return Err(Error::Protocol("measurement satellite extent"));
    }
    for index in 0..u32::from(count) {
        let start = usize::try_from(index).map_err(|_| Error::Protocol("satellite index"))? * size;
        let bytes = &sats[start..start + size];
        let mut target = list.reborrow().get(index);
        if gps {
            let data = GpsSatellite::decode(bytes)?;
            target.set_gps_parity_error_count(data.parity_error_count);
            common_satellite!(target, &data, status::Source::Gps(data.misc_status));
        } else {
            let data = GlonassSatellite::decode(bytes)?;
            target.set_glonass_frequency_index(data.frequency_index);
            target.set_glonass_hemming_error_count(data.hemming_error_count);
            common_satellite!(target, &data, status::Source::Glonass(data.misc_status));
        }
    }
    Ok(())
}
