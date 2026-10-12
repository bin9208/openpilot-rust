use super::{navigation, projection::Data, Error, Input, Projection};
use crate::models::{ClusterUiState, NaviLiveState};
use num_traits::ToPrimitive;

pub(super) struct Navigation {
    pub state: Option<NaviLiveState>,
    generation: Option<u64>,
    next_expiry: f64,
}
impl Default for Navigation {
    fn default() -> Self {
        Self {
            state: None,
            generation: None,
            next_expiry: f64::INFINITY,
        }
    }
}
impl Navigation {
    fn update(&mut self, data: Data<'_>, now: f64) {
        let generation = data.field("generation").uint();
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        self.state = navigation::parse_carrot_navi(data.0, now, self.state.as_ref());
        (self.state, self.next_expiry) = navigation::fresh_carrot_navi(self.state.as_ref(), now);
    }
    fn fresh(&mut self, now: f64) {
        if now >= self.next_expiry {
            (self.state, self.next_expiry) =
                navigation::fresh_carrot_navi(self.state.as_ref(), now);
        }
    }
}

impl Input {
    /// # Errors
    /// Returns native transport, Cereal state, or clock conversion errors.
    pub fn update(&mut self) -> Result<ClusterUiState, Error> {
        self.poll()?;
        self.project_at(super::monotonic()?)
    }
    /// # Errors
    /// Returns invalid Cereal subscription data or timestamp conversion errors.
    pub fn project_at(&mut self, now: f64) -> Result<ClusterUiState, Error> {
        if let Some(params) = &mut self.params {
            params.refresh_egpu(now);
        }
        let speed_start = self.profile.start();
        let car = self.subscriber.state.topic("carState")?;
        if car.alive {
            if let Some(speed) = Data(car.data()?).field("vEgo").float() {
                self.projection.state.speed_kph = (speed * 3.6).clamp(0.0, 260.0);
            }
        }
        self.profile.add("source.live.current_speed", speed_start);
        let updates_start = self.profile.start();
        for topic in self
            .subscriber
            .state
            .topics()
            .iter()
            .filter(|topic| topic.updated)
        {
            let time = if topic.log_mono_time == 0 {
                now
            } else {
                topic.log_mono_time.to_f64().ok_or(Error::Clock)? / 1e9
            };
            let data = topic.data()?;
            if topic.service.name == "carrotNavi" {
                self.navigation.update(Data(data), now);
            } else if topic.service.name == "navRoute" {
                self.route.update(Data(data));
            } else {
                self.projection
                    .apply_data(topic.service.name, data, time, topic.valid);
            }
        }
        self.profile.add("source.live.apply_updates", updates_start);
        if let Some(onroad) = self.onroad_state()? {
            self.projection.set_onroad(onroad);
        }
        let car = self.subscriber.state.topic("carState")?;
        let frame_start = self.profile.start();
        let frame_stage = if car.alive {
            "source.live.car_frame"
        } else {
            "source.live.standby_state"
        };
        let mut state = if car.alive {
            let time = if car.log_mono_time == 0 {
                now
            } else {
                car.log_mono_time.to_f64().ok_or(Error::Clock)? / 1e9
            };
            self.frames = self.frames.checked_add(1).ok_or(Error::Clock)?;
            self.projection.car_frame(car.data()?, time)
        } else {
            ClusterUiState::default()
        };
        self.profile.add(frame_stage, frame_start);
        self.navigation.fresh(now);
        self.with_debug_state(&mut state, now)?;
        state.navi_live.clone_from(&self.navigation.state);
        if let Some(media) = &mut self.media {
            state.navi_dashboard = Some(media.update_at(state.navi_live.as_ref(), now)?);
        }
        (state.speed_limit_kph, state.speed_limit_source) = navigation::resolve_navi_speed_limit(
            state.speed_limit_kph,
            state.speed_limit_source.as_deref(),
            state.navi_live.as_ref(),
        );
        self.hud(&mut state)?;
        state.egpu_active = self.params.as_ref().is_some_and(|params| params.egpu);
        state.alert = self.alert(state.alert.take(), state.onroad, now)?;
        self.last_state = Some(state.clone());
        Ok(state)
    }
    fn with_debug_state(&mut self, state: &mut ClusterUiState, now: f64) -> Result<(), Error> {
        if self.debug.panels.live
            || self.debug.panels.plot
            || self.debug.panels.navi
            || self.debug.panels.hud == 3
        {
            self.debug.refresh(self.params.as_ref(), now);
        }
        if self.debug.panels.live {
            let start = self.profile.start();
            state.live_debug = self.live_debug_info()?;
            self.profile.add("source.live.debug_info", start);
        }
        if self.debug.panels.plot {
            let start = self.profile.start();
            state.debug_plot = self.debug_plot_snapshot()?;
            self.profile.add("source.live.debug_plot", start);
        }
        if self.debug.panels.navi || self.debug.panels.hud == 3 {
            let start = self.profile.start();
            let info = self
                .debug
                .navigation_info(now, self.route.coordinates.len());
            self.route.apply(
                state,
                Data(self.subscriber.state.topic("carrotMan")?.data()?),
            );
            if state.speed_limit_kph.is_none() {
                state.speed_limit_kph = info.speed_limit_kph;
                if state.speed_limit_kph.is_some() {
                    state.speed_limit_source = Some("n".into());
                }
            }
            state.navi_debug = Some(info);
            self.profile.add("source.live.navi_debug", start);
        }
        if matches!(self.debug.panels.hud, 2 | 3) {
            state.debug_ui_visible = true;
        }
        Ok(())
    }
    pub(crate) fn data(&self, name: &str) -> Result<Data<'_>, Error> {
        Ok(Data(self.subscriber.state.topic(name)?.data()?))
    }
}

pub(super) fn initial_projection() -> Projection {
    let mut projection = Projection::default();
    projection.set_onroad(false);
    projection
}
