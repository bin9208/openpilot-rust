#[derive(Default)]
pub struct Framer {
    pub buffer: Vec<u8>,
    pub last_log_time: f64,
}

pub fn checksum(data: &[u8]) -> [u8; 2] {
    let mut a = 0u8;
    let mut b = 0u8;
    for value in data {
        a = a.wrapping_add(*value);
        b = b.wrapping_add(a);
    }
    [a, b]
}
impl Framer {
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
    pub fn add_data(&mut self, time: f64, incoming: &[u8]) -> Vec<Vec<u8>> {
        self.last_log_time = time;
        let mut frames = Vec::new();
        if incoming.is_empty() {
            return frames;
        }
        self.buffer.extend_from_slice(incoming);
        loop {
            if self.buffer.len() < 2 {
                break;
            }
            let Some(start) = self
                .buffer
                .windows(2)
                .position(|bytes| bytes == [0xb5, 0x62])
            else {
                self.buffer.clear();
                break;
            };
            self.buffer.drain(..start);
            if self.buffer.len() < 6 {
                break;
            }
            let size = usize::from(u16::from_le_bytes([self.buffer[4], self.buffer[5]])) + 8;
            if self.buffer.len() < size {
                break;
            }
            if checksum(&self.buffer[2..size - 2]) == self.buffer[size - 2..size] {
                frames.push(self.buffer.drain(..size).collect());
            } else {
                self.buffer.remove(0);
            }
        }
        frames
    }
}
