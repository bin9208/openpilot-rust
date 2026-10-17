use crate::bridge::ffi;
use openpilot_panda_spi::linux_io::{CallResult, Kernel, OptionKind};
use openpilot_panda_spi::protocol::Clock;
use std::{os::unix::ffi::OsStrExt, sync::OnceLock};

type Logger = Box<dyn FnMut(u8, String) + Send>;
pub struct NativeKernel {
    handle: cxx::UniquePtr<ffi::Handle>,
    logger: Logger,
}
// SAFETY: category 8 FFI. Handle owns its path and descriptor, has no thread-local
// state or borrowed pointers, and is accessed only through &mut self for mutation.
// Linux spidev permits moving the owning descriptor between threads. No Sync impl.
#[allow(unsafe_code)]
unsafe impl Send for NativeKernel {}
impl NativeKernel {
    pub fn system(logger: impl FnMut(u8, String) + Send + 'static) -> Self {
        Self::at_path("/dev/spidev0.0", logger)
    }
    fn at_path(path: &str, logger: impl FnMut(u8, String) + Send + 'static) -> Self {
        cxx::let_cxx_string!(path = path);
        Self {
            handle: ffi::create(&path),
            logger: Box::new(logger),
        }
    }
    fn result(call: ffi::Call) -> CallResult {
        CallResult {
            result: call.result,
            errno: call.error_number,
            fd: call.fd,
            request: call.request,
            argument_address: call.argument_address,
        }
    }
}
pub struct BootClock;

fn wait_for_turnaround(
    mut now_ns: impl FnMut() -> u64,
    mut sleep: impl FnMut(std::time::Duration),
    start: u64,
    duration: u64,
) {
    // Retain a short precision tail, avoiding a scheduler round trip for waits
    // already close to their deadline. Recheck after waking, including EINTR or
    // scheduling delay; neither an early wake nor rounding may shorten the gap.
    let remaining = duration.saturating_sub(now_ns().wrapping_sub(start));
    if remaining > 100_000 {
        sleep(std::time::Duration::from_nanos(remaining - 50_000));
    }
    while now_ns().wrapping_sub(start) < duration {
        std::hint::spin_loop();
    }
}

