use crate::{databases::Databases, integer_set::IntegerSet, Error};
use openpilot_can::{parser::Parser, Packet};

pub struct Reader {
    pub parser: Parser,
    pub bus: i64,
}

impl Reader {
    pub fn new(
        databases: &mut Databases,
        name: &str,
        messages: impl IntoIterator<Item = (u32, f64)>,
        bus: i64,
        clock: &mut impl FnMut() -> u64,
        emit: &mut impl FnMut(&str),
    ) -> Result<Self, Error> {
        let database = databases.get(name, emit)?;
        let mut parser = Parser::new(database, u8::try_from(bus).unwrap_or(0), clock());
        for (address, frequency) in messages {
            parser.add_address(address, Some(frequency), false, clock())?;
        }
        Ok(Self { parser, bus })
    }

    pub fn update(
        &mut self,
        packets: &[Packet],
        accumulated: &mut IntegerSet,
    ) -> Result<(), Error> {
        if u8::try_from(self.bus).is_ok() {
            self.parser.update(packets)?;
        } else {
            let without_matching_bus = packets
                .iter()
                .map(|packet| Packet {
                    mono_time: packet.mono_time,
                    frames: Vec::new(),
                })
                .collect::<Vec<_>>();
            self.parser.update(&without_matching_bus)?;
        }
        accumulated.merge(&IntegerSet::from_arrivals(
            self.parser.successful_addresses().iter().copied(),
        ));
        Ok(())
    }

    pub fn signal(&mut self, address: u32, signal: &str) -> Result<f64, Error> {
        if !self.parser.states.contains_key(&address) {
            self.parser
                .add_address(address, None, false, self.parser.last_update)?;
        }
        let message = self
            .parser
            .dbc
            .messages
            .get(&address)
            .ok_or_else(|| openpilot_can::Error::Message(address.to_string()))?;
        let index = message
            .signals
            .iter()
            .position(|s| s.name == signal)
            .ok_or_else(|| openpilot_can::Error::Signal(signal.to_owned()))?;
        Ok(*self
            .parser
            .states
            .get(&address)
            .and_then(|s| s.values.get(index))
            .ok_or(Error::Contract("registered radar signal value absent"))?)
    }
}
