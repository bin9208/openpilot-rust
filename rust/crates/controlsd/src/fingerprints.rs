use crate::{config::Config, parameters::Parameters, Error};
use std::collections::BTreeMap;

pub fn validate(text: &str, config: &Config, params: &mut impl Parameters) -> Result<(), Error> {
    let mut parser = Parser {
        chars: text.as_bytes(),
        index: 0,
    };
    let buses = parser.dictionary()?;
    parser.space();
    if parser.index != parser.chars.len() || !buses.contains_key(&0) {
        return Err(Error::Contract("invalid FingerPrints"));
    }
    let mut powertrain = config.bus_offset;
    for index in 0..3 {
        let swapped = config.flags & 1 != 0 && params.integer("HyundaiCameraSCC")? == 0;
        if index == 1 && swapped {
            powertrain += 1;
        }
    }
    if !buses.contains_key(&powertrain) {
        return Err(Error::Contract("missing FingerPrints powertrain bus"));
    }
    Ok(())
}
struct Parser<'a> {
    chars: &'a [u8],
    index: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self
            .chars
            .get(self.index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.index += 1;
        }
    }
    fn take(&mut self, value: u8) -> bool {
        self.space();
        if self.chars.get(self.index) == Some(&value) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, value: u8) -> Result<(), Error> {
        if self.take(value) {
            Ok(())
        } else {
            Err(Error::Contract("invalid FingerPrints syntax"))
        }
    }
    fn integer(&mut self) -> Result<i32, Error> {
        self.space();
        let start = self.index;
        if matches!(self.chars.get(self.index), Some(b'+' | b'-')) {
            self.index += 1;
        }
        while self.chars.get(self.index).is_some_and(|c| {
            c.is_ascii_hexdigit() || matches!(c, b'x' | b'X' | b'o' | b'O' | b'b' | b'B' | b'_')
        }) {
            self.index += 1;
        }
        let value = std::str::from_utf8(&self.chars[start..self.index])
            .map_err(|_| Error::Contract("FingerPrints integer"))?
            .replace('_', "");
        let (sign, unsigned) = if let Some(s) = value.strip_prefix('-') {
            (-1, s)
        } else {
            (1, value.strip_prefix('+').unwrap_or(&value))
        };
        let (radix, unsigned) = if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
            (16, &unsigned[2..])
        } else if unsigned.starts_with("0o") || unsigned.starts_with("0O") {
            (8, &unsigned[2..])
        } else if unsigned.starts_with("0b") || unsigned.starts_with("0B") {
            (2, &unsigned[2..])
        } else {
            (10, unsigned)
        };
        i32::from_str_radix(unsigned, radix)
            .ok()
            .and_then(|v| v.checked_mul(sign))
            .ok_or(Error::Contract("FingerPrints integer"))
    }
    fn dictionary(&mut self) -> Result<BTreeMap<i32, BTreeMap<i32, i32>>, Error> {
        self.expect(b'{')?;
        let mut result = BTreeMap::new();
        while !self.take(b'}') {
            let bus = self.integer()?;
            self.expect(b':')?;
            self.expect(b'{')?;
            let mut messages = BTreeMap::new();
            while !self.take(b'}') {
                let address = self.integer()?;
                self.expect(b':')?;
                messages.insert(address, self.integer()?);
                if !self.take(b',') {
                    self.expect(b'}')?;
                    break;
                }
            }
            result.insert(bus, messages);
            if !self.take(b',') {
                self.expect(b'}')?;
                break;
            }
        }
        Ok(result)
    }
}