impl Clock for BootClock {
    fn now_ns(&self) -> u64 {
        ffi::now_ns()
    }
    fn wait_until(&self, start: u64, duration: u64) {
        wait_for_turnaround(ffi::now_ns, std::thread::sleep, start, duration);
    }
}
impl Kernel for NativeKernel {
    type Clock = BootClock;
    fn clock(&self) -> Self::Clock {
        BootClock
    }
    fn exists(&mut self) -> bool {
        self.handle.exists()
    }
    fn open(&mut self) -> i32 {
        self.handle.pin_mut().open()
    }
    fn configure(&mut self, option: OptionKind, value: u32) -> CallResult {
        Self::result(self.handle.pin_mut().configure(
            match option {
                OptionKind::Mode => 0,
                OptionKind::Speed => 1,
                OptionKind::Bits => 2,
            },
            value,
        ))
    }
    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> CallResult {
        Self::result(self.handle.pin_mut().transfer(tx, rx))
    }
    fn flock(&mut self, exclusive: bool) {
        self.handle.pin_mut().flock(exclusive);
    }
    fn close(&mut self) {
        self.handle.pin_mut().close();
    }
    fn now_ns(&mut self) -> u64 {
        ffi::now_ns()
    }
    fn yield_now(&mut self) {
        ffi::yield_now();
    }
    fn sleep_us(&mut self, micros: u32) {
        ffi::sleep_us(micros);
    }
    fn log(&mut self, level: u8, message: String) {
        (self.logger)(level, message);
    }
    fn errno_description(&self, errno: i32) -> String {
        ffi::errno_description(errno).to_string_lossy().into_owned()
    }
    fn error_probability(&mut self) -> Result<f64, String> {
        static PROBABILITY: OnceLock<f64> = OnceLock::new();
        if let Some(value) = PROBABILITY.get() {
            return Ok(*value);
        }
        let value = std::env::var_os("SPI_ERR_PROB").unwrap_or_else(|| "-1".into());
        cxx::let_cxx_string!(value = value.as_os_str().as_bytes());
        let value = ffi::parse_probability(&value).map_err(|error| error.to_string())?;
        Ok(*PROBABILITY.get_or_init(|| value))
    }
    fn random(&mut self) -> u32 {
        ffi::random()
    }
    fn diagnostic_print(&mut self, message: &str) {
        ffi::diagnostic_print(message);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use openpilot_panda_spi::linux_io::LinuxIo;
    use std::{
        cell::Cell,
        fs,
        sync::{Arc, Mutex},
    };

    #[test]
    fn turnaround_releases_cpu_without_starting_the_next_phase_early() {
        for (start, elapsed, duration, sleep_percent) in [
            (1_000_000_u64, 0, 400_000, 100),
            (1_000_000, 0, 1_000_000, 100),
            (1_000_000, 200_000, 400_000, 100),
            (1_000_000, 0, 400_000, 50),  // interrupted sleep
            (1_000_000, 0, 400_000, 200), // late wakeup
            (u64::MAX - 200_000, 0, 400_000, 100),
        ] {
            let now = Cell::new(start.wrapping_add(elapsed));
            let reads = Cell::new(0);
            let sleeps = Cell::new(0);
            wait_for_turnaround(
                || {
                    reads.set(reads.get() + 1);
                    let value = now.get();
                    now.set(value.wrapping_add(1_000));
                    value
                },
                |duration| {
                    sleeps.set(sleeps.get() + 1);
                    now.set(
                        now.get()
                            .wrapping_add(duration.as_nanos() as u64 * sleep_percent / 100),
                    );
                },
                start,
                duration,
            );
            assert!(now.get().wrapping_sub(start) >= duration);
            assert_eq!(sleeps.get(), 1, "long waits must relinquish the CPU once");
            if sleep_percent == 100 {
                assert!(
                    reads.get() < 100,
                    "long waits must not poll the clock throughout"
                );
            }
        }
    }

    #[test]
    fn elapsed_and_short_turnarounds_do_not_add_a_sleep() {
        for elapsed in [350_000, 399_000, 400_000, 500_000] {
            let now = Cell::new(1_000_000 + elapsed);
            wait_for_turnaround(
                || {
                    let value = now.get();
                    now.set(value + 1_000);
                    value
                },
                |_| panic!("short or elapsed waits must not add scheduler latency"),
                1_000_000,
                400_000,
            );
            assert!(now.get() >= 1_400_000);
        }
    }

    #[test]
    fn hardware_clock_wait_respects_both_spi_minimum_gaps() {
        let clock = NativeKernel::system(|_, _| {}).clock();
        for duration in [400_000, 1_000_000] {
            let start = clock.now_ns();
            clock.wait_until(start, duration);
            assert!(clock.now_ns().wrapping_sub(start) >= duration);
        }
    }

    #[test]
    #[ignore = "host CPU/latency benchmark; run explicitly with --nocapture"]
    fn turnaround_cpu_benchmark() {
        let ticks_per_second = std::process::Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .unwrap();
        assert!(ticks_per_second.status.success());
        let hz: f64 = std::str::from_utf8(&ticks_per_second.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let cpu_ticks = || {
            let stat = fs::read_to_string("/proc/thread-self/stat").unwrap();
            let fields: Vec<_> = stat[stat.rfind(')').unwrap() + 2..]
                .split_whitespace()
                .collect();
            fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
        };
        for duration in [400_000, 1_000_000] {
            for sleeping in [false, true, true, false] {
                let before = cpu_ticks();
                let wall = std::time::Instant::now();
                let mut elapsed = Vec::with_capacity(2000);
                for _ in 0..2000 {
                    let start = BootClock.now_ns();
                    if sleeping {
                        BootClock.wait_until(start, duration);
                    } else {
                        while BootClock.now_ns().wrapping_sub(start) < duration {}
                    }
                    let ns = BootClock.now_ns().wrapping_sub(start);
                    assert!(ns >= duration, "the SPI phase must never start early");
                    elapsed.push(ns);
                }
                let wall = wall.elapsed().as_secs_f64();
                let cpu = (cpu_ticks() - before) as f64 / hz;
                elapsed.sort_unstable();
                println!(
                    "duration_ns={duration} sleeping={sleeping} iterations=2000 cpu_seconds={cpu:.3} wall_seconds={wall:.6} cpu_percent={:.2} p50_ns={} p99_ns={} max_ns={}",
                    100.0 * cpu / wall, elapsed[1000], elapsed[1980], elapsed[1999]
                );
            }
        }
    }
    #[test]
    fn owned_regular_file_rejects_spidev_options_and_releases_descriptors() {
        let path = std::env::temp_dir().join(format!("panda-spi-kernel-{}", std::process::id()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        drop(file);
        let initial = fs::read_dir("/proc/self/fd").unwrap().count();
        let logs = Arc::new(Mutex::new(Vec::new()));
        for _ in 0..100 {
            let record = Arc::clone(&logs);
            let kernel = NativeKernel::at_path(path.to_str().unwrap(), move |level, message| {
                record.lock().unwrap().push((level, message))
            });
            let error = match LinuxIo::open(kernel) {
                Ok(_) => panic!("regular file accepted as spidev"),
                Err(error) => error,
            };
            assert_eq!(error.0, "failed setting SPI mode");
        }
        assert_eq!(fs::read_dir("/proc/self/fd").unwrap().count(), initial);
        assert_eq!(logs.lock().unwrap().len(), 100);
        let mut kernel = NativeKernel::at_path(path.to_str().unwrap(), |_, _| {});
        assert!(kernel.open() >= 0);
        let mut output = [0xa5; 2048];
        let mismatch = kernel.transfer(&[1; 7], &mut output);
        assert_eq!((mismatch.result, mismatch.errno), (-1, 22));
        assert_eq!(output, [0xa5; 2048]);
        let call = kernel.transfer(&[1; 2048], &mut output);
        assert_eq!((call.result, call.errno), (-1, 25));
        assert_eq!(output, [0xa5; 2048]);
        kernel.close();
        kernel.close();
        assert_eq!(fs::read_dir("/proc/self/fd").unwrap().count(), initial);
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn debug_probability_uses_original_standard_library_parser() {
        for (text, expected) in [
            ("-1", -1.0),
            (" 0.125tail", 0.125),
            ("0x1p-1", 0.5),
            ("+1e-3", 0.001),
        ] {
            cxx::let_cxx_string!(value = text);
            assert_eq!(ffi::parse_probability(&value).unwrap(), expected);
        }
        for text in ["", "bad", "1e9999"] {
            cxx::let_cxx_string!(value = text);
            assert!(ffi::parse_probability(&value).is_err());
        }
    }
}
