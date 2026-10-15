use super::{effects::Effects, views::Views, Controller, Error};
use crate::radar_only_warning::Point;
use openpilot_cereal::{
    car_capnp::{car_state, radar_data::radar_point::RadarSource},
    log_capnp::{self, onroad_event::EventName},
};
use openpilot_messaging::state::State;
impl Controller {
    pub(super) fn radar_warning_event(
        &mut self,
        cs: car_state::Reader<'_>,
        state: &State,
        effects: &mut impl Effects,
    ) -> Result<(), Error> {
        let now = if self.mode.replay {
            self.car_state_time_s
        } else {
            effects.monotonic()
        };
        let views = Views { state };
        let model = views.model()?;
        let mt = model.get_timestamp_eof() as f64 * 1e-9;
        let radar_topic = state.topic("liveTracks")?;
        let rt = radar_topic.log_mono_time as f64 * 1e-9;
        let fresh = ["modelV2", "liveTracks", "radarState", "carControl"]
            .iter()
            .all(|name| {
                state.topic(name).is_ok_and(|t| {
                    t.valid && (0. ..=0.2).contains(&(now - t.log_mono_time as f64 * 1e-9))
                })
            });
        let enabled = self.enabled
            && self.config.car.openpilot_longitudinal_control
            && fresh
            && (0. ..=0.25).contains(&(now - mt))
            && (mt - rt).abs() <= 0.15
            && views.control()?.get_long_active()
            && cs.get_can_valid()
            && !cs.get_gas_pressed()
            && !cs.get_brake_pressed()
            && !views.plan()?.get_fcw()
            && !model.get_meta()?.get_hard_brake_predicted();
        if !enabled {
            self.radar_warning.clear();
            return Ok(());
        }
        if !state.topic("modelV2")?.updated || self.radar_warning.last_radar_time == Some(rt) {
            return Ok(());
        }
        self.radar_warning.last_radar_time = Some(rt);
        let event = radar_topic.event()?;
        let tracks = match event.which()? {
            log_capnp::event::Which::LiveTracks(v) => v?,
            _ => return Err(Error::Contract("liveTracks topic/event mismatch")),
        };
        let mut points = Vec::new();
        for p in tracks.get_points()?.iter() {
            let (d, y, v, yv) = (
                f64::from(p.get_d_rel()),
                f64::from(p.get_y_rel()),
                f64::from(p.get_v_rel()),
                f64::from(p.get_yv_rel()),
            );
            if ![d, y, v, yv].iter().all(|v| v.is_finite()) || !p.get_measured() {
                continue;
            }
            let source = match p.get_radar_source()? {
                RadarSource::FrontRadar => "frontRadar",
                RadarSource::Scc => "scc",
                RadarSource::Corner235 => "corner235",
                RadarSource::Corner180 => "corner180",
                RadarSource::Corner430 => "corner430",
            };
            let delta = mt - rt
                + if source.starts_with("corner") {
                    0.05
                } else {
                    self.config.front_radar_delay
                };
            points.push(Point {
                source: source.into(),
                track_id: p.get_track_id(),
                d: d + v * delta,
                y: y + yv * delta,
                v,
                yv,
                speed: f64::from(cs.get_v_ego()) + v,
                measured: true,
                track_state: i32::from(p.get_track_state()),
            });
        }
        let native_model = openpilot_radard::wire::model(model)?;
        let lane = openpilot_radard::stationary_lane::confident_ego_lane(&native_model);
        let probs = model
            .get_leads_v3()?
            .iter()
            .map(|l| f64::from(l.get_prob()))
            .collect::<Vec<_>>();
        let radar = views.radar()?;
        if self.radar_warning.update(
            mt,
            f64::from(cs.get_v_ego()),
            lane.as_ref().map(|(l, _)| l),
            &probs,
            &points,
            true,
            radar.get_lead_one()?.get_status() || radar.get_lead_two()?.get_status(),
        ) {
            self.events.add(EventName::RadarStationaryLead, false);
        }
        Ok(())
    }
}
