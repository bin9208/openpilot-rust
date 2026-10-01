use crate::{estimator::Estimator, Error};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unestimated,
    Estimated,
    Invalid,
}
#[derive(Debug, serde::Serialize)]
pub struct Packet {
    pub delay: f64,
    pub estimate: f64,
    pub std: f64,
    pub valid_blocks: i32,
    pub percent: i8,
    pub status: Status,
    pub points: Option<Vec<f64>>,
}
pub fn packet(estimator: &Estimator, debug: bool) -> Result<Packet, Error> {
    let values = estimator.blocks.statistics()?;
    let valid_blocks = estimator.blocks.valid_blocks;
    let minimum = i32::try_from(estimator.settings.min_valid_block_count)
        .map_err(|_| Error::Contract("required block count"))?;
    let status =
        if valid_blocks >= minimum && !values.valid_mean.is_nan() && !values.valid_std.is_nan() {
            if values.valid_std > 0.1 {
                Status::Invalid
            } else {
                Status::Estimated
            }
        } else {
            Status::Unestimated
        };
    let delay = match status {
        Status::Estimated => values.valid_mean.clamp(0.15, 1.),
        Status::Unestimated | Status::Invalid => estimator.initial_lag,
    };
    let (estimate, std) = if !values.current_mean.is_nan() && !values.current_std.is_nan() {
        (values.current_mean, values.current_std)
    } else {
        (estimator.initial_lag, 0.)
    };
    let size = i64::try_from(estimator.settings.block_size)
        .map_err(|_| Error::Contract("block size conversion"))?;
    let denominator = i64::from(minimum)
        .checked_mul(size)
        .filter(|value| *value > 0)
        .ok_or(Error::Contract("progress denominator"))?;
    let count = i64::from(valid_blocks) * size
        + i64::try_from(estimator.blocks.idx).map_err(|_| Error::Contract("block position"))?;
    let percent = i8::try_from(
        count
            .checked_mul(100)
            .ok_or(Error::Contract("progress overflow"))?
            .div_euclid(denominator)
            .min(100),
    )
    .map_err(|_| Error::Contract("progress exceeds int8"))?;
    Ok(Packet {
        delay,
        estimate,
        std,
        valid_blocks,
        percent,
        status,
        points: debug.then(|| estimator.blocks.values.clone()),
    })
}
