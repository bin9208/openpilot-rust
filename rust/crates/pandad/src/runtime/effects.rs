use super::{logs::Logs, platform::Hardware, Error, Panda};
use crate::{
    can_io::BulkTransport, device::Control, native_transport::Factory, peripheral, safety, state,
    state_wire,
};
use openpilot_logging::record::Level;
use openpilot_messaging::runtime::PubMaster;
use openpilot_params::Params;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct Effects<'a> {
    pub pandas: &'a [Arc<Panda>],
    pub identities: &'a [state::Identity],
    pub factory: &'a Factory,
    pub params: &'a Params,
    pub publisher: &'a mut PubMaster,
    pub stop: &'a AtomicBool,
    pub logs: &'a Logs,
    pub publication_ns: u64,
}

impl state::Effects for Effects<'_> {
    type Error = Error;
    fn identities(&self) -> &[state::Identity] {
        self.identities
    }
    fn health(&mut self, index: usize) -> Result<Option<crate::health::Health>, Error> {
        Ok(self.pandas[index].health()?)
    }
    fn can_health(
        &mut self,
        index: usize,
        bus: u16,
    ) -> Result<Option<crate::health::CanHealth>, Error> {
        Ok(self.pandas[index].can_health(bus)?)
    }
    fn healthy(&self, index: usize) -> bool {
        self.pandas[index].transport().comms_healthy()
    }
    fn set_safety(&mut self, index: usize, model: u16) -> Result<(), Error> {
        Ok(self.pandas[index].control(Control::new(0xdc, model, 0))?)
    }
    fn set_power_saving(&mut self, index: usize, value: bool) -> Result<(), Error> {
        Ok(self.pandas[index].control(Control::new(0xe7, u16::from(value), 0))?)
    }
    fn publish(&mut self, states: &[state::Snapshot], valid: bool) -> Result<(), Error> {
        Ok(self.publisher.send(
            "pandaStates",
            &state_wire::encode(states, valid, self.publication_ns)?,
        )?)
    }
    fn list_usb(&mut self) -> Result<Vec<Vec<u8>>, Error> {
        Ok(self.factory.list(true)?)
    }
    fn heartbeat(&mut self, index: usize, engaged: bool) -> Result<(), Error> {
        Ok(self.pandas[index].control(Control::new(0xf3, u16::from(engaged), 0))?)
    }
    fn request_exit(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
    fn diagnostic(&mut self, event: state::Diagnostic<'_>) -> Result<(), Error> {
        match event {
            state::Diagnostic::Checksum {
                index,
                serial,
                total,
                delta,
                baseline,
                reset,
            } => {
                let suffix =
                    reset.map_or_else(String::new, |value| format!(", reset={}", u8::from(value)));
                self.logs.named(&format!("panda[{index}]"), Level::Warning, format!("SPI checksum: serial={}, total={total}, delta={delta}, baseline={}{suffix}",
                    String::from_utf8_lossy(serial), u8::from(baseline)));
            }
            state::Diagnostic::HealthUnavailable => {
                self.logs.write(Level::Error, "Failed to get ignition_opt")
            }
            state::Diagnostic::UnhealthyReconnect => self.logs.write(
                Level::Error,
                "Reconnecting, communication to pandas not healthy",
            ),
            state::Diagnostic::NewPanda { serial } => self.logs.write(
                Level::Warning,
                format!(
                    "Reconnecting to new panda: {}",
                    String::from_utf8_lossy(serial)
                ),
            ),
        }
        Ok(())
    }
}

impl safety::Effects for Effects<'_> {
    type Error = Error;
    fn panda_count(&self) -> usize {
        self.pandas.len()
    }
    fn boolean(&mut self, key: &str) -> Result<bool, Error> {
        Ok(self.params.get_bool(key)?)
    }
    fn bytes(&mut self, key: &str) -> Result<Vec<u8>, Error> {
        Ok(self.params.get(key)?.unwrap_or_default())
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Error> {
        Ok(self.params.put_bool(key, value)?)
    }
    fn set_safety(&mut self, index: usize, model: u16, parameter: u16) -> Result<(), Error> {
        Ok(self.pandas[index].control(Control::new(0xdc, model, parameter))?)
    }
    fn set_alternative(&mut self, index: usize, experience: u16) -> Result<(), Error> {
        Ok(self.pandas[index].control(Control::new(0xdf, experience, 0))?)
    }
    fn warning(&mut self, message: &str) -> Result<(), Error> {
        self.logs.write(Level::Warning, message);
        Ok(())
    }
}

pub struct PeripheralEffects<'a> {
    pub panda: &'a Panda,
    pub params: &'a Params,
    pub hardware: &'a Hardware,
    pub error: Option<Error>,
}

impl PeripheralEffects<'_> {
    fn control(&mut self, command: Control) {
        if self.error.is_none() {
            if let Err(error) = self.panda.control(command) {
                self.error = Some(error.into());
            }
        }
    }
}

impl peripheral::Output for PeripheralEffects<'_> {
    fn driver_view_enabled(&mut self) -> bool {
        match self.params.get_bool("IsDriverViewEnabled") {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error.into());
                false
            }
        }
    }
    fn set_fan_speed(&mut self, speed: u16) {
        self.control(Control::new(0xb1, speed, 0));
    }
    fn set_panda_ir_power(&mut self, power: u16) {
        self.control(Control::new(0xb0, power, 0));
    }
    fn set_hardware_ir_power(&mut self, power: i32) {
        if self.error.is_none() {
            self.hardware.ir_power(power);
        }
    }
}
