use crate::{
    decoder::Decoder,
    types::{Profile, Token},
};

impl Decoder {
    pub(crate) fn touch_token(&self) -> Option<Token> {
        let track = self.track.as_ref()?;
        let dx = i64::from(track.last.x) - i64::from(track.start.x);
        let dy = i64::from(track.last.y) - i64::from(track.start.y);
        if dx.abs().max(dy.abs()) >= 120 {
            let (axis, delta) = if dx.abs() > dy.abs() {
                ('x', dx)
            } else {
                ('y', dy)
            };
            let positive = delta > 0;
            return Some(Token(match self.profile {
                Profile::Generic => format!("swipe:{axis}{}", if positive { '+' } else { '-' }),
                Profile::YiserJ6 => match (axis, positive) {
                    ('x', true) => "left",
                    ('x', false) => "right",
                    (_, true) => "up",
                    (_, false) => "down",
                }
                .to_owned(),
            }));
        }
        match self.profile {
            Profile::YiserJ6 => {
                let x = i64::from(track.last.x);
                let y = i64::from(track.last.y);
                if (x - 300).abs() <= 65 && (y - 500).abs() <= 65 {
                    Some(Token("center".to_owned()))
                } else if (x - 420).abs() <= 65 && (y - 850).abs() <= 65 {
                    Some(Token("2".to_owned()))
                } else {
                    None
                }
            }
            Profile::Generic => Some(Token(format!(
                "tap:{}:{}",
                rounded(track.last.x),
                rounded(track.last.y)
            ))),
        }
    }
}

fn rounded(value: i32) -> i64 {
    let value = i64::from(value);
    let quotient = value.div_euclid(25);
    (quotient + i64::from(value.rem_euclid(25) > 12)) * 25
}
