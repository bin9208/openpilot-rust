use crate::{
    geometry::{MouseEvent, Point},
    Error,
};
use std::io::BufRead;
#[derive(Default)]
pub struct Mouse {
    previous: [Option<MouseEvent>; 2],
}
impl Mouse {
    pub fn sample(
        &mut self,
        slot: u8,
        position: Point,
        down: bool,
        time: f64,
    ) -> Option<MouseEvent> {
        let index = usize::from(slot);
        let previous = self.previous.get(index).copied().flatten();
        let prior_down = previous.is_some_and(|event| event.down);
        let position = if down {
            position
        } else {
            previous.map_or(Point::default(), |event| event.pos)
        };
        let event = MouseEvent {
            pos: position,
            slot,
            pressed: down && !prior_down,
            released: prior_down && !down,
            down,
            time,
        };
        let changed = previous.is_none_or(|prior| {
            prior.pos.x != event.pos.x
                || prior.pos.y != event.pos.y
                || prior.pressed != event.pressed
                || prior.released != event.released
                || prior.down != event.down
        });
        if changed {
            if let Some(target) = self.previous.get_mut(index) {
                *target = Some(event);
            }
            Some(event)
        } else {
            None
        }
    }
}
pub fn read_stdin(reader: &mut impl BufRead) -> Result<Vec<String>, Error> {
    let mut lines = Vec::new();
    loop {
        let descriptor = rustix::stdio::stdin();
        let mut poll = [rustix::event::PollFd::new(
            &descriptor,
            rustix::event::PollFlags::IN,
        )];
        let ready = rustix::event::poll(
            &mut poll,
            Some(&rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            }),
        )
        .map_err(std::io::Error::from)?;
        if ready == 0 {
            break;
        }
        let mut text = String::new();
        reader.read_line(&mut text)?;
        let line = crate::text::trim(&text);
        if line.is_empty() {
            break;
        }
        lines.push(line.into());
    }
    Ok(lines)
}

#[derive(Default)]
pub struct TouchSlots {
    active: [bool; 2],
    current: i32,
    saw_mt: bool,
}
impl TouchSlots {
    pub fn event(&mut self, kind: u16, code: u16, value: i32) -> Option<[bool; 2]> {
        match (kind, code) {
            (3, 0x2f) => self.current = value,
            (3, 0x39) => {
                self.saw_mt = true;
                if let Ok(slot) = usize::try_from(self.current) {
                    if let Some(active) = self.active.get_mut(slot) {
                        *active = value != -1;
                    }
                }
            }
            (1, 0x14a) if !self.saw_mt => self.active[0] = value != 0,
            (0, 0) => return Some(self.active),
            _ => {}
        }
        None
    }
}
#[cfg(feature = "native")]
pub struct BoardInput {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    events: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<MouseEvent>>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
#[cfg(feature = "native")]
impl BoardInput {
    pub fn start(scale: f32) -> Result<Self, Error> {
        use std::{
            collections::VecDeque,
            sync::{
                atomic::{AtomicBool, Ordering},
                Arc, Mutex,
            },
            time::{Duration, Instant},
        };
        let stop = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let worker_stop = stop.clone();
        let worker_events = events.clone();
        let worker = std::thread::Builder::new()
            .name("startup-touch".into())
            .spawn(move || {
                let fd = match rustix::fs::open(
                    "/dev/input/by-path/platform-894000.i2c-event",
                    rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK,
                    rustix::fs::Mode::empty(),
                ) {
                    Ok(fd) => Some(fd),
                    Err(error) => {
                        eprintln!("mouse: using raylib touch, can't open touch device: {error}");
                        None
                    }
                };
                let mut slots = TouchSlots::default();
                let mut mouse = Mouse::default();
                let start = Instant::now();
                let period = Duration::from_secs_f64(1.0 / 140.0);
                let mut next = start;
                while !worker_stop.load(Ordering::Relaxed) {
                    crate::bridge::ffi::poll_input();
                    let mut append = |active: Option<[bool; 2]>| {
                        for slot in 0..2u8 {
                            let sample = crate::bridge::ffi::sample_input(i32::from(slot));
                            let down = active.map_or(sample.down, |state| state[usize::from(slot)]);
                            if let Some(event) = mouse.sample(
                                slot,
                                Point {
                                    x: sample.x / scale,
                                    y: sample.y / scale,
                                },
                                down,
                                start.elapsed().as_secs_f64(),
                            ) {
                                let Ok(mut queue) = worker_events.lock() else {
                                    return;
                                };
                                if queue.len() == 140 {
                                    queue.pop_front();
                                }
                                queue.push_back(event);
                            }
                        }
                    };
                    if let Some(fd) = &fd {
                        let mut bytes = [0u8; 24 * 64];
                        if let Ok(count) = rustix::io::read(fd, &mut bytes) {
                            for event in bytes[..count].chunks_exact(24) {
                                let kind = u16::from_ne_bytes([event[16], event[17]]);
                                let code = u16::from_ne_bytes([event[18], event[19]]);
                                let value = i32::from_ne_bytes([
                                    event[20], event[21], event[22], event[23],
                                ]);
                                if let Some(active) = slots.event(kind, code, value) {
                                    append(Some(active));
                                }
                            }
                        }
                    } else {
                        append(None);
                    }
                    next += period;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    } else {
                        next = now;
                    }
                }
            })?;
        Ok(Self {
            stop,
            events,
            worker: Some(worker),
        })
    }
    pub fn drain(&self) -> Result<Vec<MouseEvent>, Error> {
        Ok(self
            .events
            .lock()
            .map_err(|_| Error::Contract("touch queue poisoned"))?
            .drain(..)
            .collect())
    }
}
#[cfg(feature = "native")]
impl Drop for BoardInput {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                eprintln!("startup touch worker panicked");
            }
        }
    }
}
