use std::{ffi::CStr, io, path::Path, time::Duration};

pub fn now_ns() -> u64 {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    value.tv_sec as u64 * 1_000_000_000 + value.tv_nsec as u64
}

pub fn seconds() -> f64 {
    let value = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    value.tv_sec as f64 + value.tv_nsec as f64 * 1e-9
}

pub fn thread_name(name: &CStr) -> io::Result<()> {
    rustix::thread::set_name(name).map_err(io::Error::from)
}

pub fn realtime(priority: i32) -> io::Result<()> {
    openpilot_panda_spi_linux::set_scheduler(libc::SCHED_FIFO, priority)
}

pub fn core_three() -> io::Result<()> {
    let mut set = rustix::thread::CpuSet::new();
    set.set(3);
    rustix::thread::sched_setaffinity(None, &set).map_err(io::Error::from)
}

pub struct Hardware {
    pub board: bool,
    pub name: String,
}

impl Hardware {
    pub fn detect() -> io::Result<Self> {
        if !Path::new("/TICI").is_file() {
            return Ok(Self {
                board: false,
                name: "pc".into(),
            });
        }
        let model = std::fs::read_to_string("/sys/firmware/devicetree/base/model")?;
        let name = model
            .get(6..)
            .ok_or_else(|| io::Error::other("invalid board model"))?
            .trim_matches(|value: char| value == '\0' || value.is_ascii_whitespace())
            .to_owned();
        if !matches!(name.as_str(), "tici" | "tizi" | "mici") {
            return Err(io::Error::other(format!("unknown board model: {name}")));
        }
        Ok(Self { board: true, name })
    }

    pub fn ir_power(&self, percent: i32) {
        if self.name == "mici" {
            let value = percent.clamp(0, 100) * 3;
            for (path, value) in [
                ("/sys/class/leds/led:switch_2/brightness", 0),
                ("/sys/class/leds/led:torch_2/brightness", value),
                ("/sys/class/leds/led:switch_2/brightness", value),
            ] {
                // The original C++ ofstream writes are best effort and do not alter the control loop.
                match std::fs::write(path, format!("{value}\n")) {
                    Ok(()) | Err(_) => {}
                }
            }
        }
    }

    pub fn power(&self) -> (u32, u32) {
        if !self.board {
            return (0, 0);
        }
        fn read(path: &str) -> u32 {
            let text = std::fs::read(path).unwrap_or_default();
            let text = String::from_utf8_lossy(&text);
            let text = text.trim_start_matches(|value: char| value.is_ascii_whitespace());
            let mut end = 0;
            for (index, byte) in text.bytes().enumerate() {
                if byte.is_ascii_digit() || (index == 0 && matches!(byte, b'+' | b'-')) {
                    end = index + 1;
                } else {
                    break;
                }
            }
            text[..end].parse::<i32>().unwrap_or(0) as u32
        }
        (
            read("/sys/class/hwmon/hwmon1/in1_input"),
            read("/sys/class/hwmon/hwmon1/curr1_input"),
        )
    }
}

pub struct RateKeeper {
    pub frame: u64,
    next: f64,
    interval: f64,
}

impl RateKeeper {
    pub fn new() -> Self {
        let interval = f64::from(1.0_f32 / 100.0);
        Self {
            frame: 0,
            next: seconds() + interval,
            interval,
        }
    }

    pub fn keep_time(&mut self) -> io::Result<()> {
        self.frame = self.frame.wrapping_add(1);
        let now = seconds();
        let remaining = self.next - now;
        self.next = if remaining < 0.0 {
            now + self.interval
        } else {
            self.next + self.interval
        };
        if remaining > 0.0 {
            let delay = Duration::try_from_secs_f64(remaining).map_err(io::Error::other)?;
            std::thread::sleep(delay);
        }
        Ok(())
    }
}
