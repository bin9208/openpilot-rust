use super::{input, publication, ApplyInput, Card, Error, StateTail, StepIo, Vehicle};
use crate::{can_wire, toggle::Button, xiaoge};
use openpilot_cereal::car_capnp::{car_params, car_state};

impl Card {
    pub fn step(
        &mut self,
        vehicle: &mut impl Vehicle,
        tail: &mut impl StateTail,
        io: &mut impl StepIo,
    ) -> Result<(), Error> {
        let cpu_start = io.thread_cpu_ns();
        let raw = io.receive_can_raw()?;
        let receive = io.monotonic_ns();
        self.diagnostics.receive(receive, raw.is_empty());
        let packets = raw
            .iter()
            .map(|bytes| can_wire::decode(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let decoded = io.monotonic_ns();
        let mut message = vehicle.update(&packets, decoded)?;
        vehicle.emit_diagnostics(io)?;
        let updated = io.monotonic_ns();
        io.update_subscribers()?;
        let subscribed = io.monotonic_ns();
        if let Some(bytes) = input::vision_bytes(io.subscribers())? {
            match xiaoge::parse(bytes) {
                Ok(result) => self.vision = Some(result),
                Err(error) => {
                    self.vision = None;
                    if subscribed.saturating_sub(self.vision_error_at) >= 5_000_000_000 {
                        io.warning(&format!("invalid Xiaoge vision payload: {error}"))?;
                        self.vision_error_at = subscribed;
                    }
                }
            }
        }
        let mut state = message.get_root::<car_state::Builder>()?;
        xiaoge::apply(state.reborrow(), self.vision.as_ref(), subscribed);
        if raw.is_empty() {
            self.can_timeouts = self.can_timeouts.checked_add(1).ok_or(Error::Numeric)?;
        }
        if self.replay && !raw.is_empty() {
            self.replay_time = packets.first().map(|packet| packet.mono_time);
        }
        let mut radar = state.reborrow().init_radar_input();
        radar.set_first_can_mono_time(packets.first().map_or(0, |packet| packet.mono_time));
        radar.set_last_can_mono_time(packets.last().map_or(0, |packet| packet.mono_time));
        radar.set_can_packet_count(u32::try_from(packets.len())?);
        radar.set_receive_mono_time(if self.replay && !raw.is_empty() {
            self.replay_time.ok_or(Error::ReplayTimestamp)?
        } else {
            receive
        });
        let vision_done = io.monotonic_ns();
        if let Some(flags) = io.settings_flags() {
            self.metric = flags.metric;
            self.experimental = flags.experimental;
        } else if self
            .settings_updated
            .is_none_or(|previous| subscribed.saturating_sub(previous) >= 100_000_000)
        {
            self.metric = self.settings.get_bool("IsMetric")?;
            self.experimental = self.settings.get_bool("ExperimentalMode")?
                && self
                    .params
                    .get_root_as_reader::<car_params::Reader>()?
                    .get_openpilot_longitudinal_control();
            self.settings_updated = Some(subscribed);
        }
        let tail_now = io.now();
        tail.update(state.reborrow(), io.subscribers(), self.metric, tail_now)?;
        for (key, bytes) in tail.take_param_writes() {
            io.put_nonblocking(&key, &bytes)?;
        }
        for line in tail.take_prints() {
            io.print(&line)?;
        }
        if input::control(io.subscribers())?.get_enabled() && !self.previous_enabled {
            if let Some(flags) = io.settings_flags() {
                self.experimental = flags.experimental;
            }
            tail.initialize(
                self.previous_state
                    .get_root_as_reader::<car_state::Reader>()?,
                self.experimental,
            )?;
        }
        tail.project(state.reborrow())?;
        vehicle.set_soft_hold(state.reborrow_as_reader().get_soft_hold_active());
        vehicle.commit_state(state.reborrow_as_reader())?;
        let state_done = io.monotonic_ns();
        for (index, start, end) in [
            (0, receive, decoded),
            (1, decoded, updated),
            (2, updated, subscribed),
            (3, subscribed, vision_done),
            (4, vision_done, state_done),
            (5, receive, state_done),
        ] {
            self.diagnostics.stage(index, start, end);
        }
        let buttons = state
            .reborrow_as_reader()
            .get_button_events()?
            .iter()
            .map(|button| {
                Ok(Button {
                    kind: button.get_type()? as u16,
                    pressed: button.get_pressed(),
                })
            })
            .collect::<Result<Vec<_>, capnp::NotInSchema>>()?;
        if self.toggle.update(
            &buttons,
            (input::control(io.subscribers())?.get_enabled(), io.now()),
        ) {
            if self.has_controller
                && !self
                    .params
                    .get_root_as_reader::<car_params::Reader>()?
                    .get_dashcam_only()
            {
                let enabled = !self.settings.get_bool("OpenpilotEnabledToggle")?;
                let enabled_text = if enabled { "True" } else { "False" };
                io.warning(&format!(
                    "Cruise MAIN long press: setting OpenpilotEnabledToggle to {enabled_text}"
                ))?;
                self.settings.put_bool("OpenpilotEnabledToggle", enabled)?;
                self.settings.put_bool("OnroadCycleRequested", true)?;
            } else {
                io.warning("Cruise MAIN long press ignored: vehicle has no openpilot controller")?;
            }
        }
        let publish_start = io.monotonic_ns();
        publication::publish(self, state.into_reader(), io)?;
        let published = io.monotonic_ns();
        self.diagnostics.stage(6, publish_start, published);
        let initialized = input::initialized(io.subscribers())?;
        if !self
            .params
            .get_root_as_reader::<car_params::Reader>()?
            .get_passive()
            && initialized
        {
            if !self.initialized_previous {
                vehicle.init(io)?;
                vehicle.emit_diagnostics(io)?;
                io.put_nonblocking("ControlsReady", b"1")?;
            }
            if io.subscribers().all_alive(&["carControl"])? {
                let apply_start = io.monotonic_ns();
                let now_ns = if self.replay {
                    self.replay_time.ok_or(Error::ReplayTimestamp)?
                } else {
                    publication::event_time(io)?
                };
                let result = vehicle.apply(ApplyInput {
                    control: input::control(io.subscribers())?,
                    now_ns,
                    model: input::model(io.subscribers())?,
                    radar: input::radar(io.subscribers())?,
                });
                for (key, bytes) in vehicle.take_param_writes() {
                    io.put_nonblocking(&key, &bytes)?;
                }
                let output = result?;
                vehicle.emit_diagnostics(io)?;
                self.last_actuators = output.actuators;
                let applied = io.monotonic_ns();
                let state = message.get_root_as_reader::<car_state::Reader>()?;
                let bytes = can_wire::sendcan(
                    &output.can,
                    state.get_can_valid(),
                    publication::event_time(io)?,
                )?;
                io.publish("sendcan", &bytes)?;
                let sent = io.monotonic_ns();
                self.diagnostics.stage(7, apply_start, applied);
                self.diagnostics.stage(8, applied, sent);
                self.diagnostics.stage(9, receive, sent);
                for line in self.diagnostics.applied() {
                    io.warning(&line)?;
                }
                self.previous_enabled = input::control(io.subscribers())?.get_enabled();
            }
        }
        self.initialized_previous = initialized;
        self.previous_state = message;
        let end = io.monotonic_ns();
        let cpu_end = io.thread_cpu_ns();
        io.diagnostics(&self.diagnostics.values(end, cpu_start, cpu_end)?)?;
        Ok(())
    }
}
