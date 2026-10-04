use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize_float<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_nan() {
        serializer.serialize_str("NaN")
    } else if *value == f64::INFINITY {
        serializer.serialize_str("Infinity")
    } else if *value == f64::NEG_INFINITY {
        serializer.serialize_str("-Infinity")
    } else {
        serializer.serialize_f64(*value)
    }
}

pub fn deserialize_float<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Float(f64),
        Text(String),
    }
    match Number::deserialize(deserializer)? {
        Number::Float(value) => Ok(value),
        Number::Text(value) => match value.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            _ => Err(serde::de::Error::custom("invalid floating-point text")),
        },
    }
}

pub fn bits<'a>(
    values: impl IntoIterator<Item = (&'a str, f64)>,
) -> std::collections::BTreeMap<&'a str, String> {
    values
        .into_iter()
        .map(|(name, value)| {
            (
                name,
                value
                    .to_le_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect(),
            )
        })
        .collect()
}

pub fn serialize_history<S: Serializer>(
    values: &std::collections::VecDeque<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    struct Float(f64);
    impl Serialize for Float {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serialize_float(&self.0, serializer)
        }
    }
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        sequence.serialize_element(&Float(*value))?;
    }
    sequence.end()
}

pub fn minimum(first: f64, second: f64) -> f64 {
    if second < first {
        second
    } else {
        first
    }
}

pub fn maximum(first: f64, second: f64) -> f64 {
    if second > first {
        second
    } else {
        first
    }
}

pub fn clip(value: f64, lower: f64, upper: f64) -> f64 {
    if value < lower {
        lower
    } else if value > upper {
        upper
    } else {
        value
    }
}

pub fn divide(numerator: f64, denominator: f64) -> Result<f64, Error> {
    if denominator == 0. {
        Err(Error::DivisionByZero)
    } else {
        Ok(numerator / denominator)
    }
}

pub fn square(value: f64) -> Result<f64, Error> {
    // Python float **2 invokes libm pow. The opaque exponent prevents replacing
    // that call with multiplication, whose rounding can differ by one ULP.
    let result = value.powf(std::hint::black_box(2.));
    if value.is_finite() && result.is_infinite() {
        Err(Error::PowerOverflow)
    } else {
        Ok(result)
    }
}

pub fn float_sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut total = 0_f64;
    let mut correction = 0_f64;
    for value in values {
        let next = total + value;
        correction += if total.abs() >= value.abs() {
            (total - next) + value
        } else {
            (value - next) + total
        };
        total = next;
    }
    if correction != 0. && correction.is_finite() {
        total + correction
    } else {
        total
    }
}

#[derive(Clone, Serialize)]
pub struct FirstOrder {
    #[serde(serialize_with = "serialize_float")]
    pub x: f64,
    #[serde(serialize_with = "serialize_float")]
    pub dt: f64,
    #[serde(serialize_with = "serialize_float")]
    pub alpha: f64,
    pub initialized: bool,
    #[serde(skip)]
    numpy_period: bool,
}

impl FirstOrder {
    pub fn new(x: f64, rc: f64, dt: f64) -> Result<Self, Error> {
        Self::new_for_period(x, rc, dt, false)
    }

    pub fn new_for_period(x: f64, rc: f64, dt: f64, numpy_period: bool) -> Result<Self, Error> {
        Ok(Self {
            x,
            dt,
            alpha: if numpy_period {
                dt / (rc + dt)
            } else {
                divide(dt, rc + dt)?
            },
            initialized: true,
            numpy_period,
        })
    }
    pub fn update_alpha(&mut self, rc: f64) -> Result<(), Error> {
        self.alpha = if self.numpy_period {
            self.dt / (rc + self.dt)
        } else {
            divide(self.dt, rc + self.dt)?
        };
        Ok(())
    }
    pub fn update(&mut self, value: f64) -> f64 {
        if self.initialized {
            self.x = (1. - self.alpha) * self.x + self.alpha * value;
        } else {
            self.initialized = true;
            self.x = value;
        }
        self.x
    }
}
