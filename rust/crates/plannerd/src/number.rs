use crate::Error;
use num_traits::ToPrimitive;

pub fn wire_float(value: f64) -> Result<f64, Error> {
    Ok(f64::from(
        value
            .to_f32()
            .ok_or(Error::Contract("cereal float32 conversion"))?,
    ))
}

pub fn timestamp_seconds(nanoseconds: u64) -> Result<f64, Error> {
    Ok(nanoseconds
        .to_f64()
        .ok_or(Error::Contract("timestamp conversion"))?
        * 1e-9)
}

pub fn age_seconds(newer: u64, older: u64) -> Result<f64, Error> {
    Ok((i128::from(newer) - i128::from(older))
        .to_f64()
        .ok_or(Error::Contract("timestamp difference conversion"))?
        * 1e-9)
}

pub fn float32(value: f64) -> Result<f32, Error> {
    value
        .to_f32()
        .ok_or(Error::Contract("cereal float32 conversion"))
}
