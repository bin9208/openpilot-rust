//! Road-worker capture gates and bounded cadence, independent of inference.
use crate::pixels::Rgb;
use openpilot_opencv_runtime as cv;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Capture {
    Duplicate,
    Stale,
    Process { gap: Option<i64>, reset: bool },
}
#[derive(Debug, Default)]
pub struct CapturePolicy {
    previous: Option<u32>,
}
impl CapturePolicy {
    /// Source retains the previous ID across reconnect/timeout tracker resets.
    pub fn receive(&mut self, frame_id: u32, eof_ns: u64, now_ns: u64) -> Capture {
        if self.previous == Some(frame_id) {
            return Capture::Duplicate;
        }
        let gap = self
            .previous
            .map(|last| i64::from(frame_id) - i64::from(last));
        self.previous = Some(frame_id);
        if now_ns < eof_ns || now_ns - eof_ns > 150_000_000 {
            return Capture::Stale;
        }
        Capture::Process {
            gap,
            reset: gap.is_some_and(|g| g <= 0),
        }
    }
}
pub fn next_delay(wall: f64, cpu: f64) -> f64 {
    0_f64.max(0.05 - wall).max(2. * cpu - wall)
}
pub fn result_fresh(eof_ns: u64, now_ns: u64) -> bool {
    now_ns >= eof_ns && now_ns - eof_ns <= 200_000_000
}
/// Borrow padded planes for conversion and return owned RGB. Caller rechecks
/// the shared frame ID afterwards, retaining the source reuse window.
pub fn copy_nv12_rgb(
    raw: &[u8],
    width: u32,
    height: u32,
    stride: usize,
    uv_offset: usize,
) -> Result<Rgb, cv::Error> {
    if (width, height) != (1344, 760)
        || stride < width as usize
        || !stride.is_multiple_of(2)
        || uv_offset
            < stride
                .checked_mul(height as usize)
                .ok_or(cv::Error::Contract("camera extent overflow"))?
    {
        return Err(cv::Error::Contract("unsupported tracking camera geometry"));
    }
    let end = uv_offset
        .checked_add(
            stride
                .checked_mul(height as usize / 2)
                .ok_or(cv::Error::Contract("camera extent overflow"))?,
        )
        .ok_or(cv::Error::Contract("camera extent overflow"))?;
    if raw.len() < end {
        return Err(cv::Error::Contract("truncated tracking camera buffer"));
    }
    let image =
        cv::nv12_planes_to_rgb(raw, cv::Dimensions::new(width, height)?, stride, uv_offset)?;
    Ok(Rgb {
        width,
        height,
        data: image.into_data(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_identity_and_staleness_are_separate() {
        let mut p = CapturePolicy::default();
        assert_eq!(
            p.receive(20, 1_000_000_000, 1_150_000_000),
            Capture::Process {
                gap: None,
                reset: false
            }
        );
        assert_eq!(p.receive(20, 0, 9_000_000_000), Capture::Duplicate);
        assert_eq!(p.receive(21, 1, 200_000_001), Capture::Stale);
        assert_eq!(
            p.receive(19, 1_000_000_000, 1_000_000_000),
            Capture::Process {
                gap: Some(-2),
                reset: true
            }
        );
        assert!(!result_fresh(10, 9));
        assert!(result_fresh(10, 200_000_010));
        assert_eq!(next_delay(0.01, 0.07), 0.13);
    }
    #[test]
    fn cadence_counts_time_already_spent_preempted_and_never_catches_up() {
        for (wall, cpu, expected) in [
            (0.080, 0.030, 0.0),
            (0.050, 0.040, 0.030),
            (0.010, 0.005, 0.040),
            (0.200, 0.200, 0.200),
        ] {
            assert!((next_delay(wall, cpu) - expected).abs() < 1e-12);
            assert!(wall + next_delay(wall, cpu) + 1e-12 >= 0.05);
            assert!(wall + next_delay(wall, cpu) + 1e-12 >= 2. * cpu);
        }
    }
}
