//! The source catches an unknown-type log failure inside its metric parse boundary.
use crate::{aggregation::MetricEvent, Error};
pub fn report(
    event: MetricEvent<'_>,
    metric: &str,
    mut emit: impl FnMut(&str, (&str, &str)) -> Result<(), Error>,
) -> Result<(), Error> {
    match event {
        MetricEvent::Accepted => Ok(()),
        MetricEvent::Malformed(metric) => emit("malformed metric", ("metric", metric)),
        MetricEvent::Unknown(kind) => match emit("unknown metric type", ("metric_type", kind)) {
            Ok(()) => Ok(()),
            Err(_) => emit("malformed metric", ("metric", metric)),
        },
    }
}
