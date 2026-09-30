use crate::{wire, Calibrator, Error, Update};
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::state::State;

pub struct LoopOutput {
    pub update: Update,
    pub publish: bool,
    pub valid: bool,
}

/// One source main-loop iteration after SubMaster.update, including timeout frames.
pub fn step(
    calibrator: &mut Calibrator,
    state: &State,
    yaw_trim_deg: f64,
) -> Result<LoopOutput, Error> {
    let mut update = Update {
        rpy: None,
        persist: false,
    };
    let camera = state.topic("cameraOdometry")?;
    if camera.updated {
        let event::CarState(car) = state.topic("carState")?.event()?.which()? else {
            return Err(Error::Contract("carState topic contains another service"));
        };
        calibrator.v_ego = f64::from(car?.get_v_ego());
        if !calibrator.frozen(yaw_trim_deg) {
            let event::CameraOdometry(odometry) = camera.event()?.which()? else {
                return Err(Error::Contract(
                    "cameraOdometry topic contains another service",
                ));
            };
            update = calibrator.update(&wire::odometry(odometry?)?)?;
        }
    }
    Ok(LoopOutput {
        update,
        publish: state.frame() % 5 == 0,
        valid: state.all_checks(&[])?,
    })
}
