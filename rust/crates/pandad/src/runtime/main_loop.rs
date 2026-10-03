use super::{
    effects::{Effects, PeripheralEffects},
    platform, Error, Shared,
};
use crate::{
    can_io::BulkTransport, device::Control, inputs::Inputs, peripheral::Peripheral, safety::Safety,
    state, state_wire,
};
use openpilot_cereal::{car_capnp::car_params::SafetyModel, log_capnp::event};
use openpilot_logging::record::Level;
use openpilot_messaging::{runtime::PubMaster, services};
use openpilot_msgq::{MultiSubscriber, Subscription};
use openpilot_params::Params;
use std::{sync::atomic::Ordering, time::Duration};

struct InputReader {
    subscriber: MultiSubscriber,
    state: Inputs,
}

impl InputReader {
    fn new(names: &[&str]) -> Result<Self, Error> {
        let specifications = names
            .iter()
            .map(|name| {
                let service =
                    services::lookup(name).ok_or(Error::Contract("missing Panda input service"))?;
                Ok(Subscription {
                    endpoint: name,
                    capacity: service.queue_size,
                    polled: true,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let simulation = std::env::var_os("SIMULATION").is_some_and(|value| value == "1");
        let frequency = services::lookup("selfdriveState")
            .ok_or(Error::Contract("missing selfdriveState service"))?
            .frequency as f32;
        Ok(Self {
            subscriber: MultiSubscriber::for_runtime(&specifications)?,
            state: Inputs::new(simulation, frequency),
        })
    }

    fn update(&mut self) -> Result<u64, Error> {
        let ready = self.subscriber.poll_ready(Duration::ZERO)?;
        let now = platform::now_ns();
        let mut messages = Vec::with_capacity(ready.len());
        for index in ready {
            if let Some(bytes) = self.subscriber.receive_one(index)? {
                messages.push(bytes);
            }
        }
        self.state.update(now, &messages)?;
        Ok(now)
    }
}

fn peripheral_state(shared: &Shared, publisher: &mut PubMaster) -> Result<(), Error> {
    let panda = &shared.pandas[0];
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(platform::now_ns());
    event.set_valid(panda.transport().comms_healthy());
    let mut output = event.init_peripheral_state();
    state_wire::raw_enum(
        output.reborrow().into(),
        "pandaType",
        u16::from(panda.hardware_type()),
    )?;
    let started = platform::seconds() * 1000.0;
    let (mut voltage, mut current) = shared.hardware.power();
    let elapsed = platform::seconds() * 1000.0 - started;
    if elapsed > 50.0 {
        shared
            .logs
            .write(Level::Warning, format!("reading hwmon took {elapsed:.6}ms"));
    }
    if voltage == 0 && current == 0 {
        if let Some(health) = panda.health()? {
            voltage = health.voltage;
            current = health.current;
        }
    }
    output.set_voltage(voltage);
    output.set_current(current);
    output.set_fan_speed_rpm(panda.fan_speed()?);
    publisher.send(
        "peripheralState",
        &capnp::serialize::write_message_to_words(&message),
    )?;
    Ok(())
}

pub fn run(shared: &Shared, frames: Option<u64>) -> Result<(), Error> {
    let params = Params::for_runtime()?;
    let mut rate = platform::RateKeeper::new();
    let mut state_inputs = InputReader::new(&["selfdriveState"])?;
    let mut publisher = PubMaster::for_runtime(&["pandaStates", "peripheralState"])?;
    let mut safety = Safety::default();
    let mut state_publisher = state::Publisher::default();
    let mut peripheral = Peripheral::default();
    let mut peripheral_inputs = None;
    let identities = shared
        .pandas
        .iter()
        .map(|panda| state::Identity {
            hardware_type: panda.hardware_type(),
            serial: panda.transport().serial().to_vec(),
        })
        .collect::<Vec<_>>();
    let mut engaged = false;
    let mut onroad = false;
    let result = (|| {
        while shared.connected() {
            if rate.frame.is_multiple_of(5) {
                if peripheral_inputs.is_none() {
                    peripheral_inputs =
                        Some(InputReader::new(&["deviceState", "driverCameraState"])?);
                }
                if let Some(inputs) = peripheral_inputs.as_mut() {
                    inputs.update()?;
                    let input = inputs
                        .state
                        .peripheral(platform::now_ns(), !shared.no_fan_control);
                    let mut output = PeripheralEffects {
                        panda: &shared.pandas[0],
                        params: &params,
                        hardware: &shared.hardware,
                        error: None,
                    };
                    peripheral.update(input, &mut output)?;
                    if let Some(error) = output.error {
                        return Err(error);
                    }
                }
            }
            if rate.frame.is_multiple_of(10) {
                let now = state_inputs.update()?;
                engaged = state_inputs.state.engaged(now);
                onroad = params.get_bool("IsOnroad")?;
                shared.onroad.store(onroad, Ordering::Relaxed);
                let mut effects = Effects {
                    pandas: &shared.pandas,
                    identities: &identities,
                    factory: &shared.factory,
                    params: &params,
                    publisher: &mut publisher,
                    stop: &shared.stop,
                    logs: &shared.logs,
                    publication_ns: platform::now_ns(),
                };
                let ignition = state_publisher.update(
                    state::Input {
                        onroad,
                        engaged,
                        spoofing_started: shared.spoofing_started,
                    },
                    &mut effects,
                )?;
                safety.configure(onroad && ignition.unwrap_or(true), &mut effects)?;
            }
            if rate.frame.is_multiple_of(50) {
                peripheral_state(shared, &mut publisher)?;
            }
            if rate.frame.is_multiple_of(10) {
                for (index, panda) in shared.pandas.iter().enumerate() {
                    let serial = panda.serial_read(0)?;
                    if !serial.is_empty() {
                        shared.logs.serial(index, &serial);
                    }
                }
            }
            rate.keep_time()?;
            if frames == Some(rate.frame) {
                shared.stop.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    })();
    result?;
    if onroad && !engaged {
        for panda in &shared.pandas {
            if panda.transport().connected() {
                panda.control(Control::new(0xdc, u16::from(SafetyModel::NoOutput), 0))?;
            }
        }
    }
    Ok(())
}
