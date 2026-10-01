use crate::policy::UPLOAD_TOS;
use socket2::SockRef;
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{
        Buffers, ConnectProxyConnector, ConnectionDetails, Connector, Either, LazyBuffers,
        NextTimeout, RustlsConnector, Transport,
    },
};

#[derive(Debug)]
struct Tcp;
impl<In: Transport> Connector<In> for Tcp {
    type Out = Either<In, Socket>;
    fn connect(
        &self,
        details: &ConnectionDetails<'_>,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        if let Some(transport) = chained {
            return Ok(Some(Either::A(transport)));
        }
        let mut failure =
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no resolved address");
        for address in &details.addrs {
            match crate::net::connect(address, Duration::from_secs(30)) {
                Ok(stream) => {
                    SockRef::from(&stream).set_tos_v4(UPLOAD_TOS)?;
                    return Ok(Some(Either::B(Socket {
                        stream,
                        buffers: LazyBuffers::new(
                            details.config.input_buffer_size(),
                            details.config.output_buffer_size(),
                        ),
                        open: true,
                    })));
                }
                Err(error) => failure = error,
            }
        }
        Err(failure.into())
    }
}
#[derive(Debug)]
struct Socket {
    stream: TcpStream,
    buffers: LazyBuffers,
    open: bool,
}
impl Transport for Socket {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, amount: usize, _: NextTimeout) -> Result<(), ureq::Error> {
        let start = Instant::now();
        let mut offset = 0;
        while offset < amount {
            let remaining = Duration::from_secs(30).saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(ureq::Timeout::SendBody));
            }
            self.stream.set_write_timeout(Some(remaining))?;
            match self.stream.write(&self.buffers.output()[offset..amount]) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "upload socket closed",
                    )
                    .into())
                }
                Ok(count) => offset += count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    fn await_input(&mut self, _: NextTimeout) -> Result<bool, ureq::Error> {
        let start = Instant::now();
        loop {
            let remaining = Duration::from_secs(30).saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(ureq::Error::Timeout(ureq::Timeout::RecvBody));
            }
            self.stream.set_read_timeout(Some(remaining))?;
            match self.stream.read(self.buffers.input_append_buf()) {
                Ok(count) => {
                    self.buffers.input_appended(count);
                    self.open = count != 0;
                    return Ok(count != 0);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn is_open(&mut self) -> bool {
        if !self.open || self.stream.set_nonblocking(true).is_err() {
            return false;
        }
        let mut byte = [0];
        let ready = matches!(self.stream.peek(&mut byte),Err(error) if error.kind()==std::io::ErrorKind::WouldBlock);
        self.open = ready && self.stream.set_nonblocking(false).is_ok();
        self.open
    }
    fn is_tls(&self) -> bool {
        false
    }
}
pub fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(None)
        .timeout_connect(Some(Duration::from_secs(30)))
        .user_agent("python-requests/2.34.2")
        .build();
    let connector =
        ().chain(ConnectProxyConnector::default())
            .chain(Tcp)
            .chain(RustlsConnector::default());
    ureq::Agent::with_parts(config, connector, DefaultResolver::default())
}
