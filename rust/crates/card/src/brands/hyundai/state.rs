use super::{
    bus::{contains, Fingerprints},
    config::CarConfig,
    flags as f,
    limits::TorqueLimits,
    monitor::Monitor,
    navigation::Navigation,
    parameters::setting_int,
    parser_inputs::{Channel, Inputs},
    Error,
};
use crate::state_helpers::{Blinkers, SpeedFilter, SteeringPressed};
use capnp::message::{Builder, HeapAllocator};
use openpilot_can::Packet;
use openpilot_cereal::car_capnp::car_state::{self, GearShifter};
use openpilot_params::Params;
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

pub struct Capabilities {
    pub lfa_button: bool,
    pub alt_cruise_button: bool,
    pub alt_lfa_button: bool,
    pub gear: bool,
    pub alt_gear: bool,
    pub tpms: bool,
    pub local_time: bool,
}

pub struct CanSetup<'a> {
    pub path: &'a Path,
    pub fingerprints: &'a Fingerprints,
    pub now_ns: u64,
}

pub struct State {
    pub config: CarConfig,
    pub settings: Params,
    pub inputs: Inputs,
    pub monitor: Monitor,
    pub navigation: Navigation,
    pub soft_hold: i16,
    pub out: Builder<HeapAllocator>,
    pub metric: bool,
    pub main_enabled: bool,
    pub manual_main_off: bool,
    pub gear_shifter: GearShifter,
    pub limits: TorqueLimits,
    pub speed_filter: SpeedFilter,
    pub cluster_filter: SpeedFilter,
    pub blinkers: Blinkers,
    pub steering_pressed: SteeringPressed,
    pub cluster_speed: f64,
    pub cluster_counter: u32,
    pub cluster_seen: bool,
    pub cruise_buttons: VecDeque<u8>,
    pub main_buttons: VecDeque<u8>,
    pub paddle: u8,
    pub buttons_counter: f64,
    pub main_mode: bool,
    pub acc_mode: f64,
    pub lfa_icon: f64,
    pub bsm_channel: Option<Channel>,
    pub trailer_connected: bool,
    pub trailer_count: u32,
    pub avh_latched: bool,
    pub avh_grace: u32,
    pub scc_hold: bool,
    pub gear_values: BTreeMap<i64, String>,
    pub capabilities: Capabilities,
    pub time_zone: Option<&'static str>,
    pub zone: Option<super::local_time::Zone>,
}

impl State {
    pub fn new(config: CarConfig, settings: Params, can: CanSetup<'_>) -> Result<Self, Error> {
        let CanSetup {
            path,
            fingerprints,
            now_ns,
        } = can;
        let mut inputs = Inputs::new(path, &config, now_ns)?;
        super::diagnostics::constructor(&mut inputs, config.flags & f::CANFD != 0);
        let monitor = Monitor::new(&config, &settings)?;
        let navigation = Navigation::new(config.candidate == "KIA_PV5", &settings)?;
        let main_enabled = setting_int(&settings, "AutoEngage")? == 2;
        let (gear_message, gear_signal) = if config.flags & f::CANFD != 0 {
            (config.gear_message(), "GEAR")
        } else if config.flags & (f::HYBRID | f::EV) != 0 {
            ("ELECT_GEAR", "Elect_Gear_Shifter")
        } else if config.flags & f::CLUSTER_GEARS != 0 {
            ("CLU15", "CF_Clu_Gear")
        } else if config.flags & f::TCU_GEARS != 0 {
            ("TCU12", "CUR_GR")
        } else if config.flags & f::FCEV != 0 {
            ("EMS20", "HYDROGEN_GEAR_SHIFTER")
        } else {
            ("LVR12", "CF_Lvr_Gear")
        };
        let address = inputs.pt.dbc.message(gear_message)?.address;
        let gear_values = inputs
            .pt
            .dbc
            .definitions()?
            .get(&address)
            .and_then(|signals| signals.get(gear_signal))
            .cloned()
            .ok_or_else(|| Error::Signal(gear_signal.into()))?;
        let capabilities = Capabilities {
            lfa_button: contains(fingerprints, 0, 0x391),
            alt_cruise_button: contains(fingerprints, 0, 0x3ef),
            alt_lfa_button: contains(fingerprints, 0, 0x416),
            gear: contains(fingerprints, config.bus.ecan, 69),
            alt_gear: contains(fingerprints, config.bus.ecan, 64),
            tpms: contains(fingerprints, config.bus.ecan, 0x3a0),
            local_time: contains(fingerprints, config.bus.ecan, 1264),
        };
        let limits = TorqueLimits::new(&config.candidate, config.flags);
        let mut out = Builder::new_default();
        out.init_root::<car_state::Builder>();
        Ok(Self {
            config,
            settings,
            inputs,
            monitor,
            navigation,
            soft_hold: 0,
            out,
            metric: false,
            main_enabled,
            manual_main_off: false,
            gear_shifter: GearShifter::Drive,
            limits,
            speed_filter: SpeedFilter::new()?,
            cluster_filter: SpeedFilter::new()?,
            blinkers: Blinkers::default(),
            steering_pressed: SteeringPressed::default(),
            cluster_speed: 0.,
            cluster_counter: 20,
            cluster_seen: false,
            cruise_buttons: VecDeque::from([0; 8]),
            main_buttons: VecDeque::from([0; 8]),
            paddle: 0,
            buttons_counter: 0.,
            main_mode: false,
            acc_mode: 0.,
            lfa_icon: 0.,
            bsm_channel: None,
            trailer_connected: false,
            trailer_count: 0,
            avh_latched: false,
            avh_grace: 0,
            scc_hold: false,
            gear_values,
            capabilities,
            time_zone: None,
            zone: None,
        })
    }

    pub fn commit(&mut self, reader: car_state::Reader<'_>) -> Result<(), Error> {
        self.out.set_root(reader)?;
        Ok(())
    }

    pub fn update(
        &mut self,
        packets: &[Packet],
        now: u64,
    ) -> Result<Builder<HeapAllocator>, Error> {
        self.inputs.update(packets)?;
        self.monitor
            .update(&mut self.inputs, &self.config, &self.settings, now)?;
        let mut message = Builder::new_default();
        let mut ret = message.init_root::<car_state::Builder>();
        if self.config.flags & f::CANFD != 0 {
            super::state_canfd::update(self, ret.reborrow(), now)?;
        } else {
            super::state_legacy::update(self, ret.reborrow(), now)?;
        }
        let [valid, timeout] = self.inputs.validity();
        ret.set_can_valid(valid);
        ret.set_can_timeout(timeout);
        let mut cluster = ret.reborrow_as_reader().get_v_ego_cluster();
        if cluster == 0. && !self.cluster_seen {
            cluster = ret.reborrow_as_reader().get_v_ego();
        } else {
            self.cluster_seen = true;
        }
        ret.set_v_ego_cluster(cluster);
        let mut cruise = ret.reborrow().get_cruise_state()?;
        if cruise.reborrow_as_reader().get_speed_cluster() == 0. {
            let speed = cruise.reborrow_as_reader().get_speed();
            cruise.set_speed_cluster(speed);
        }
        let enabled = crate::state_helpers::button_enable(
            self.config.pcm,
            ret.reborrow_as_reader().get_button_events()?,
        )?;
        ret.set_button_enable(enabled);
        self.commit(ret.into_reader())?;
        Ok(message)
    }
}
