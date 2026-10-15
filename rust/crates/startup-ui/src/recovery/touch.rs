use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
#[derive(Default)]
struct Tracker {
    slot: i32,
    down: bool,
    previous: bool,
    saw_mt: bool,
}
impl Tracker {
    fn read(&mut self, bytes: &[u8]) -> bool {
        let mut pressed = false;
        // Linux timeval + type/code/value has the same 24-byte native ABI on
        // the supported x86_64 and aarch64 targets; incomplete records discard.
        for event in bytes.chunks_exact(24) {
            let kind = u16::from_ne_bytes(event[16..18].try_into().expect("event type"));
            let code = u16::from_ne_bytes(event[18..20].try_into().expect("event code"));
            let value = i32::from_ne_bytes(event[20..24].try_into().expect("event value"));
            match (kind, code) {
                (3, 0x2f) => self.slot = value,
                (3, 0x39) => {
                    self.saw_mt = true;
                    if self.slot == 0 {
                        self.down = value != -1;
                    }
                }
                (1, 0x14a) if !self.saw_mt => self.down = value != 0,
                (0, 0) => {
                    pressed |= self.down && !self.previous;
                    self.previous = self.down;
                }
                _ => {}
            }
        }
        pressed
    }
}
pub struct Touch {
    file: Option<File>,
    tracker: Tracker,
}
impl Touch {
    pub fn open(path: &Path) -> Self {
        Self {
            file: OpenOptions::new()
                .read(true)
                .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
                .open(path)
                .ok(),
            tracker: Tracker::default(),
        }
    }
    pub fn raw(&self) -> bool {
        self.file.is_some()
    }
    pub fn pressed(&mut self, mouse_pressed: bool) -> bool {
        let Some(file) = &mut self.file else {
            return mouse_pressed;
        };
        let mut bytes = [0; 24 * 128];
        match file.read(&mut bytes) {
            Ok(n) => self.tracker.read(&bytes[..n]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => false,
            Err(_) => {
                self.file = None;
                false
            }
        }
    }
}
#[cfg(test)]
mod tests;
