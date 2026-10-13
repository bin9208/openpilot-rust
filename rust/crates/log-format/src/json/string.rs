use super::{parse::Parser, Error, Text};
impl Parser<'_> {
    fn hex(&mut self) -> Result<u32, Error> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = self
                .peek()
                .and_then(|byte| char::from(byte).to_digit(16))
                .ok_or_else(|| self.error("invalid Unicode escape"))?;
            value = value * 16 + digit;
            self.offset += 1;
        }
        Ok(value)
    }
    pub(super) fn text(&mut self) -> Result<Text, Error> {
        if self.peek() != Some(b'"') {
            return Err(self.error("expected string"));
        }
        self.offset += 1;
        let mut output = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(Text(output));
                }
                Some(b'\\') => {
                    self.offset += 1;
                    let code = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.offset += 1;
                    let value = match code {
                        b'"' | b'\\' | b'/' => u32::from(code),
                        b'b' => 8,
                        b'f' => 12,
                        b'n' => 10,
                        b'r' => 13,
                        b't' => 9,
                        b'u' => {
                            let high = self.hex()?;
                            if (0xd800..=0xdbff).contains(&high) && self.rest().starts_with("\\u") {
                                let saved = self.offset;
                                self.offset += 2;
                                let low = self.hex()?;
                                if (0xdc00..=0xdfff).contains(&low) {
                                    0x10000 + ((high - 0xd800) << 10) + low - 0xdc00
                                } else {
                                    self.offset = saved;
                                    high
                                }
                            } else {
                                high
                            }
                        }
                        _ => return Err(self.error("invalid escape")),
                    };
                    output.push(value);
                }
                Some(0..=31) => return Err(self.error("unescaped control character")),
                Some(_) => {
                    let character = self
                        .rest()
                        .chars()
                        .next()
                        .ok_or_else(|| self.error("missing character"))?;
                    self.offset += character.len_utf8();
                    output.push(u32::from(character));
                }
            }
        }
    }
}
