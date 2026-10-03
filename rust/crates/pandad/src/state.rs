use crate::health::{CanHealth, Health};
use openpilot_cereal::car_capnp::car_params::SafetyModel;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identity {
    pub hardware_type: u8,
    pub serial: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Input {
    pub onroad: bool,
    pub engaged: bool,
    pub spoofing_started: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub identity: Identity,
    pub health: Health,
    pub can: [CanHealth; 3],
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Diagnostic<'a> {
    Checksum {
        index: usize,
        serial: &'a [u8],
        total: u16,
        delta: u32,
        baseline: bool,
        reset: Option<bool>,
    },
    HealthUnavailable,
    UnhealthyReconnect,
    NewPanda {
        serial: &'a [u8],
    },
}

pub trait Effects {
    type Error;
    fn identities(&self) -> &[Identity];
    fn health(&mut self, index: usize) -> Result<Option<Health>, Self::Error>;
    fn can_health(&mut self, index: usize, bus: u16) -> Result<Option<CanHealth>, Self::Error>;
    fn healthy(&self, index: usize) -> bool;
    fn set_safety(&mut self, index: usize, model: u16) -> Result<(), Self::Error>;
    fn set_power_saving(&mut self, index: usize, value: bool) -> Result<(), Self::Error>;
    fn publish(&mut self, states: &[Snapshot], valid: bool) -> Result<(), Self::Error>;
    fn list_usb(&mut self) -> Result<Vec<Vec<u8>>, Self::Error>;
    fn heartbeat(&mut self, index: usize, engaged: bool) -> Result<(), Self::Error>;
    fn request_exit(&mut self);
    fn diagnostic(&mut self, event: Diagnostic<'_>) -> Result<(), Self::Error>;
}

#[derive(Default)]
pub struct Publisher {
    checksums: HashMap<Vec<u8>, u16>,
}

impl Publisher {
    pub fn update<E: Effects>(
        &mut self,
        input: Input,
        effects: &mut E,
    ) -> Result<Option<bool>, E::Error> {
        let identities = effects.identities().to_vec();
        let red_panda_c3 = identities.len() == 2
            && identities[0].hardware_type == 6
            && identities[1].hardware_type == 7;
        let mut snapshots = Vec::with_capacity(identities.len());
        let mut ignition = false;
        for (index, identity) in identities.iter().enumerate() {
            let Some(mut health) = effects.health(index)? else {
                effects.diagnostic(Diagnostic::HealthUnavailable)?;
                return Ok(None);
            };
            let total = health.spi_checksum_errors;
            match self.checksums.entry(identity.serial.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(total);
                    effects.diagnostic(Diagnostic::Checksum {
                        index,
                        serial: &identity.serial,
                        total,
                        delta: 0,
                        baseline: true,
                        reset: None,
                    })?;
                }
                Entry::Occupied(mut entry) => {
                    if *entry.get() != total {
                        let reset = total < *entry.get();
                        let delta = if reset { total } else { total - *entry.get() };
                        effects.diagnostic(Diagnostic::Checksum {
                            index,
                            serial: &identity.serial,
                            total,
                            delta: u32::from(delta),
                            baseline: false,
                            reset: Some(reset),
                        })?;
                        entry.insert(total);
                    }
                }
            }
            let mut can = [CanHealth::default(); 3];
            for (bus, state) in (0_u16..3).zip(can.iter_mut()) {
                let Some(value) = effects.can_health(index, bus)? else {
                    effects.diagnostic(Diagnostic::HealthUnavailable)?;
                    return Ok(None);
                };
                *state = value;
            }
            if input.spoofing_started {
                health.ignition_line = 1;
            }
            if red_panda_c3 && identity.hardware_type == 6 {
                health.ignition_line = 0;
            }
            ignition |= health.ignition_line != 0 || health.ignition_can != 0;
            snapshots.push(Snapshot {
                identity: identity.clone(),
                health,
                can,
            });
        }

        let mut valid = true;
        for (index, snapshot) in snapshots.iter().enumerate() {
            let health = snapshot.health;
            if u16::from(health.safety_model) == u16::from(SafetyModel::Silent) {
                effects.set_safety(index, u16::from(SafetyModel::NoOutput))?;
            }
            if health.power_save != u8::from(!ignition) {
                effects.set_power_saving(index, !ignition)?;
            }
            if (!ignition || !input.onroad)
                && u16::from(health.safety_model) != u16::from(SafetyModel::NoOutput)
            {
                effects.set_safety(index, u16::from(SafetyModel::NoOutput))?;
            }
            valid &= effects.healthy(index);
        }
        effects.publish(&snapshots, valid)?;

        if !ignition {
            let mut healthy = true;
            for index in 0..identities.len() {
                healthy &= effects.healthy(index);
            }
            if !healthy {
                effects.diagnostic(Diagnostic::UnhealthyReconnect)?;
                effects.request_exit();
            } else if !input.onroad {
                for serial in effects.list_usb()? {
                    if !identities.iter().any(|identity| identity.serial == serial) {
                        effects.diagnostic(Diagnostic::NewPanda { serial: &serial })?;
                        effects.request_exit();
                        break;
                    }
                }
            }
        }
        for index in 0..identities.len() {
            effects.heartbeat(index, input.engaged)?;
        }
        Ok(Some(ignition))
    }
}
