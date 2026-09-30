//! One connection budget spans DNS, TCP and every TLS handshake operation.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use ureq::{
    config::Config,
    http::Uri,
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::{
            Buffers, ConnectionDetails, Connector, NextTimeout, RustlsConnector, TcpConnector,
            Transport,
        },
    },
};

#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectDeadline(Arc<Mutex<Option<Instant>>>);
impl ConnectDeadline {
    pub(crate) fn resolver(&self, file: bool) -> DeadlineResolver {
        DeadlineResolver {
            deadline: self.clone(),
            file,
        }
    }
    pub(crate) fn connect(
        &self,
        details: &ConnectionDetails<'_>,
    ) -> Result<Option<Box<dyn Transport>>, ureq::Error> {
        let deadline = *self
            .0
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let timeout = remaining(deadline, details.timeout)?;
        let details = ConnectionDetails {
            uri: details.uri,
            addrs: details.addrs.clone(),
            config: details.config,
            request_level: details.request_level,
            resolver: details.resolver,
            now: details.now,
            timeout,
            current_time: details.current_time.clone(),
            run_connector: details.run_connector.clone(),
        };
        let tcp = TcpConnector::default().connect(&details, None::<()>)?;
        let active = Arc::new(AtomicBool::new(true));
        let tcp = tcp.map(|inner| DeadlineTransport {
            inner: Box::new(inner),
            deadline,
            active: active.clone(),
        });
        let result = RustlsConnector::default().connect(&details, tcp);
        active.store(false, Ordering::Release);
        remaining(deadline, details.timeout)?;
        Ok(result?.map(|inner| Box::new(inner) as Box<dyn Transport>))
    }
}
#[derive(Debug)]
pub(crate) struct DeadlineResolver {
    deadline: ConnectDeadline,
    file: bool,
}
impl Resolver for DeadlineResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &Config,
        mut timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        if self.file {
            timeout = NextTimeout {
                after: Duration::from_secs(20).into(),
                reason: ureq::Timeout::Connect,
            };
        }
        let deadline = if timeout.after.is_not_happening() {
            None
        } else {
            Some(Instant::now() + *timeout.after)
        };
        *self
            .deadline
            .0
            .lock()
            .map_err(|error| std::io::Error::other(error.to_string()))? = deadline;
        DefaultResolver::default().resolve(uri, config, timeout)
    }
}
fn remaining(
    deadline: Option<Instant>,
    mut timeout: NextTimeout,
) -> Result<NextTimeout, ureq::Error> {
    if let Some(deadline) = deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ureq::Error::Timeout(timeout.reason));
        }
        timeout.after = timeout.after.min(remaining.into());
    }
    Ok(timeout)
}
#[derive(Debug)]
struct DeadlineTransport {
    inner: Box<dyn Transport>,
    deadline: Option<Instant>,
    active: Arc<AtomicBool>,
}
impl DeadlineTransport {
    fn timeout(&self, timeout: NextTimeout) -> Result<NextTimeout, ureq::Error> {
        if self.active.load(Ordering::Acquire) {
            remaining(self.deadline, timeout)
        } else {
            Ok(timeout)
        }
    }
}
impl Transport for DeadlineTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        let timeout = self.timeout(timeout)?;
        self.inner.transmit_output(amount, timeout)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let timeout = self.timeout(timeout)?;
        self.inner.await_input(timeout)
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}
