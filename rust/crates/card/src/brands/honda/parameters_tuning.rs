use super::Error;
use openpilot_cereal::car_capnp::car_params;

pub(super) fn tuning(
    cp: &mut car_params::Builder<'_>,
    candidate: &str,
    modified: bool,
) -> Result<(), Error> {
    let (bp, values, kp, ki): (&[i32], &[i32], f32, f32) = match candidate {
        "HONDA_CIVIC" if modified => (&[0, 2560, 8000], &[0, 2560, 3840], 0.3, 0.1),
        "HONDA_CIVIC" => (&[0, 2560], &[0, 2560], 1.1, 0.33),
        "HONDA_CIVIC_BOSCH" | "HONDA_CIVIC_BOSCH_DIESEL" | "HONDA_CIVIC_2022" | "HONDA_HRV_3G" => {
            (&[0, 4096], &[0, 4096], 0.8, 0.24)
        }
        "HONDA_ACCORD" if modified => (&[0, 4096], &[0, 4096], 0.3, 0.09),
        "HONDA_ACCORD" | "HONDA_CRV_HYBRID" | "HONDA_INSIGHT" | "HONDA_E" => {
            (&[0, 4096], &[0, 4096], 0.6, 0.18)
        }
        "ACURA_ILX" => (&[0, 3840], &[0, 3840], 0.8, 0.24),
        "HONDA_CRV" | "HONDA_CRV_EU" | "ACURA_RDX" => (&[0, 1000], &[0, 1000], 0.8, 0.24),
        "HONDA_CRV_5G" if modified => (&[0, 2560, 10000], &[0, 2560, 3840], 0.21, 0.07),
        "HONDA_CRV_5G" => (&[0, 3840], &[0, 3840], 0.64, 0.192),
        "HONDA_FIT" | "HONDA_FREED" => (&[0, 4096], &[0, 4096], 0.2, 0.05),
        "HONDA_HRV" => (&[0, 4096], &[0, 4096], 0.16, 0.025),
        "ACURA_RDX_3G" => (&[0, 3840], &[0, 3840], 0.2, 0.06),
        "HONDA_ODYSSEY" => (&[0, 4096], &[0, 4096], 0.28, 0.08),
        "HONDA_ODYSSEY_CHN" => (&[0, 32767], &[0, 32767], 0.28, 0.08),
        "HONDA_PILOT" | "HONDA_RIDGELINE" => (&[0, 4096], &[0, 4096], 0.38, 0.11),
        unknown => return Err(Error::Platform(unknown.to_owned())),
    };
    let mut lateral = cp.reborrow().init_lateral_params();
    let mut points = lateral
        .reborrow()
        .init_torque_b_p(u32::try_from(bp.len()).map_err(|_| Error::Numeric)?);
    for (index, value) in bp.iter().enumerate() {
        points.set(u32::try_from(index).map_err(|_| Error::Numeric)?, *value);
    }
    let mut output =
        lateral.init_torque_v(u32::try_from(values.len()).map_err(|_| Error::Numeric)?);
    for (index, value) in values.iter().enumerate() {
        output.set(u32::try_from(index).map_err(|_| Error::Numeric)?, *value);
    }
    let mut pid = cp.reborrow().get_lateral_tuning().init_pid();
    pid.reborrow().init_ki_b_p(1).set(0, 0.);
    pid.reborrow().init_kp_b_p(1).set(0, 0.);
    pid.reborrow().init_kp_v(1).set(0, kp);
    pid.reborrow().init_ki_v(1).set(0, ki);
    pid.set_kf(0.00006);
    if matches!(
        candidate,
        "HONDA_CRV" | "HONDA_CRV_EU" | "HONDA_CRV_5G" | "HONDA_CRV_HYBRID" | "HONDA_HRV"
    ) {
        cp.set_wheel_speed_factor(1.025);
    }
    Ok(())
}
