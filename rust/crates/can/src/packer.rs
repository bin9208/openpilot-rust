use crate::{dbc::Dbc, Diagnostic, Error};
use num_bigint::BigInt;
use num_traits::{FromPrimitive, One};
use std::{collections::BTreeMap, sync::Arc};

pub struct Packer {
    pub dbc: Arc<Dbc>,
    pub counters: BTreeMap<u32, BigInt>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Packer {
    pub fn new(dbc: Arc<Dbc>) -> Self {
        Self {
            dbc,
            counters: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn pack(
        &mut self,
        name: &str,
        values: &[(&str, f64)],
        rx_counter: Option<i64>,
    ) -> Result<Vec<u8>, Error> {
        let Some(address) = self.dbc.names.get(name) else {
            self.diagnostics.push(Diagnostic {
                address: 0,
                message: format!("msg not found for name_or_addr='{name}'"),
            });
            return Ok(Vec::new());
        };
        self.pack_address(*address, values, rx_counter)
    }

    pub fn pack_address(
        &mut self,
        address: u32,
        values: &[(&str, f64)],
        rx_counter: Option<i64>,
    ) -> Result<Vec<u8>, Error> {
        let Some(message) = self.dbc.messages.get(&address) else {
            self.diagnostics.push(Diagnostic {
                address,
                message: format!("msg not found for address={address}"),
            });
            return Ok(Vec::new());
        };
        let mut data = vec![0; message.size];
        let mut counter_set = false;
        for (name, value) in values {
            let Some(signal) = message.signals.iter().find(|s| s.name == *name) else {
                self.diagnostics.push(Diagnostic {
                    address,
                    message: format!("unknown signal name='{name}' in {}", message.name),
                });
                continue;
            };
            if signal.factor == 0. {
                return Err(Error::Numeric);
            }
            let mut raw = BigInt::from_f64(((value - signal.offset) / signal.factor + 0.5).floor())
                .ok_or(Error::Numeric)?;
            if raw < BigInt::from(0) {
                raw += BigInt::one() << signal.size;
            }
            signal.set(&mut data, &raw)?;
            if signal.counter() {
                self.counters.insert(
                    address,
                    BigInt::from_f64(value.trunc()).ok_or(Error::Numeric)?,
                );
                counter_set = true;
            }
        }
        if let Some(counter) = message.signals.iter().find(|s| s.counter()) {
            if !counter_set {
                let modulus = BigInt::one() << counter.size;
                let value = self.counters.entry(address).or_insert_with(|| {
                    rx_counter.map_or_else(
                        || BigInt::from(0),
                        |c| modulo(BigInt::from(c) + 1, &modulus),
                    )
                });
                counter.set(&mut data, value)?;
                *value = modulo(&*value + 1, &modulus);
            }
        }
        if let Some(checksum) = message.signals.iter().find(|s| s.kind.is_checksum()) {
            let value = checksum.kind.compute(address, checksum, &mut data)?;
            checksum.set(&mut data, &BigInt::from(value))?;
        }
        Ok(data)
    }
}

fn modulo(value: BigInt, modulus: &BigInt) -> BigInt {
    let remainder = value % modulus;
    if remainder < BigInt::from(0) {
        remainder + modulus
    } else {
        remainder
    }
}
