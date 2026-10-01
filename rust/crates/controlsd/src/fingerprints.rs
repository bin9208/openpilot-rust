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
        let sign = if self.take(b'-') {
            -1
        } else {
            self.take(b'+');
            1
        };
        self.space();
        let start = self.index;
        while self
            .chars
            .get(self.index)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.index += 1;
        }
        let token = &self.chars[start..self.index];
        let (radix, digits) = match token.get(..2) {
            Some(b"0x" | b"0X") => (16, &token[2..]),
            Some(b"0o" | b"0O") => (8, &token[2..]),
            Some(b"0b" | b"0B") => (2, &token[2..]),
            _ => (10, token),
        };
        let digit = |c: u8| char::from(c).is_digit(radix);
        if digits.is_empty()
            || !digits.last().is_some_and(|&last| digit(last))
            || digits.iter().enumerate().any(|(i, &c)| {
                if c == b'_' {
                    (i == 0 && radix == 10)
                        || (i > 0 && !digit(digits[i - 1]))
                        || !digits.get(i + 1).is_some_and(|&next| digit(next))
                } else {
                    !digit(c)
                }
            })
            || (radix == 10 && digits[0] == b'0' && digits.iter().any(|&c| c != b'0' && c != b'_'))
        {
            return Err(Error::Contract("FingerPrints integer syntax"));
        }
        let normalized: String = digits
            .iter()
            .filter(|&&c| c != b'_')
            .map(|&c| char::from(c))
            .collect();
        i64::from_str_radix(&normalized, radix)
            .ok()
            .and_then(|v| i32::try_from(v * sign).ok())
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
