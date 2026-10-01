use crate::{estimator::Estimator, types::Input, wire, Error};
use openpilot_messaging::state::State;

pub struct Output {
    pub packet: Option<Vec<u8>>,
    pub cache: bool,
    pub gps: Option<Vec<u8>>,
}
pub struct LoopState {
    pub estimator: Estimator,
    pub gps_service: String,
    pub debug: bool,
}
impl LoopState {
    pub fn step(&mut self, state: &State, timestamp: u64) -> Result<Output, Error> {
        let valid = state.all_checks(&[])?;
        if valid {
            let mut topics: Vec<_> = state
                .topics()
                .iter()
                .filter(|topic| topic.updated)
                .collect();
            topics.sort_by_key(|topic| topic.log_mono_time);
            for topic in topics {
                self.estimator.handle(wire::decode_event(topic.event()?)?)?;
            }
        }
        let mut gps_bytes = None;
        let gps = state.topic(&self.gps_service)?;
        if gps.updated {
            if let Input::Gps {
                fix: true,
                latitude,
                longitude,
                bearing,
            } = wire::decode_event(gps.event()?)?.input
            {
                let mut text = String::from("{\"latitude\": ");
                for (i, value) in [latitude, longitude, bearing].into_iter().enumerate() {
                    openpilot_runtime_core::python_float::write_float(value, &mut text)
                        .map_err(|_| Error::Contract("GPS number formatting"))?;
                    text.push_str([", \"longitude\": ", ", \"bearing\": ", "}"][i]);
                }
                gps_bytes = Some(text.into_bytes());
            }
        }
        let packet = if state.topic("livePose")?.updated {
            Some(wire::encode(
                &self.estimator.parameters(valid, self.debug)?,
                timestamp,
            )?)
        } else {
            None
        };
        Ok(Output {
            cache: packet.is_some() && state.frame() % 1200 == 0,
            packet,
            gps: gps_bytes,
        })
    }
}
