use crate::{number, Error, STATS_SOCKET};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    Dropped,
}
/// Keep the source caller's numeric kind on the metric wire.
#[derive(Clone, Copy, Debug)]
pub enum MetricValue {
    Integer(i128),
    Float(f64),
}
impl From<f64> for MetricValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}
impl From<f32> for MetricValue {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}
macro_rules! integer_values {
    ($($kind:ty),*) => { $(impl From<$kind> for MetricValue {
        fn from(value: $kind) -> Self { Self::Integer(i128::from(value)) }
    })* };
}
integer_values!(i8, u8, i16, u16, i32, u32, i64, u64, i128);
impl MetricValue {
    fn text(self) -> Result<String, Error> {
        match self {
            Self::Integer(value) => Ok(value.to_string()),
            Self::Float(value) => number::repr(value),
        }
    }
}
struct Connection {
    socket: zmq::Socket,
    pid: u32,
}
/// One producer per application thread; reconnects after fork before touching inherited handles.
pub struct StatLog {
    endpoint: String,
    connection: Option<Connection>,
}
impl Default for StatLog {
    fn default() -> Self {
        Self::new(STATS_SOCKET.into())
    }
}
impl StatLog {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            connection: None,
        }
    }
    pub fn gauge(&mut self, name: &str, value: impl Into<MetricValue>) -> Result<Delivery, Error> {
        self.send(&format!("{name}:{}|g", value.into().text()?))
    }
    pub fn sample(&mut self, name: &str, value: impl Into<MetricValue>) -> Result<Delivery, Error> {
        self.send(&format!("{name}:{}|sa", value.into().text()?))
    }
    pub fn send(&mut self, metric: &str) -> Result<Delivery, Error> {
        let pid = std::process::id();
        if self
            .connection
            .as_ref()
            .is_none_or(|connection| connection.pid != pid)
        {
            self.release();
            let context = zmq::Context::new();
            let socket = context.socket(zmq::PUSH)?;
            socket.set_linger(10)?;
            socket.connect(&self.endpoint)?;
            self.connection = Some(Connection { socket, pid });
        }
        let connection = self
            .connection
            .as_ref()
            .ok_or(Error::Configuration("stats connection missing"))?;
        loop {
            match connection.socket.send(metric, zmq::DONTWAIT) {
                Ok(()) => return Ok(Delivery::Sent),
                Err(zmq::Error::EINTR) => continue,
                Err(zmq::Error::EAGAIN) => return Ok(Delivery::Dropped),
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn release(&mut self) {
        if let Some(connection) = self.connection.take() {
            if connection.pid != std::process::id() {
                // pyzmq likewise skips close/term for handles inherited from another PID.
                std::mem::forget(connection);
            }
        }
    }
}
impl Drop for StatLog {
    fn drop(&mut self) {
        self.release();
    }
}
