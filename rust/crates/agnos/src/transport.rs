use crate::Error;
use std::{cell::RefCell, io::Read, time::Duration};

pub struct Client {
    agent: ureq::Agent,
    cookies: RefCell<cookie_store::CookieStore>,
}

pub struct Response {
    pub status: u16,
    pub content_length: Option<String>,
    pub body: Box<dyn Read + Send>,
}
pub fn request_error(error: ureq::Error) -> Error {
    let (class, transient) = match &error {
        ureq::Error::StatusCode(code) => ("HTTPError", matches!(code, 408 | 429) || *code >= 500),
        ureq::Error::Timeout(_) => ("Timeout", true),
        ureq::Error::Io(error) if error.get_ref().is_some_and(|e| e.is::<rustls::Error>()) => {
            ("SSLError", false)
        }
        ureq::Error::Io(_) | ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
            ("ConnectionError", true)
        }
        ureq::Error::Protocol(_) => ("ChunkedEncodingError", true),
        ureq::Error::Rustls(_) | ureq::Error::Tls(_) => ("SSLError", false),
        _ => ("RequestException", false),
    };
    Error::Request {
        class,
        transient,
        message: error.to_string(),
    }
}
pub fn agent(connect_timeout: u64, timeout: u64) -> Client {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(connect_timeout)))
        .timeout_global(None)
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .accept_encoding(ureq::config::AutoHeaderValue::None)
        .build();
    Client {
        agent: openpilot_http_transport::socket_timeout_agent(config, Duration::from_secs(timeout)),
        cookies: RefCell::new(cookie_store::CookieStore::default()),
    }
}
pub fn check_status(status: u16) -> Result<(), Error> {
    if status >= 400 {
        return Err(request_error(ureq::Error::StatusCode(status)));
    }
    Ok(())
}
pub fn get_raw(client: &Client, url: &str, offset: u64) -> Result<Response, Error> {
    let parsed = url::Url::parse(url).map_err(|error| Error::Request {
        class: if matches!(error, url::ParseError::RelativeUrlWithoutBase) {
            "MissingSchema"
        } else {
            "InvalidURL"
        },
        message: error.to_string(),
        transient: false,
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::Request {
            class: "InvalidSchema",
            message: format!("unsupported URL scheme: {}", parsed.scheme()),
            transient: false,
        });
    }
    let mut parsed = parsed;
    for redirects in 0..=30 {
        let mut request = client.agent.get(parsed.as_str());
        if offset != 0 {
            request = request.header("Range", format!("bytes={offset}-"));
        }
        let cookie_header = {
            let cookies = client.cookies.borrow();
            let mut matched = cookies.matches(&parsed);
            matched.retain(|cookie| cookie.secure() != Some(true) || parsed.scheme() == "https");
            matched.sort_by_key(|cookie| std::cmp::Reverse(cookie.path.as_ref().len()));
            matched
                .into_iter()
                .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
                .collect::<Vec<_>>()
                .join("; ")
        };
        if !cookie_header.is_empty() {
            request = request.header("Cookie", cookie_header);
        }
        // ureq 3.4.2 otherwise serializes Set-Cookie attributes into outbound Cookie values.
        client.agent.cookie_jar_lock().clear();
        let response = request.call().map_err(request_error)?;
        for value in response.headers().get_all("set-cookie") {
            if let Ok(value) = value.to_str() {
                let _ = client.cookies.borrow_mut().parse(value, &parsed);
            }
        }
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if let Some(location) = location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308)) {
            if redirects == 30 {
                return Err(Error::Request {
                    class: "TooManyRedirects",
                    message: "exceeded 30 redirects".into(),
                    transient: false,
                });
            }
            let mut body = response.into_body().into_reader();
            let mut buffer = vec![0; 1024 * 1024];
            while body_read(&mut body, &mut buffer)? != 0 {}
            parsed = parsed.join(&location).map_err(|e| Error::Request {
                class: "InvalidURL",
                message: e.to_string(),
                transient: false,
            })?;
            continue;
        }
        let content_length = response
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        return Ok(Response {
            status,
            content_length,
            body: Box::new(response.into_body().into_reader()),
        });
    }
    Err(Error::Contract("redirect limit".into()))
}

pub fn get(url: &str, offset: u64, connect_timeout: u64, timeout: u64) -> Result<Response, Error> {
    let response = get_raw(&agent(connect_timeout, timeout), url, offset)?;
    check_status(response.status)?;
    Ok(response)
}
pub fn all(agent: &Client, url: &str) -> Result<(u16, Vec<u8>), Error> {
    let mut response = get_raw(agent, url, 0)?;
    let mut output = Vec::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = body_read(&mut response.body, &mut buffer)?;
        if n == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..n]);
    }
    Ok((response.status, output))
}
pub fn body_read(body: &mut dyn Read, buffer: &mut [u8]) -> Result<usize, Error> {
    let mut used = 0;
    while used < buffer.len() {
        let count = body
            .read(&mut buffer[used..])
            .map_err(|error| {
                let error=ureq::Error::from(error);
                if matches!(&error,ureq::Error::Protocol(_))||matches!(&error,ureq::Error::Io(e) if e.kind()==std::io::ErrorKind::UnexpectedEof){
                    Error::Request{class:"ChunkedEncodingError",message:error.to_string(),transient:true}
                }else if matches!(&error,ureq::Error::Timeout(_)){Error::connection(error.to_string())}else{request_error(error)}
            })?;
        if count == 0 {
            break;
        }
        used += count;
    }
    Ok(used)
}
