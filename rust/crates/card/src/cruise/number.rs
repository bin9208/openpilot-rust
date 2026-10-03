use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Clone, Copy, Debug)]
pub(super) struct Number {
    pub value: f64,
    integer: bool,
}
impl Number {
    pub const fn int(value: f64) -> Self {
        Self {
            value,
            integer: true,
        }
    }
    pub const fn float(value: f64) -> Self {
        Self {
            value,
            integer: false,
        }
    }
    pub fn clip(self, low: f64, high: f64) -> Self {
        Self {
            value: self.value.max(low).min(high),
            ..self
        }
    }
    pub fn max(self, other: Self) -> Self {
        if other.value > self.value {
            other
        } else {
            self
        }
    }
    pub fn min(self, other: Self) -> Self {
        if other.value < self.value {
            other
        } else {
            self
        }
    }
    pub fn sub_int(self, value: f64) -> Self {
        Self {
            value: self.value - value,
            ..self
        }
    }
    pub fn with_value(self, value: f64) -> Self {
        Self { value, ..self }
    }
}
impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.integer {
            write!(f, "{:.0}", self.value)
        } else {
            write!(f, "{:?}", self.value)
        }
    }
}
impl Serialize for Number {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f64(self.value)
    }
}
