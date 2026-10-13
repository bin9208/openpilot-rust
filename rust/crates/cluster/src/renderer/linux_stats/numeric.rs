use super::super::{Error, Result};
use num_bigint::{BigInt, BigUint};
use num_traits::{One, ToPrimitive, Zero};

pub(super) fn ratio(numerator: &BigInt, denominator: &BigInt) -> Result<f64> {
    if denominator.is_zero() {
        return Err(Error::Contract("division by zero"));
    }
    if numerator.is_zero() {
        return Ok(0.0);
    }
    let negative = numerator.sign() != denominator.sign();
    let numerator = numerator.magnitude();
    let denominator = denominator.magnitude();
    let mut exponent = i64::try_from(numerator.bits())
        .map_err(|_| Error::Contract("integer too large"))?
        - i64::try_from(denominator.bits()).map_err(|_| Error::Contract("integer too large"))?;
    let below = if exponent >= 0 {
        numerator < &(denominator << shift(exponent)?)
    } else {
        &(numerator << shift(-exponent)?) < denominator
    };
    if below {
        exponent -= 1;
    }
    let sign = u64::from(negative) << 63;
    if exponent < -1075 {
        return Ok(f64::from_bits(sign));
    }
    if exponent > 1023 {
        return Err(Error::Contract(
            "integer division result too large for a float",
        ));
    }
    let scale = if exponent < -1022 {
        1074
    } else {
        52 - exponent
    };
    let (scaled, divisor) = if scale >= 0 {
        (numerator << shift(scale)?, denominator.clone())
    } else {
        (numerator.clone(), denominator << shift(-scale)?)
    };
    let mut quotient = &scaled / &divisor;
    let remainder = &scaled % &divisor;
    let doubled = remainder << 1_usize;
    if doubled > divisor || (doubled == divisor && quotient.bit(0)) {
        quotient += BigUint::one();
    }
    let mut significand = quotient
        .to_u64()
        .ok_or(Error::Contract("float significand out of range"))?;
    if exponent < -1022 {
        return Ok(f64::from_bits(sign | significand));
    }
    if significand == 1_u64 << 53 {
        significand >>= 1;
        exponent += 1;
    }
    if exponent > 1023 {
        return Err(Error::Contract(
            "integer division result too large for a float",
        ));
    }
    let biased = u64::try_from(exponent + 1023)
        .map_err(|_| Error::Contract("float exponent out of range"))?;
    Ok(f64::from_bits(
        sign | (biased << 52) | (significand - (1_u64 << 52)),
    ))
}
fn shift(value: i64) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::Contract("integer shift out of range"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryBytes(BigInt);
impl MemoryBytes {
    #[must_use]
    pub fn from_integer(value: BigInt) -> Self {
        Self(value)
    }
    pub(in crate::renderer) fn gibibytes(&self) -> Result<f64> {
        let value = self
            .0
            .to_f64()
            .filter(|value| value.is_finite())
            .ok_or(Error::Contract("int too large to convert to float"))?;
        Ok(value / 1024.0_f64.powi(3))
    }
}
impl serde::Serialize for MemoryBytes {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        if let Some(value) = self.0.to_i64() {
            return serializer.serialize_i64(value);
        }
        if let Some(value) = self.0.to_u64() {
            return serializer.serialize_u64(value);
        }
        let raw = serde_json::value::RawValue::from_string(self.0.to_string())
            .map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for MemoryBytes {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        openpilot_carrot_navi::json::Value::text(raw.get())
            .int()
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
