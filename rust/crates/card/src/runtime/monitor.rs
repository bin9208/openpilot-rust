#[derive(Default)]
pub struct Monitor {
    last: Option<f64>,
    next: f64,
    pub remaining: f64,
    pub frames: u64,
}
impl Monitor {
    pub fn monitor(&mut self, mut clock: impl FnMut() -> f64) {
        if self.last.is_none() {
            self.next = clock() + 0.01;
            self.last = Some(clock());
        }
        self.last = Some(clock());
        self.remaining = self.next - clock();
        self.next += 0.01;
        self.frames += 1;
    }
}
