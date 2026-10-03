//! Safe Linux syscall policy; the native UAPI implementation is supplied separately.
use crate::{
    device::Transport,
    protocol::{Clock, Io, BUFFER_SIZE},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    Mode,
    Speed,
    Bits,
}
#[derive(Clone, Copy, Default)]
pub struct CallResult {
    pub result: i32,
    pub errno: i32,
    pub fd: i32,
    pub request: u64,
    pub argument_address: u64,
}
/// Each syscall is synchronous. The kernel implementation must not retain borrowed slices.
pub trait Kernel: Send {
    type Clock: Clock;
    fn clock(&self) -> Self::Clock;
    fn exists(&mut self) -> bool;
    fn open(&mut self) -> i32;
    fn configure(&mut self, option: OptionKind, value: u32) -> CallResult;
    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> CallResult;
    fn flock(&mut self, exclusive: bool);
    fn close(&mut self);
    fn now_ns(&mut self) -> u64;
    fn yield_now(&mut self);
    fn sleep_us(&mut self, micros: u32);
    fn log(&mut self, level: u8, message: String);
    fn errno_description(&self, errno: i32) -> String;
    fn error_probability(&mut self) -> Result<f64, String>;
    /// Linux libc rand(), with RAND_MAX == 2147483647.
    fn random(&mut self) -> u32;
    fn diagnostic_print(&mut self, message: &str);
}
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SetupError(pub String);
pub struct LinuxIo<K: Kernel> {
    kernel: K,
    probability: f64,
    open: bool,
}
impl<K: Kernel> LinuxIo<K> {
    pub fn open(kernel: K) -> Result<Self, SetupError> {
        let mut io = Self {
            kernel,
            probability: -1.0,
            open: false,
        };
        if !io.kernel.exists() {
            return Err(SetupError(
                "Error connecting to panda: SPI device not found".into(),
            ));
        }
        let fd = io.kernel.open();
        if fd < 0 {
            io.kernel.log(40, format!("failed opening SPI device {fd}"));
            return Err(SetupError(
                "Error connecting to panda: failed to open SPI device".into(),
            ));
        }
        io.open = true;
        for (option, value, message) in [
            (OptionKind::Mode, 0, "failed setting SPI mode"),
            (OptionKind::Speed, 50_000_000, "failed setting SPI speed"),
            (OptionKind::Bits, 8, "failed setting SPI bits per word"),
        ] {
            let call = loop {
                let call = io.kernel.configure(option, value);
                if call.result != -1 || call.errno != 4 {
                    break call;
                }
            };
            if call.result == -1 {
                io.kernel.log(
                    40,
                    format!(
                        "safe_ioctl error: {message} {}({}) (fd: {} request: {:x} argp: 0x{:x})",
                        io.kernel.errno_description(call.errno),
                        call.errno,
                        call.fd,
                        call.request,
                        call.argument_address
                    ),
                );
                return Err(SetupError(message.into()));
            }
        }
        io.probability = io.kernel.error_probability().map_err(SetupError)?;
        Ok(io)
    }
    fn random_fraction(&mut self) -> f64 {
        f64::from(self.kernel.random()) / 2_147_483_647.0
    }
    fn corrupt(&mut self, data: &mut [u8]) {
        for byte in data {
            if self.random_fraction() > 0.9 {
                *byte = (self.kernel.random() % 256) as u8;
            }
        }
    }
}
impl<K: Kernel> Io for LinuxIo<K> {
    fn now_ns(&mut self) -> u64 {
        self.kernel.now_ns()
    }
    fn transfer(&mut self, tx: &mut [u8], rx: &mut [u8], mut length: usize) -> i32 {
        if self.probability > 0.0 {
            if self.random_fraction() < self.probability {
                self.kernel.diagnostic_print("transfer len error\n");
                length = self.kernel.random() as usize % BUFFER_SIZE;
            }
            // Source injection may overrun the response sub-buffer. Keep injected errors memory-safe.
            if length > tx.len() || length > rx.len() {
                return -1;
            }
            if self.random_fraction() < self.probability {
                self.kernel.diagnostic_print("corrupting TX\n");
                self.corrupt(&mut tx[..length]);
            }
        }
        let (Some(tx), Some(rx)) = (tx.get(..length), rx.get_mut(..length)) else {
            return -1;
        };
        let result = loop {
            let call = self.kernel.transfer(tx, rx);
            if call.result != -1 || call.errno != 4 {
                break call.result;
            }
        };
        if self.probability > 0.0 && self.random_fraction() < self.probability {
            self.kernel.diagnostic_print("corrupting RX\n");
            self.corrupt(rx);
        }
        result
    }
    fn log(&mut self, level: u8, message: String) {
        self.kernel.log(level, message);
    }
}
impl<K: Kernel> Transport for LinuxIo<K> {
    type Clock = K::Clock;
    fn clock(&self) -> Self::Clock {
        self.kernel.clock()
    }
    fn file_lock(&mut self) {
        self.kernel.flock(true);
    }
    fn file_unlock(&mut self) {
        self.kernel.flock(false);
    }
    fn yield_now(&mut self) {
        self.kernel.yield_now();
    }
    fn sleep_us(&mut self, micros: u32) {
        self.kernel.sleep_us(micros);
    }
    fn close(&mut self) {
        if self.open {
            self.kernel.close();
            self.open = false;
        }
    }
}
impl<K: Kernel> Drop for LinuxIo<K> {
    fn drop(&mut self) {
        self.close();
    }
}
