use crate::{
    transport::{agent, Policy},
    Error, Response,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Seek},
    time::{Duration, Instant},
};
use ureq::RequestExt;

pub(crate) enum Body<'a> {
    Empty,
    Bytes(Cursor<Vec<u8>>),
    Stream(&'a mut dyn Read),
}
impl Read for Body<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Empty => Ok(0),
            Self::Bytes(value) => value.read(output),
            Self::Stream(value) => value.read(output),
        }
    }
}
pub(crate) struct Client {
    agent: ureq::Agent,
    total: Option<Duration>,
    requests: bool,
}
impl Client {
    pub(crate) fn total() -> Self {
        Self {
            agent: agent(Policy::Total),
            total: Some(Duration::from_secs(12)),
            requests: false,
        }
    }
    pub(crate) fn file() -> Self {
        Self {
            agent: agent(Policy::File),
            total: None,
            requests: false,
        }
    }
    pub(crate) fn socket(seconds: u64) -> Self {
        Self {
            agent: agent(Policy::Socket(Duration::from_secs(seconds))),
            total: None,
            requests: true,
        }
    }
    pub(crate) fn request(
        &self,
        request: Request<'_>,
        mut body: Body<'_>,
    ) -> Result<Response, Error> {
        let mut url = url::Url::parse(request.url)?;
        let mut method = request.method;
        let mut headers = request.headers;
        let started = Instant::now();
        let limit = if self.requests { 30 } else { 10 };
        let mut count = 0;
        let mut retry_connection =
            !self.requests && matches!(method, ureq::http::Method::GET | ureq::http::Method::PUT);
        loop {
            let mut builder = ureq::http::Request::builder()
                .method(method.clone())
                .uri(url.as_str());
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
            let length = match &body {
                Body::Empty => Some(0),
                Body::Bytes(cursor) => Some(cursor.get_ref().len()),
                Body::Stream(_) => None,
            };
            if let Some(length) = length {
                if method != ureq::http::Method::GET {
                    builder = builder.header("Content-Length", length);
                }
            }
            let total = self
                .total
                .map(|duration| duration.saturating_sub(started.elapsed()));
            let response = if matches!(body, Body::Empty) {
                builder
                    .body(())?
                    .with_agent(&self.agent)
                    .configure()
                    .timeout_global(total)
                    .build()
                    .run()
            } else {
                builder
                    .body(ureq::SendBody::from_reader(&mut body))?
                    .with_agent(&self.agent)
                    .configure()
                    .timeout_global(total)
                    .build()
                    .run()
            };
            let mut response = match response {
                Ok(response) => response,
                Err(ureq::Error::Io(error))
                    if retry_connection
                        && matches!(
                            error.kind(),
                            std::io::ErrorKind::UnexpectedEof
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                                | std::io::ErrorKind::BrokenPipe
                        ) =>
                {
                    retry_connection = false;
                    if let Body::Bytes(cursor) = &mut body {
                        cursor.rewind()?;
                    }
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let status = response.status().as_u16();
            let response_headers = response.headers().clone();
            let mut bytes = Vec::new();
            response.body_mut().as_reader().read_to_end(&mut bytes)?;
            let location = response_headers
                .get("location")
                .and_then(|value| value.to_str().ok());
            if let Some(location) =
                location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308))
            {
                if count + 1 >= limit && !self.requests || count >= limit {
                    return Err(Error::Source(format!("exceeded {limit} redirects")));
                }
                let next = url.join(location)?;
                if url.origin() != next.origin() {
                    headers.retain(|name, _| !name.eq_ignore_ascii_case("authorization"));
                }
                let switch = status == 303
                    || (matches!(status, 301 | 302) && method == ureq::http::Method::POST)
                    || (self.requests && status == 302);
                if switch {
                    method = ureq::http::Method::GET;
                    body = Body::Empty;
                    if self.requests {
                        headers.retain(|name, _| !name.eq_ignore_ascii_case("content-type"));
                    }
                } else if let Body::Bytes(cursor) = &mut body {
                    cursor.rewind()?;
                }
                count += 1;
                url = next;
            } else {
                return Ok(Response {
                    status,
                    headers: response_headers,
                    body: bytes,
                });
            }
        }
    }
}
pub(crate) struct Request<'a> {
    pub url: &'a str,
    pub method: ureq::http::Method,
    pub headers: BTreeMap<String, String>,
}
pub(crate) fn auth(token: &str) -> BTreeMap<String, String> {
    if token.is_empty() {
        BTreeMap::new()
    } else {
        BTreeMap::from([("Authorization".into(), format!("Bearer {token}"))])
    }
}
