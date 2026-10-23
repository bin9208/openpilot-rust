use crate::{
    policy::{strip_zst, UploadItem},
    Error,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Cursor, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

// A fixed-schema stderr receipt, independent of source-compatible log events.
struct Attribution {
    attempt: u64,
    phase: &'static str,
    redirect_index: u8,
    response_status: u16,
    start_mono_ns: i64,
    start_wall_ns: u128,
}
fn stamps() -> (i64, u128) {
    (
        crate::state::mono_ns().unwrap_or(-1),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos()),
    )
}
impl Attribution {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let (start_mono_ns, start_wall_ns) = stamps();
        Self {
            attempt: NEXT.fetch_add(1, Ordering::Relaxed),
            phase: "prepare_body",
            redirect_index: 0,
            response_status: 0,
            start_mono_ns,
            start_wall_ns,
        }
    }
    fn failed(&self, error: &Error) {
        let (category, detail) = category(error);
        let (end_mono_ns, end_wall_ns) = stamps();
        // All strings are closed labels. Never format the supplied error, item,
        // URL, header, path or callback text. Receipt write failure is nonfatal.
        let _ = writeln!(io::stderr().lock(),
            "ATHENA_UPLOAD_FAILURE {{\"attempt\":{},\"pid\":{},\"phase\":\"{}\",\"redirect_index\":{},\"response_status\":{},\"category\":\"{}\",\"detail\":\"{}\",\"start_mono_ns\":{},\"end_mono_ns\":{},\"start_wall_ns\":{},\"end_wall_ns\":{}}}",
            self.attempt, std::process::id(), self.phase, self.redirect_index, self.response_status, category, detail,
            self.start_mono_ns, end_mono_ns, self.start_wall_ns, end_wall_ns);
    }
}
fn io_category(error: &io::Error) -> (&'static str, &'static str) {
    if let Some(error) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<ureq::Error>())
    {
        return http_category(error);
    }
    let detail = match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::TimedOut => "timed_out",
        io::ErrorKind::WouldBlock => "would_block",
        io::ErrorKind::UnexpectedEof => "unexpected_eof",
        io::ErrorKind::ConnectionRefused => "connection_refused",
        io::ErrorKind::ConnectionReset => "connection_reset",
        io::ErrorKind::ConnectionAborted => "connection_aborted",
        io::ErrorKind::BrokenPipe => "broken_pipe",
        io::ErrorKind::NotConnected => "not_connected",
        io::ErrorKind::InvalidData => "invalid_data",
        io::ErrorKind::InvalidInput => "invalid_input",
        io::ErrorKind::Interrupted => "interrupted",
        io::ErrorKind::WriteZero => "write_zero",
        _ => "other",
    };
    ("io", detail)
}
fn http_category(error: &ureq::Error) -> (&'static str, &'static str) {
    match error {
        ureq::Error::Io(error) => io_category(error),
        ureq::Error::Timeout(timeout) => (
            "timeout",
            match timeout {
                ureq::Timeout::Global => "global",
                ureq::Timeout::PerCall => "per_call",
                ureq::Timeout::Resolve => "resolve",
                ureq::Timeout::Connect => "connect",
                ureq::Timeout::SendRequest => "send_request",
                ureq::Timeout::Await100 => "await_100",
                ureq::Timeout::SendBody => "send_body",
                ureq::Timeout::RecvResponse => "recv_response",
                ureq::Timeout::RecvBody => "recv_body",
                _ => "other",
            },
        ),
        ureq::Error::HostNotFound => ("dns", "none"),
        ureq::Error::ConnectionFailed => ("connect", "none"),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) | ureq::Error::TlsRequired => ("tls", "none"),
        ureq::Error::Protocol(_) => ("protocol", "none"),
        ureq::Error::Http(_) | ureq::Error::BadUri(_) => ("request", "none"),
        _ => ("http", "other"),
    }
}
fn category(error: &Error) -> (&'static str, &'static str) {
    match error {
        Error::Io(error) => io_category(error),
        Error::Http(error) => http_category(error),
        Error::Request(_) => ("request", "none"),
        Error::Url(_) => ("url", "none"),
        Error::Contract(_) => ("contract", "none"),
        _ => ("other", "none"),
    }
}

