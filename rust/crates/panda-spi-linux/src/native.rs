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
impl Clock for BootClock {
    fn now_ns(&self) -> u64 {
        ffi::now_ns()
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
        fs,
        sync::{Arc, Mutex},
    };
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
