use super::Error;
use num_traits::ToPrimitive;

#[derive(Default)]
pub struct Diagnostics {
    previous_receive: Option<u64>,
    pub receive: u64,
    frames: u64,
    loop_max: i128,
    process_max: i128,
    slow_loop: u64,
    slow_process: u64,
    timeouts: u64,
    pub current: [i128; 10],
    sums: [i128; 10],
    maxima: [i128; 10],
}
impl Diagnostics {
    pub(super) fn receive(&mut self, now: u64, empty: bool) {
        if let Some(previous) = self.previous_receive {
            let interval = (i128::from(now) - i128::from(previous)).div_euclid(1000);
            self.loop_max = self.loop_max.max(interval);
            self.slow_loop += u64::from(interval > 12000);
        }
        self.previous_receive = (now != 0).then_some(now);
        self.receive = now;
        self.timeouts += u64::from(empty);
        self.current = [0; 10];
    }
    pub(super) fn stage(&mut self, index: usize, start: u64, end: u64) {
        self.current[index] = (i128::from(end) - i128::from(start)).div_euclid(1000);
    }
    pub(super) fn applied(&mut self) -> Vec<String> {
        self.frames += 1;
        for index in 0..10 {
            self.sums[index] += self.current[index];
            self.maxima[index] = self.maxima[index].max(self.current[index]);
        }
        self.process_max = self.process_max.max(self.current[9]);
        self.slow_process += u64::from(self.current[9] > 5000);
        if self.frames < 100 {
            return vec![];
        }
        let mut lines = Vec::new();
        if self.timeouts > 0 || self.loop_max >= 50000 || self.process_max >= 20000 {
            lines.push(format!("card_sendcan_diag: can_timeouts={}, loop_max_us={}, process_max_us={}, loop_over_12ms={}, process_over_5ms={}",self.timeouts,self.loop_max,self.process_max,self.slow_loop,self.slow_process));
            for (label, indices, names) in [
                (
                    "card_stage_state_diag",
                    vec![0, 1, 2, 3, 4, 5],
                    vec!["decode", "ci", "sm", "vision", "tail", "state"],
                ),
                (
                    "card_stage_send_diag",
                    vec![6, 7, 8, 9],
                    vec!["publish", "apply", "sendcan", "total"],
                ),
            ] {
                let mut values = Vec::new();
                for (index, name) in indices.into_iter().zip(names) {
                    values.push(format!(
                        "{name}_avg_us={}, {name}_max_us={}",
                        self.sums[index].div_euclid(i128::from(self.frames)),
                        self.maxima[index]
                    ));
                }
                lines.push(format!("{label}: {}", values.join(", ")));
            }
        }
        self.frames = 0;
        self.loop_max = 0;
        self.process_max = 0;
        self.slow_loop = 0;
        self.slow_process = 0;
        self.timeouts = 0;
        self.sums = [0; 10];
        self.maxima = [0; 10];
        lines
    }
    pub(super) fn values(
        &self,
        end: u64,
        cpu_start: u64,
        cpu_end: u64,
    ) -> Result<Vec<(&'static str, f64)>, Error> {
        let mut values = vec![
            (
                "work_ms",
                (i128::from(end) - i128::from(self.receive))
                    .to_f64()
                    .ok_or(Error::Numeric)?
                    / 1e6,
            ),
            (
                "thread_cpu_ms",
                (i128::from(cpu_end) - i128::from(cpu_start))
                    .to_f64()
                    .ok_or(Error::Numeric)?
                    / 1e6,
            ),
        ];
        for (name, index) in [
            ("decode_ms", 0),
            ("ci_update_ms", 1),
            ("sm_update_ms", 2),
            ("vision_ms", 3),
            ("state_tail_ms", 4),
            ("state_total_ms", 5),
            ("publish_ms", 6),
            ("apply_ms", 7),
            ("sendcan_ms", 8),
            ("total_ms", 9),
        ] {
            values.push((
                name,
                self.current[index].to_f64().ok_or(Error::Numeric)? / 1000.,
            ));
        }
        Ok(values)
    }
}
