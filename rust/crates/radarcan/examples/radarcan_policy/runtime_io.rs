use super::{decoder, decoder_settings};
use openpilot_can::Packet;
use openpilot_radarcan::{
    batch::Ego,
    data::Data,
    databases::Databases,
    decoder::{hyundai::Environment, Config, Interface},
    numerics::Numerics,
    runtime::{Io, Reason},
    wire, Error,
};
use serde_json::{json, Value};

pub struct Capture<'a> {
    pub now: u64,
    pub processing: u64,
    pub flip: bool,
    pub stdout: &'a mut String,
    pub databases: &'a mut Databases,
    pub numerics: &'a mut Numerics,
    pub settings: decoder_settings::Observed,
    pub params: Vec<Value>,
    pub states: Vec<Value>,
    pub warnings: Vec<String>,
    pub publications: Vec<Vec<u8>>,
    pub errors: Vec<String>,
}

impl Io for Capture<'_> {
    fn monotonic_ns(&mut self) -> u64 {
        self.now
    }
    fn create_interface(&mut self, config: &Config) -> Result<Interface, Error> {
        let mut state = Interface::with_settings(
            config.clone(),
            &mut Environment {
                databases: self.databases,
                clock: &mut || self.now,
                emit: &mut |line: &str| self.stdout.push_str(line),
                settings: &mut self.settings,
            },
        )?;
        let (snapshot, warnings) = decoder::snapshot(&mut state)?;
        self.states.push(snapshot);
        self.warnings.extend(warnings);
        Ok(state)
    }
    fn track_flip(&mut self) -> Result<bool, Error> {
        self.params.push(json!({"key":"RadarTrackFlip"}));
        Ok(self.flip)
    }
    fn update(
        &mut self,
        state: &mut Interface,
        ego: Ego,
        packets: &[Packet],
    ) -> Result<Option<Data>, Error> {
        let result = state.update_carrot(
            ego.v_ego,
            ego.a_ego,
            ego.receive_ns as f64 * 1e-9,
            packets,
            self.numerics,
            &mut |line| self.stdout.push_str(line),
        );
        let (snapshot, warnings) = decoder::snapshot(state)?;
        *self
            .states
            .last_mut()
            .ok_or(Error::Contract("fixture interface absent"))? = snapshot;
        self.warnings.extend(warnings);
        if result.is_ok() {
            self.now = self
                .now
                .checked_add(self.processing)
                .ok_or(Error::IntegerOverflow)?;
        }
        result
    }
    fn publish(&mut self, data: Data, valid: bool) -> Result<(), Error> {
        let stamp = ((self.now as f64 * 1e-9) * 1e9) as u64;
        self.publications.push(wire::encode(&data, valid, stamp)?);
        Ok(())
    }
    fn input_error(&mut self, reason: Reason) -> Result<(), Error> {
        self.errors
            .push(format!("radarcan input invalid: {reason}"));
        Ok(())
    }
}
