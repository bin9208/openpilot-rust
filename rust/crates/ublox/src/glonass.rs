use crate::{
    binary::Cursor,
    layouts::glonass::{String1, String2, String3, String4, String5},
    parser::GlonassString,
    Error,
};
use openpilot_cereal::log_capnp::ublox_gnss;
use std::collections::BTreeMap;

fn signed(value: u32, sign: bool) -> f64 {
    if sign {
        -f64::from(value)
    } else {
        f64::from(value)
    }
}
fn cursor(data: &[u8]) -> Result<Cursor<'_>, Error> {
    let mut cursor = Cursor::new(data);
    cursor.bits(5)?;
    Ok(cursor)
}
pub fn ephemeris(
    caches: &mut BTreeMap<u8, BTreeMap<u8, GlonassString>>,
    received: (u8, u8, f64),
    words: &[u32],
    gnss: ublox_gnss::Builder<'_>,
) -> Result<bool, Error> {
    if words.len() != 4 {
        return Ok(false);
    }
    let (sv, frequency, time) = received;
    let bytes: Vec<_> = words.iter().flat_map(|word| word.to_be_bytes()).collect();
    let number = (bytes[0] >> 3) & 15;
    if bytes[0] & 128 != 0 || !(1..=5).contains(&number) {
        return Ok(false);
    }
    let superframe = u16::from_be_bytes([bytes[12], bytes[13]]);
    let cache = caches.entry(frequency).or_default();
    let mut unknown = false;
    let mut clear = false;
    for (i, prev) in cache.iter() {
        if prev.superframe == 0 || superframe == 0 {
            unknown = true;
        } else if prev.superframe != superframe {
            clear = true;
        }
        if unknown
            && ((prev.time - 2. * f64::from(*i)) - (time - 2. * f64::from(number))).abs() > 10.
        {
            clear = true;
        }
    }
    if clear {
        cache.clear();
    }
    cache.insert(
        number,
        GlonassString {
            bytes,
            superframe,
            time,
        },
    );
    if sv == 255 || cache.len() != 5 {
        return Ok(false);
    }
    let data = |i| {
        cache
            .get(&i)
            .ok_or(Error::Malformed("GLONASS cache"))
            .and_then(|entry| cursor(&entry.bytes))
    };
    let s1 = String1::read(&mut data(1)?)?;
    let s2 = String2::read(&mut data(2)?)?;
    let s3 = String3::read(&mut data(3)?)?;
    let s4 = String4::read(&mut data(4)?)?;
    let s5 = String5::read(&mut data(5)?)?;
    let mut out = gnss.init_glonass_ephemeris();
    out.set_sv_id(u16::from(sv));
    out.set_freq_num(i16::from(frequency) - 7);
    out.set_p1(u8::try_from(s1.p1).map_err(|_| Error::Malformed("p1"))?);
    out.reborrow()
        .get_deprecated()
        .set_tk(u16::try_from(s1.t_k).map_err(|_| Error::Malformed("tk"))?);
    out.set_x_vel(signed(s1.x_vel_value, s1.x_vel_sign) * 2f64.powi(-20));
    out.set_x_accel(signed(s1.x_accel_value, s1.x_accel_sign) * 2f64.powi(-30));
    out.set_x(signed(s1.x_value, s1.x_sign) * 2f64.powi(-11));
    out.set_sv_health(
        u8::try_from(s2.b_n >> 2).map_err(|_| Error::Malformed("health"))? | u8::from(s3.l_n),
    );
    out.set_p2(u8::from(s2.p2));
    out.set_tb(u16::try_from(s2.t_b).map_err(|_| Error::Malformed("tb"))?);
    out.set_y_vel(signed(s2.y_vel_value, s2.y_vel_sign) * 2f64.powi(-20));
    out.set_y_accel(signed(s2.y_accel_value, s2.y_accel_sign) * 2f64.powi(-30));
    out.set_y(signed(s2.y_value, s2.y_sign) * 2f64.powi(-11));
    out.set_p3(u8::from(s3.p3));
    out.set_gamma_n(signed(s3.gamma_n_value, s3.gamma_n_sign) * 2f64.powi(-40));
    out.set_z_vel(signed(s3.z_vel_value, s3.z_vel_sign) * 2f64.powi(-20));
    out.set_z_accel(signed(s3.z_accel_value, s3.z_accel_sign) * 2f64.powi(-30));
    out.set_z(signed(s3.z_value, s3.z_sign) * 2f64.powi(-11));
    out.set_nt(u16::try_from(s4.n_t).map_err(|_| Error::Malformed("nt"))?);
    out.set_tau_n(signed(s4.tau_n_value, s4.tau_n_sign) * 2f64.powi(-30));
    out.set_delta_tau_n(signed(s4.delta_tau_n_value, s4.delta_tau_n_sign) * 2f64.powi(-30));
    out.set_age(u8::try_from(s4.e_n).map_err(|_| Error::Malformed("age"))?);
    out.set_p4(u8::from(s4.p4));
    let ura = [
        1., 2., 2.5, 4., 5., 7., 10., 12., 14., 16., 32., 64., 128., 256., 512., 1024.,
    ];
    out.set_sv_u_r_a(
        *ura.get(usize::try_from(s4.f_t).map_err(|_| Error::Malformed("URA"))?)
            .ok_or(Error::Malformed("URA"))?,
    );
    out.set_sv_type(u8::try_from(s4.m).map_err(|_| Error::Malformed("type"))?);
    out.set_n4(u8::try_from(s5.n_4).map_err(|_| Error::Malformed("n4"))?);
    out.set_tk_seconds(3600 * ((s1.t_k >> 7) & 31) + 60 * ((s1.t_k >> 1) & 63) + (s1.t_k & 1) * 30);
    cache.clear();
    Ok(true)
}
