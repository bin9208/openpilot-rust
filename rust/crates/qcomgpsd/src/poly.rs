use crate::{reports::SvPoly, Error};
use openpilot_cereal::log_capnp::qcom_gnss::dr_sv_poly_report;
pub fn fill(mut target: dr_sv_poly_report::Builder<'_>, bytes: &[u8]) -> Result<(), Error> {
    let data = SvPoly::decode(bytes)?;
    if data.version != 2 {
        return Err(Error::Protocol("SV polynomial version"));
    }
    target.set_sv_id(data.sv_id);
    target.set_frequency_index(data.frequency_index);
    target.set_iode(data.iode);
    target.set_t0(data.t0);
    target.set_xyz0(&data.xyz0)?;
    target.set_xyz_n(&data.xyz_n)?;
    target.set_other(&data.other)?;
    target.set_position_uncertainty(data.position_uncertainty);
    target.set_iono_delay(data.iono_delay);
    target.set_iono_dot(data.iono_dot);
    target.set_sbas_iono_delay(data.sbas_iono_delay);
    target.set_sbas_iono_dot(data.sbas_iono_dot);
    target.set_tropo_delay(data.tropo_delay);
    target.set_elevation(data.elevation);
    target.set_elevation_dot(data.elevation_dot);
    target.set_elevation_uncertainty(data.elevation_uncertainty);
    target.set_velocity_coeff(&data.velocity_coeff)?;
    Ok(())
}
