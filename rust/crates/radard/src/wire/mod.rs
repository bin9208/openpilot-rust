mod decode;
mod publication;
use crate::{controller::Options, math::maximum, Error};
use capnp::{message::ReaderOptions, serialize};
pub use decode::{display_points, model, points, yaw};
use openpilot_cereal::car_capnp::car_params;
pub use publication::publication;

pub fn configuration(bytes: &[u8], radar_mode: i32, corner_mode: i32) -> Result<Options, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let cp = message.get_root::<car_params::Reader<'_>>()?;
    let brand = cp.get_brand()?.to_str()?;
    Ok(Options {
        prefer_corner_radar: brand == "hyundai"
            && corner_mode > 0
            && cp.get_ext_flags() & (4096 | 8192 | 16384) != 0,
        enable_radar_tracks: if brand == "hyundai" {
            radar_mode
        } else if cp.get_radar_unavailable() {
            -2
        } else {
            1
        },
        front_radar_measurement_delay_s: if brand == "volkswagen" && cp.get_flags() & 16 != 0 {
            0.
        } else {
            maximum(0., f64::from(cp.get_radar_delay()))
        },
        production_live_tracks: true,
        ..Options::default()
    })
}

/// Source Bosch subscriptions consume both conflated 20Hz streams so their
/// drifting clocks cannot discard alternating radar publications. Model updates
/// remain the sole controller/publication trigger.
pub fn subscriptions(
    bytes: &[u8],
    radar_mode: i32,
) -> Result<openpilot_messaging::state::Options, Error> {
    let message = serialize::read_message(bytes, ReaderOptions::new())?;
    let cp = message.get_root::<car_params::Reader<'_>>()?;
    let mut options = crate::daemon::options();
    if cp.get_brand()?.to_str()? == "hyundai"
        && cp.get_ext_flags() & (1 << 17) != 0
        && radar_mode > 0
    {
        options.poll =
            openpilot_messaging::state::Poll::Many(vec!["modelV2".into(), "liveTracks".into()]);
    }
    Ok(options)
}
