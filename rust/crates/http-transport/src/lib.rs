//! Per-socket I/O deadlines shared by the Python-compatible HTTP callers.
use std::time::{Duration, Instant};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport},
};

pub fn socket_timeout_agent(config: ureq::config::Config, timeout: Duration) -> ureq::Agent {
    ureq::Agent::with_parts(
        config,
        SocketTimeoutConnector(timeout),
        DefaultResolver::default(),
    )
}

#[derive(Debug)]
struct SocketTimeoutConnector(Duration);
impl Connector for SocketTimeoutConnector {
    type Out = SocketTimeout;
    fn connect(
        &self,
        details: &ConnectionDetails<'_>,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(DefaultConnector::default()
            .connect(details, chained)?
            .map(|inner| SocketTimeout {
                inner,
                timeout: self.0,
            }))
    }
}
#[derive(Debug)]
struct SocketTimeout {
    inner: Box<dyn Transport>,
    timeout: Duration,
}
// requests(timeout=10) applies to socket I/O, not the complete response body.
// ureq's body-stage deadline would reject transfers that keep making progress.
impl Transport for SocketTimeout {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, _: NextTimeout) -> Result<(), ureq::Error> {
        let started = Instant::now();
        let result = self.inner.transmit_output(
            amount,
            NextTimeout {
                after: self.timeout.into(),
                reason: ureq::Timeout::SendBody,
            },
        );
        self.completed(started, ureq::Timeout::SendBody, result)
    }
    fn await_input(&mut self, _: NextTimeout) -> Result<bool, ureq::Error> {
        let started = Instant::now();
        loop {
            let remaining = self.timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(ureq::Timeout::RecvBody));
            }
            let result = self.inner.await_input(NextTimeout {
                after: remaining.into(),
                reason: ureq::Timeout::RecvBody,
            });
            match self.completed(started, ureq::Timeout::RecvBody, result) {
                Err(ureq::Error::Io(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
                result => return result,
            }
        }
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

impl SocketTimeout {
    fn completed<T>(
        &self,
        started: Instant,
        reason: ureq::Timeout,
        result: Result<T, ureq::Error>,
    ) -> Result<T, ureq::Error> {
        // Kernel socket deadlines can round up; late bytes must not mark an upload successful.
        if started.elapsed() >= self.timeout {
            Err(ureq::Error::Timeout(reason))
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests;
