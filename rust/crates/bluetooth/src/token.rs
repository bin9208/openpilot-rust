use crate::{config::ConfigError, Token};

impl Token {
    pub(crate) fn parse_mapping(value: &str) -> Result<Self, ConfigError> {
        let base = match value.split_once('@') {
            Some((base, "double" | "long")) => base,
            Some(_) => return Err(ConfigError::Button),
            None => value,
        };
        match base {
            "up" | "down" | "left" | "right" | "center" | "1" | "2" => {}
            "swipe:x+" | "swipe:x-" | "swipe:y+" | "swipe:y-" => {}
            key if key.starts_with("key:") => {
                let digits = &key[4..];
                if !(1..=4).contains(&digits.len())
                    || !digits.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(ConfigError::Button);
                }
                if digits.parse::<u16>().map_err(|_| ConfigError::KeyCode)? > 767 {
                    return Err(ConfigError::KeyCode);
                }
            }
            tap if tap.starts_with("tap:") => {
                let Some((x, y)) = tap[4..].split_once(':') else {
                    return Err(ConfigError::Button);
                };
                if [x, y].iter().any(|part| {
                    !(1..=5).contains(&part.len())
                        || !part.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return Err(ConfigError::Button);
                }
            }
            _ => return Err(ConfigError::Button),
        }
        Ok(Self(value.to_owned()))
    }
}
