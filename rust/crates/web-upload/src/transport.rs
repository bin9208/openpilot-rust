use crate::CHUNK_SIZE;
use std::time::{Duration, Instant};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport},
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Policy {
    Total,
    File,
    Socket(Duration),
}
#[derive(Debug)]
struct Timeouts {
    policy: Policy,
    deadline: crate::connection::ConnectDeadline,
}
impl Connector for Timeouts {
    type Out = Timed;
    fn connect(
        &self,
        details: &ConnectionDetails<'_>,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let transport = match self.policy {
            Policy::Socket(_) => DefaultConnector::default().connect(details, chained)?,
            Policy::Total | Policy::File => self.deadline.connect(details)?,
        };
        Ok(transport.map(|inner| Timed {
            inner,
            policy: self.policy,
        }))
    }
}
#[derive(Debug)]
struct Timed {
    inner: Box<dyn Transport>,
    policy: Policy,
}
impl Transport for Timed {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        let timeout = match self.policy {
            Policy::Total | Policy::File => timeout,
            Policy::Socket(duration) => NextTimeout {
                after: duration.into(),
                reason: ureq::Timeout::SendBody,
            },
        };
        let started = Instant::now();
        let result = self.inner.transmit_output(amount, timeout);
        completed(started, timeout, result)
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let timeout = match self.policy {
            Policy::Total => timeout,
            Policy::File => NextTimeout {
                after: Duration::from_secs(180).into(),
                reason: ureq::Timeout::RecvBody,
            },
            Policy::Socket(duration) => NextTimeout {
                after: duration.into(),
                reason: ureq::Timeout::RecvBody,
            },
        };
        let started = Instant::now();
        let result = self.inner.await_input(timeout);
        completed(started, timeout, result)
    }
    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }
    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}
fn completed<T>(
    started: Instant,
    timeout: NextTimeout,
    result: Result<T, ureq::Error>,
) -> Result<T, ureq::Error> {
    // SO_RCVTIMEO can round up substantially for long waits. A response that
    // arrives after the source deadline must not turn a timeout into success.
    if !timeout.after.is_not_happening() && started.elapsed() >= *timeout.after {
        Err(ureq::Error::Timeout(timeout.reason))
    } else {
        result
    }
}
pub(crate) fn agent(policy: Policy) -> ureq::Agent {
    let connect = match policy {
        Policy::Total => None,
        Policy::File => Some(Duration::from_secs(20)),
        Policy::Socket(duration) => Some(duration),
    };
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(None)
        .timeout_connect(connect)
        .max_idle_connections(0)
        .input_buffer_size(CHUNK_SIZE)
        .output_buffer_size(CHUNK_SIZE + 4096)
        .user_agent("openpilot-rust-web-upload");
    let config = match policy {
        Policy::Socket(_) => config,
        Policy::Total | Policy::File => config.proxy(None),
    }
    .build();
    let deadline = crate::connection::ConnectDeadline::default();
    let connector = Timeouts {
        policy,
        deadline: deadline.clone(),
    };
    match policy {
        Policy::Socket(_) => ureq::Agent::with_parts(config, connector, DefaultResolver::default()),
        Policy::Total | Policy::File => ureq::Agent::with_parts(
            config,
            connector,
            deadline.resolver(matches!(policy, Policy::File)),
        ),
    }
}
