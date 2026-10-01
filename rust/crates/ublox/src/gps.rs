use crate::{
    binary::Cursor,
    layouts::gps::{How, Subframe1, Subframe2, Subframe3},
    Error,
};
use openpilot_cereal::log_capnp::ublox_gnss;
use std::collections::BTreeMap;

fn header(data: &[u8]) -> Result<(How, Cursor<'_>), Error> {
    let mut cursor = Cursor::new(data);
    if cursor.u8()? != 0x8b {
        return Err(Error::Malformed("GPS preamble"));
    }
    cursor.skip(2)?;
    Ok((How::read(&mut cursor)?, cursor))
}
pub fn ephemeris(
    caches: &mut BTreeMap<u8, BTreeMap<u8, Vec<u8>>>,
    sv: u8,
    words: &[u32],
    gnss: ublox_gnss::Builder<'_>,
) -> Result<bool, Error> {
    if words.len() != 10 {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(30);
    for word in words {
        bytes.extend_from_slice(&(word >> 6).to_be_bytes()[1..]);
    }
    let (how, mut cursor) = header(&bytes)?;
    let id = match how.subframe_id {
        1 => {
            Subframe1::read(&mut cursor)?;
            1
        }
        2 => {
            Subframe2::read(&mut cursor)?;
            2
        }
        3 => {
            Subframe3::read(&mut cursor)?;
            3
        }
        _ => return Ok(false),
    };
    let cache = caches.entry(sv).or_default();
    cache.insert(id, bytes);
    if cache.len() != 3 {
        return Ok(false);
    }
    let data = |id| {
        cache
            .get(&id)
            .map(Vec::as_slice)
            .ok_or(Error::Malformed("GPS cache"))
    };
    let (h1, mut r1) = header(data(1)?)?;
    let (h2, mut r2) = header(data(2)?)?;
    let (_, mut r3) = header(data(3)?)?;
    let s1 = Subframe1::read(&mut r1)?;
    let s2 = Subframe2::read(&mut r2)?;
    let s3 = Subframe3::read(&mut r3)?;
    let mut out = gnss.init_ephemeris();
    let mut week = s1.week_no + 1024;
    if week < 1877 {
        week += 1024;
    }
    if s2.t_oe == 0 && h2.tow_count * 6 >= 604800 - 7200 {
        week += 1;
    }
    let week = u16::try_from(week).map_err(|_| Error::Malformed("GPS week"))?;
    #[expect(
        clippy::approx_constant,
        reason = "source GPS ICD pi constant differs from std::f64::consts::PI"
    )]
    let pi = 3.1415926535898;
    out.set_sv_id(u16::from(sv));
    out.set_tgd(f64::from(s1.t_gd) * 2f64.powi(-31));
    out.set_toc(f64::from(s1.t_oc) * 2f64.powi(4));
    out.set_af2(f64::from(s1.af_2) * 2f64.powi(-55));
    out.set_af1(f64::from(s1.af_1) * 2f64.powi(-43));
    out.set_af0(
        (f64::from(s1.af_0_value) - if s1.af_0_sign { f64::from(1 << 21) } else { 0. })
            * 2f64.powi(-31),
    );
    out.set_sv_health(f64::from(s1.sv_health));
    out.set_tow_count(h1.tow_count);
    out.set_crs(f64::from(s2.c_rs) * 2f64.powi(-5));
    out.set_delta_n(f64::from(s2.delta_n) * 2f64.powi(-43) * pi);
    out.set_m0(f64::from(s2.m_0) * 2f64.powi(-31) * pi);
    out.set_cuc(f64::from(s2.c_uc) * 2f64.powi(-29));
    out.set_ecc(f64::from(s2.e) * 2f64.powi(-33));
    out.set_cus(f64::from(s2.c_us) * 2f64.powi(-29));
    out.set_a((f64::from(s2.sqrt_a) * 2f64.powi(-19)).powf(2.0));
    out.set_toe(f64::from(s2.t_oe) * 2f64.powi(4));
    out.set_cic(f64::from(s3.c_ic) * 2f64.powi(-29));
    out.set_omega0(f64::from(s3.omega_0) * 2f64.powi(-31) * pi);
    out.set_cis(f64::from(s3.c_is) * 2f64.powi(-29));
    out.set_i0(f64::from(s3.i_0) * 2f64.powi(-31) * pi);
    out.set_crc(f64::from(s3.c_rc) * 2f64.powi(-5));
    out.set_omega(f64::from(s3.omega) * 2f64.powi(-31) * pi);
    out.set_omega_dot(
        (f64::from(s3.omega_dot_value)
            - if s3.omega_dot_sign {
                f64::from(1 << 23)
            } else {
                0.
            })
            * 2f64.powi(-43)
            * pi,
    );
    out.set_iode(f64::from(s3.iode));
    out.set_i_dot(
        (f64::from(s3.idot_value) - if s3.idot_sign { f64::from(1 << 13) } else { 0. })
            * 2f64.powi(-43)
            * pi,
    );
    out.set_toe_week(week);
    out.set_toc_week(week);
    cache.clear();
    Ok(s1.iodc_lsb == s2.iode && s2.iode == s3.iode)
}