enum Body {
    File(File),
    Compressed(Cursor<Vec<u8>>),
}
impl Body {
    fn open(path: &str) -> Result<Self, Error> {
        if !Path::new(path).exists() && Path::new(strip_zst(path)).exists() {
            let mut source = File::open(strip_zst(path))?;
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 10)?;
            io::copy(&mut source, &mut encoder)?;
            Ok(Self::Compressed(Cursor::new(encoder.finish()?)))
        } else {
            Ok(Self::File(File::open(path)?))
        }
    }
}
impl Read for Body {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(body) => body.read(bytes),
            Self::Compressed(body) => body.read(bytes),
        }
    }
}
impl Seek for Body {
    fn seek(&mut self, offset: SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(body) => body.seek(offset),
            Self::Compressed(body) => body.seek(offset),
        }
    }
}
struct Callback<'a, F> {
    body: &'a mut Body,
    callback: &'a mut F,
    size: u64,
    current: u64,
    attribution: &'a mut Attribution,
}
impl<F: FnMut(u64, u64) -> io::Result<()>> Read for Callback<'_, F> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.attribution.phase = "upload_body_read";
        let count = self.body.read(bytes)?;
        self.current +=
            u64::try_from(count).map_err(|_| io::Error::other("upload length overflow"))?;
        self.attribution.phase = "progress_callback";
        (self.callback)(self.size, self.current)?;
        self.attribution.phase = "request";
        Ok(count)
    }
}
pub fn upload(
    agent: &ureq::Agent,
    item: &UploadItem,
    mut callback: impl FnMut(u64, u64) -> io::Result<()>,
) -> Result<u16, Error> {
    let mut attribution = Attribution::new();
    let result = (|| {
        let mut body = Body::open(&item.path)?;
        let size = body.seek(SeekFrom::End(0))?;
        body.rewind()?;
        attribution.phase = "prepare_request";
        let mut url = url::Url::parse(&item.url)?;
        let mut method = ureq::http::Method::PUT;
        let mut headers = item
            .headers
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.to_ascii_lowercase(), value.to_owned()))
                    .ok_or(Error::Contract("upload header requires text"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        headers.insert("content-length".into(), size.to_string());
        let mut use_body = true;
        for redirects in 0..=30 {
            attribution.redirect_index = redirects;
            attribution.response_status = 0;
            attribution.phase = "request";
            let mut request = ureq::http::Request::builder()
                .uri(url.as_str())
                .method(method.clone());
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            let mut response = if use_body {
                attribution.phase = "prepare_body";
                body.rewind()?;
                attribution.phase = "request";
                let mut reader = Callback {
                    body: &mut body,
                    callback: &mut callback,
                    size,
                    current: 0,
                    attribution: &mut attribution,
                };
                agent.run(request.body(ureq::SendBody::from_reader(&mut reader))?)?
            } else {
                agent.run(request.body(())?)?
            };
            let status = response.status().as_u16();
            attribution.response_status = status;
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            attribution.phase = "response_body";
            io::copy(&mut response.body_mut().as_reader(), &mut io::sink())?;
            if let Some(location) =
                location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308))
            {
                attribution.phase = "redirect";
                if redirects == 30 {
                    return Err(Error::Contract("exceeded 30 redirects"));
                }
                let next = url.join(&location)?;
                let upgrade = url.scheme() == "http"
                    && next.scheme() == "https"
                    && url.port_or_known_default() == Some(80)
                    && next.port_or_known_default() == Some(443);
                if url.host_str() != next.host_str()
                    || (!upgrade
                        && (url.scheme() != next.scheme()
                            || url.port_or_known_default() != next.port_or_known_default()))
                {
                    headers.remove("authorization");
                }
                if status == 302 || status == 303 {
                    method = ureq::http::Method::GET;
                }
                if !matches!(status, 307 | 308) {
                    use_body = false;
                    headers.remove("content-length");
                    headers.remove("content-type");
                    headers.remove("transfer-encoding");
                }
                headers.remove("cookie");
                url = next;
            } else {
                return Ok(status);
            }
        }
        Err(Error::Contract("redirect limit"))
    })();
    if let Err(error) = &result {
        attribution.failed(error);
    }
    result
}

#[cfg(test)]
#[path = "upload_http_attribution_tests.rs"]
mod attribution_tests;
