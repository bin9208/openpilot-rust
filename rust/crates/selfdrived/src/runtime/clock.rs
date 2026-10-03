use serde::Serialize;

#[derive(Serialize)]
pub struct Ratekeeper {
    pub frame: u64,
    pub remaining: f64,
    pub last_monitor_time: f64,
    pub next_frame_time: f64,
    buffer: Vec<f64>,
    index: usize,
    count: usize,
    sum: f64,
}
impl Default for Ratekeeper {
    fn default() -> Self {
        let mut buffer = vec![0.0; 100];
        buffer[0] = 0.01;
        Self {
            frame: 0,
            remaining: 0.0,
            last_monitor_time: -1.0,
            next_frame_time: -1.0,
            buffer,
            index: 1,
            count: 1,
            sum: 0.01,
        }
    }
}
impl Ratekeeper {
    pub fn lagging(&self) -> bool {
        self.sum / self.count as f64 > 0.01 * (1.0 / 0.9)
    }
    pub fn monitor_time(&mut self, mut now: impl FnMut() -> f64) -> bool {
        if self.last_monitor_time < 0.0 {
            self.next_frame_time = now() + 0.01;
            self.last_monitor_time = now();
        }
        let previous = self.last_monitor_time;
        self.last_monitor_time = now();
        self.sum -= self.buffer[self.index];
        self.buffer[self.index] = self.last_monitor_time - previous;
        self.sum += self.buffer[self.index];
        self.index = (self.index + 1) % 100;
        self.count = (self.count + 1).min(100);
        self.remaining = self.next_frame_time - now();
        self.next_frame_time += 0.01;
        self.frame += 1;
        false
    }
}
