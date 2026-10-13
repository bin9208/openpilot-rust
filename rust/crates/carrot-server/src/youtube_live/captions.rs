use crate::Error;

fn parity(value: u8) -> u8 {
    let value = value & 127;
    value
        | if value.count_ones().is_multiple_of(2) {
            128
        } else {
            0
        }
}
pub fn sei(pairs: &[(u8, u8)]) -> Result<Vec<u8>, Error> {
    if pairs.is_empty() || pairs.len() > 31 {
        return Err(Error::Source(
            "CEA-608 SEI requires 1 to 31 caption pairs".into(),
        ));
    }
    let count = u8::try_from(pairs.len()).unwrap_or(0);
    let mut t35 = b"\xb5\x00\x31GA94\x03".to_vec();
    t35.extend([64 | count, 255]);
    for &(first, second) in pairs {
        t35.extend([252, parity(first), parity(second)]);
    }
    t35.push(255);
    let mut body = vec![4, u8::try_from(t35.len()).unwrap_or(0)];
    body.extend(t35);
    body.push(128);
    let mut output = vec![0, 0, 0, 1, 6];
    let mut zeros = 0;
    for value in body {
        if zeros >= 2 && value <= 3 {
            output.push(3);
            zeros = 0;
        }
        output.push(value);
        zeros = if value == 0 { zeros + 1 } else { 0 };
    }
    Ok(output)
}
fn timestamp_pairs(text: &str) -> Vec<(u8, u8)> {
    let mut clean: Vec<_> = text
        .chars()
        .take(32)
        .map(|value| {
            if (' '..='~').contains(&value) {
                u8::try_from(u32::from(value)).unwrap_or(b'?')
            } else {
                b'?'
            }
        })
        .collect();
    if clean.len() % 2 != 0 {
        clean.push(b' ');
    }
    let mut pairs = vec![(20, 32), (20, 32), (20, 46), (20, 46), (17, 82), (17, 82)];
    pairs.extend(clean.chunks_exact(2).map(|pair| (pair[0], pair[1])));
    pairs.push((20, 47));
    pairs
}
#[derive(Default)]
pub struct Injector {
    enabled: bool,
    last_text: String,
    pub packets: usize,
}
impl Injector {
    pub fn reset(&mut self) {
        self.enabled = false;
        self.last_text.clear();
    }
    pub fn inject(&mut self, payload: &[u8], enabled: bool, text: &str) -> Result<Vec<u8>, Error> {
        if payload.is_empty() {
            return Ok(Vec::new());
        }
        let pairs = if enabled != self.enabled {
            self.enabled = enabled;
            self.last_text.clear();
            if !enabled {
                Some(vec![(20, 44), (20, 44)])
            } else {
                None
            }
        } else {
            None
        };
        let pairs = if enabled && text != self.last_text {
            self.last_text = text.into();
            Some(timestamp_pairs(text))
        } else {
            pairs
        };
        let Some(pairs) = pairs else {
            return Ok(payload.to_vec());
        };
        self.packets += 1;
        let mut output = sei(&pairs)?;
        output.extend_from_slice(payload);
        Ok(output)
    }
}
