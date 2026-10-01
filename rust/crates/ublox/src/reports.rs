use crate::{
    binary::Cursor,
    layouts::{hardware as hw, nav, raw},
    Error,
};
use openpilot_cereal::log_capnp::{gps_location_data, ublox_gnss};

pub fn nav(cursor: &mut Cursor<'_>, mut gps: gps_location_data::Builder<'_>) -> Result<(), Error> {
    let v = nav::NavPvt::read(cursor)?;
    gps.set_source(gps_location_data::SensorSource::Ublox);
    gps.set_flags(u16::from(v.flags));
    gps.set_has_fix(v.flags % 2 == 1);
    gps.set_latitude(f64::from(v.lat) * 1e-7);
    gps.set_longitude(f64::from(v.lon) * 1e-7);
    gps.set_altitude(f64::from(v.height) * 1e-3);
    gps.set_speed((f64::from(v.g_speed) * 1e-3) as f32);
    gps.set_bearing_deg((f64::from(v.head_mot) * 1e-5) as f32);
    gps.set_horizontal_accuracy((f64::from(v.h_acc) * 1e-3) as f32);
    gps.set_satellite_count(
        i8::try_from(v.num_sv).map_err(|_| Error::Malformed("satellite count"))?,
    );
    let utc = if (1..=9999).contains(&v.year) {
        chrono::NaiveDate::from_ymd_opt(i32::from(v.year), u32::from(v.month), 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|date| {
                date.and_utc().timestamp()
                    + (i64::from(v.day) - 1) * 86400
                    + i64::from(v.hour) * 3600
                    + i64::from(v.min) * 60
                    + i64::from(v.sec)
            })
            .unwrap_or(0)
    } else {
        0
    };
    gps.set_unix_timestamp_millis((utc as f64 * 1e3 + f64::from(v.nano) * 1e-6) as i64);
    let mut velocity = gps.reborrow().init_v_n_e_d(3);
    for (i, value) in [v.vel_n, v.vel_e, v.vel_d].into_iter().enumerate() {
        velocity.set(
            u32::try_from(i).map_err(|_| Error::Malformed("velocity index"))?,
            value as f32 * 1e-3f32,
        );
    }
    gps.set_vertical_accuracy((f64::from(v.v_acc) * 1e-3) as f32);
    gps.set_speed_accuracy((f64::from(v.s_acc) * 1e-3) as f32);
    gps.set_bearing_accuracy_deg((f64::from(v.head_acc) * 1e-5) as f32);
    Ok(())
}
pub fn raw(
    cursor: &mut Cursor<'_>,
    mut report: ublox_gnss::measurement_report::Builder<'_>,
) -> Result<(), Error> {
    let header = raw::Rawx::read(cursor)?;
    report.set_rcv_tow(header.rcv_tow);
    report.set_gps_week(header.week);
    report.set_leap_seconds(
        u16::try_from(header.leap_s).map_err(|_| Error::Malformed("negative leap seconds"))?,
    );
    report.set_num_meas(header.num_meas);
    let mut records = report
        .reborrow()
        .init_measurements(u32::from(header.num_meas));
    for i in 0..u32::from(header.num_meas) {
        let v = raw::Measurement::read(cursor)?;
        if v.gnss_id > 6 {
            return Err(Error::Malformed("GNSS enum"));
        }
        let mut out = records.reborrow().get(i);
        out.set_sv_id(v.sv_id);
        out.set_pseudorange(v.pr_mes);
        out.set_carrier_cycles(v.cp_mes);
        out.set_doppler(v.do_mes);
        out.set_gnss_id(v.gnss_id);
        out.set_glonass_frequency_index(v.freq_id);
        out.set_locktime(v.lock_time);
        out.set_cno(v.cno);
        out.set_pseudorange_stdev((0.01 * 2f64.powi(i32::from(v.pr_stdev & 15))) as f32);
        out.set_carrier_phase_stdev((0.004 * f64::from(v.cp_stdev & 15)) as f32);
        out.set_doppler_stdev((0.002 * 2f64.powi(i32::from(v.do_stdev & 15))) as f32);
        let mut status = out.init_tracking_status();
        status.set_pseudorange_valid(v.trk_stat & 1 != 0);
        status.set_carrier_phase_valid(v.trk_stat & 2 != 0);
        status.set_half_cycle_valid(v.trk_stat & 4 != 0);
        status.set_half_cycle_subtracted(v.trk_stat & 8 != 0);
    }
    let mut status = report.init_receiver_status();
    status.set_leap_sec_valid(header.rec_stat & 1 != 0);
    status.set_clk_reset(header.rec_stat & 4 != 0);
    Ok(())
}
pub fn sat(
    cursor: &mut Cursor<'_>,
    mut report: ublox_gnss::sat_report::Builder<'_>,
) -> Result<(), Error> {
    let header = nav::NavSat::read(cursor)?;
    report.set_i_tow(header.itow);
    let mut records = report.init_svs(u32::from(header.num_svs));
    for i in 0..u32::from(header.num_svs) {
        let v = nav::Satellite::read(cursor)?;
        if v.gnss_id > 6 {
            return Err(Error::Malformed("GNSS enum"));
        }
        let mut out = records.reborrow().get(i);
        out.set_sv_id(v.sv_id);
        out.set_gnss_id(v.gnss_id);
        out.set_flags_bitfield(v.flags);
        out.set_cno(v.cno);
        out.set_elevation_deg(v.elev);
        out.set_azimuth_deg(v.azim);
        out.set_pseudorange_residual((f64::from(v.pr_res) * 0.1) as f32);
    }
    Ok(())
}
pub fn hardware(
    cursor: &mut Cursor<'_>,
    mut report: ublox_gnss::hw_status::Builder<'_>,
) -> Result<(), Error> {
    let v = hw::MonHw::read(cursor)?;
    report.set_noise_per_m_s(v.noise_per_ms);
    report.set_flags(v.flags);
    report.set_agc_cnt(v.agc_cnt);
    report.set_a_status(v.a_status.into_enum()?);
    report.set_a_power(v.a_power.into_enum()?);
    report.set_jam_ind(v.jam_ind);
    Ok(())
}
trait AntennaEnum {
    fn into_enum<T: TryFrom<u16>>(self) -> Result<T, Error>;
}
impl AntennaEnum for u8 {
    fn into_enum<T: TryFrom<u16>>(self) -> Result<T, Error> {
        T::try_from(u16::from(self)).map_err(|_| Error::Malformed("antenna enum"))
    }
}
pub fn hardware2(
    cursor: &mut Cursor<'_>,
    mut report: ublox_gnss::hw_status2::Builder<'_>,
) -> Result<(), Error> {
    use ublox_gnss::hw_status2::ConfigSource;
    let v = hw::MonHw2::read(cursor)?;
    report.set_ofs_i(v.ofs_i);
    report.set_mag_i(v.mag_i);
    report.set_ofs_q(v.ofs_q);
    report.set_mag_q(v.mag_q);
    report.set_cfg_source(match v.cfg_source {
        113 => ConfigSource::Rom,
        111 => ConfigSource::Otp,
        112 => ConfigSource::Configpins,
        102 => ConfigSource::Flash,
        _ => ConfigSource::Undefined,
    });
    report.set_low_lev_cfg(v.low_lev_cfg);
    report.set_post_status(v.post_status);
    Ok(())
}
