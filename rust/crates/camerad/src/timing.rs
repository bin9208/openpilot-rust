use serde::Serialize;

#[derive(Debug, Default)]
pub struct CameraEventTiming {
    last_sof: u64,
    last_report: u64,
    suppressed: u64,
}

#[derive(Debug, Serialize)]
pub struct TimingSample {
    pub sof_delta_ns: u64,
    pub event_age_ns: u64,
    pub suppressed: u64,
    pub report: bool,
}

impl CameraEventTiming {
    pub fn observe(&mut self, sof: u64, received: u64) -> TimingSample {
        let delta = if self.last_sof != 0 {
            sof.saturating_sub(self.last_sof)
        } else {
            0
        };
        let age = received.saturating_sub(sof);
        self.last_sof = sof;
        let anomalous = delta > 75_000_000 || age > 75_000_000;
        let report = anomalous
            && (self.last_report == 0 || received.wrapping_sub(self.last_report) >= 1_000_000_000);
        let sample = TimingSample {
            sof_delta_ns: delta,
            event_age_ns: age,
            suppressed: self.suppressed,
            report,
        };
        if report {
            self.last_report = received;
            self.suppressed = 0;
        } else if anomalous {
            self.suppressed = self.suppressed.wrapping_add(1);
        }
        sample
    }
}
