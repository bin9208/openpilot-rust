use crate::{
    correlation::{next_good_size, Correlator},
    Error,
};
use num_traits::ToPrimitive;
#[derive(Clone, Debug, serde::Serialize)]
pub struct Estimate {
    pub delay: f64,
    pub correlation: f64,
    pub confidence: f64,
    pub peak: usize,
    pub run: usize,
    pub width: usize,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}
pub struct Delay {
    count: usize,
    dt: f64,
    minimum: f64,
    low: usize,
    high: usize,
    second: usize,
    correlation: Correlator,
}
impl Delay {
    pub fn new(count: usize, dt: f64, bounds: (f64, f64)) -> Result<Self, Error> {
        let low = (bounds.0 / dt)
            .round_ties_even()
            .to_usize()
            .ok_or(Error::Contract("minimum lag samples"))?;
        let high = (bounds.1 / dt)
            .round_ties_even()
            .to_usize()
            .ok_or(Error::Contract("maximum lag samples"))?;
        let second = (1. / dt)
            .round_ties_even()
            .to_usize()
            .ok_or(Error::Contract("one-second samples"))?;
        let n = next_good_size(
            count
                .checked_add(high.max(second))
                .ok_or(Error::Contract("padded FFT size"))?,
        )?;
        Ok(Self {
            count,
            dt,
            minimum: bounds.0,
            low,
            high,
            second,
            correlation: Correlator::new(n)?,
        })
    }
    pub fn estimate(
        &mut self,
        expected: &[f64],
        actual: &[f64],
        mask: &[bool],
    ) -> Result<Estimate, Error> {
        if expected.len() != self.count || self.count == 0 {
            return Err(Error::Contract("delay input length"));
        }
        let ncc = self.correlation.masked(expected, actual, mask)?;
        let origin = self.count - 1;
        let roi = ncc
            .get(origin + self.low..(origin + self.high).min(ncc.len()))
            .filter(|value| !value.is_empty())
            .ok_or(Error::Contract("empty lag ROI"))?;
        let threshold_roi = ncc
            .get(origin..(origin + self.second).min(ncc.len()))
            .filter(|value| !value.is_empty())
            .ok_or(Error::Contract("empty threshold ROI"))?;
        let start = if origin >= 5 {
            origin - 5
        } else {
            ncc.len().saturating_sub(5 - origin)
        };
        let confidence_roi = ncc
            .get(start..(origin + self.second + 5).min(ncc.len()))
            .ok_or(Error::Contract("confidence ROI"))?;
        let peak = roi.iter().enumerate().fold(
            0,
            |best, (index, value)| if *value > roi[best] { index } else { best },
        );
        let delay = parabolic(roi, peak)? * self.dt + self.minimum;
        let maximum = threshold_roi
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let minimum = threshold_roi.iter().copied().fold(f64::INFINITY, f64::min);
        let threshold = (maximum - minimum) * 0.9 + minimum;
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut previous = false;
        for (index, value) in confidence_roi.iter().enumerate() {
            let good = *value >= threshold;
            if good && !previous {
                starts.push(index);
            }
            if !good && previous {
                ends.push(index - 1);
            }
            previous = good;
        }
        if previous {
            ends.push(confidence_roi.len() - 1);
        }
        if starts.is_empty() {
            return Err(Error::Contract("no confidence candidate run"));
        }
        // Preserve the original search index: it has no minimum-lag sample offset.
        let run = starts
            .partition_point(|start| *start <= peak + 5)
            .checked_sub(1)
            .unwrap_or(starts.len() - 1);
        let width = ends[run] - starts[run] + 1;
        let confidence = (1. - width.to_f64().ok_or(Error::Contract("candidate width"))? * self.dt)
            .clamp(0., 1.);
        Ok(Estimate {
            delay,
            correlation: roi[peak],
            confidence,
            peak,
            run,
            width,
            starts,
            ends,
        })
    }
}
pub fn parabolic(values: &[f64], peak: usize) -> Result<f64, Error> {
    let center = *values.get(peak).ok_or(Error::Contract("peak index"))?;
    let index = peak.to_f64().ok_or(Error::Contract("peak conversion"))?;
    if peak == 0 || peak == values.len() - 1 {
        return Ok(index);
    }
    Ok(index
        + 0.5 * (values[peak + 1] - values[peak - 1])
            / (2. * center - values[peak + 1] - values[peak - 1]))
}
