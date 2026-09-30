use crate::{diagnostics::python_text, Event, EventSink, Transfer, TransferError, UploadResponse};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use openpilot_logging::{log_site, record::Level, Value};
use p256::pkcs8::EncodePrivateKey;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use ureq::unversioned::{
    resolver::DefaultResolver,
    transport::{Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport},
};

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
        let result = self.inner.await_input(NextTimeout {
            after: self.timeout.into(),
            reason: ureq::Timeout::RecvBody,
        });
        self.completed(started, ureq::Timeout::RecvBody, result)
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

pub struct SigningKey {
    algorithm: Algorithm,
    pem: Vec<u8>,
}
impl SigningKey {
    pub fn load(persist: &Path) -> io::Result<Option<Self>> {
        for (name, algorithm) in [("id_rsa", Algorithm::RS256), ("id_ecdsa", Algorithm::ES256)] {
            let path = persist.join("comma").join(name);
            let public = path.with_extension("pub");
            if path.is_file() && public.is_file() {
                let pem = fs::read_to_string(path)?.into_bytes();
                let _ = fs::read_to_string(public)?;
                return Ok(Some(Self { algorithm, pem }));
            }
        }
        Ok(None)
    }
    pub fn token(&self, identity: &str, seconds: u64) -> Result<String, TransferError> {
        let key = match self.algorithm {
            Algorithm::RS256 => EncodingKey::from_rsa_pem(&self.pem),
            Algorithm::ES256 if self.pem.starts_with(b"-----BEGIN EC PRIVATE KEY-----") => {
                let pem = std::str::from_utf8(&self.pem)?;
                let key = p256::SecretKey::from_sec1_pem(pem)?;
                let pkcs8 = key.to_pkcs8_der()?;
                Ok(EncodingKey::from_ec_der(pkcs8.as_bytes()))
            }
            Algorithm::ES256 => EncodingKey::from_ec_pem(&self.pem),
            _ => {
                return Err(TransferError::Contract(
                    "unsupported upload signing algorithm",
                ))
            }
        }?;
        jsonwebtoken::encode(
            &Header::new(self.algorithm),
            &json!({"identity":identity,"nbf":seconds,"iat":seconds,"exp":seconds+3600}),
            &key,
        )
        .map_err(TransferError::from)
    }
}

pub struct HttpTransfer {
    pub api_host: String,
    pub dongle_id: String,
    pub key: Option<SigningKey>,
    pub version_header: PathBuf,
    pub fake_upload: bool,
    pub socket_timeout: Duration,
}
enum Body {
    File(File),
    Compressed(Cursor<Vec<u8>>),
}
impl Read for Body {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.read(output),
            Self::Compressed(f) => f.read(output),
        }
    }
}
impl Seek for Body {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        match self {
            Self::File(f) => f.seek(position),
            Self::Compressed(f) => f.seek(position),
        }
    }
}
impl Body {
    fn open(path: &Path, compress: bool) -> io::Result<Self> {
        let mut file = File::open(path)?;
        if compress {
            let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 10)?;
            io::copy(&mut file, &mut encoder)?;
            Ok(Self::Compressed(Cursor::new(encoder.finish()?)))
        } else {
            Ok(Self::File(file))
        }
    }
    fn length(&mut self) -> io::Result<u64> {
        let size = self.seek(SeekFrom::End(0))?;
        self.seek(SeekFrom::Start(0))?;
        Ok(size)
    }
}

struct Response {
    status: u16,
    body: Vec<u8>,
    length: String,
}
impl HttpTransfer {
    fn agent(&self) -> ureq::Agent {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(self.socket_timeout))
            .timeout_global(None)
            .max_idle_connections(0)
            // Preserve the inherited PUT default from the repository's uv.lock.
            .user_agent("python-requests/2.34.2")
            .build();
        ureq::Agent::with_parts(
            config,
            SocketTimeoutConnector(self.socket_timeout),
            DefaultResolver::default(),
        )
    }
    fn request(
        &self,
        mut url: url::Url,
        mut method: ureq::http::Method,
        mut headers: BTreeMap<String, String>,
        mut body: Option<&mut Body>,
    ) -> Result<Response, TransferError> {
        let agent = self.agent();
        for redirects in 0..=30 {
            let mut request = ureq::http::Request::builder()
                .method(method.clone())
                .uri(url.as_str());
            let length = if let Some(ref mut body) = body {
                let length = body.length()?.to_string();
                headers.insert("content-length".into(), length.clone());
                length
            } else {
                headers
                    .get("content-length")
                    .cloned()
                    .unwrap_or_else(|| "0".into())
            };
            for (name, value) in &headers {
                request = request.header(name, value);
            }
            let mut response = if let Some(ref mut body) = body {
                agent.run(request.body(ureq::SendBody::from_reader(&mut **body))?)
            } else {
                agent.run(request.body(())?)
            }?;
            let status = response.status().as_u16();
            let location = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let mut bytes = Vec::new();
            response.body_mut().as_reader().read_to_end(&mut bytes)?;
            if let Some(location) =
                location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308))
            {
                if redirects == 30 {
                    return Err(TransferError::Contract("exceeded 30 redirects"));
                }
                let next = url.join(&location)?;
                let standard_upgrade = url.scheme() == "http"
                    && next.scheme() == "https"
                    && url.port_or_known_default() == Some(80)
                    && next.port_or_known_default() == Some(443);
                if url.host_str() != next.host_str()
                    || (!standard_upgrade
                        && (url.scheme() != next.scheme()
                            || url.port_or_known_default() != next.port_or_known_default()))
                {
                    headers.remove("authorization");
                }
                if status == 302
                    || status == 303
                    || (status == 301 && method == ureq::http::Method::POST)
                {
                    method = ureq::http::Method::GET;
                }
                if !matches!(status, 307 | 308) {
                    body = None;
                    headers.remove("content-length");
                    headers.remove("content-type");
                    headers.remove("transfer-encoding");
                }
                headers.remove("cookie");
                url = next;
            } else {
                return Ok(Response {
                    status,
                    body: bytes,
                    length,
                });
            }
        }
        Err(TransferError::Contract("redirect limit"))
    }
    fn do_upload(
        &self,
        key: &Path,
        path: &Path,
        events: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        let key_text = key
            .to_str()
            .ok_or(TransferError::Contract("upload key is not UTF-8"))?;
        let seconds = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let token = self
            .key
            .as_ref()
            .ok_or(TransferError::Contract("uploader signing key unavailable"))?
            .token(&self.dongle_id, seconds)?;
        let version = fs::read_to_string(&self.version_header)?;
        let version = version
            .split('"')
            .nth(1)
            .ok_or(TransferError::Contract("invalid version.h"))?;
        let mut url = url::Url::parse(&format!(
            "{}/v1.4/{}/upload_url/",
            self.api_host, self.dongle_id
        ))?;
        url.query_pairs_mut().append_pair("path", key_text);
        let response = self.request(
            url,
            ureq::http::Method::GET,
            BTreeMap::from([
                ("authorization".into(), format!("JWT {token}")),
                ("user-agent".into(), format!("openpilot-{version}")),
            ]),
            None,
        )?;
        if response.status == 412 {
            return Ok(UploadResponse {
                status: 412,
                content_length: response.length,
            });
        }
        let value: Value = serde_json::from_slice(&response.body)?;
        let Value::Object(value) = value else {
            return Err(TransferError::Contract("upload URL missing"));
        };
        let url = value
            .get("url")
            .ok_or(TransferError::Contract("upload URL missing"))?;
        let headers = value
            .get("headers")
            .ok_or(TransferError::Contract("upload headers missing"))?;
        events.emit(Event::text(
            log_site!(),
            Level::Debug,
            format!(
                "upload_url v1.4 {} {}",
                python_text(url)?,
                python_text(headers)?
            ),
        ))?;
        if self.fake_upload {
            return Ok(UploadResponse {
                status: 200,
                content_length: "0".into(),
            });
        }
        let Value::Text(url) = url else {
            return Err(TransferError::Contract("upload URL is not a string"));
        };
        let Value::Object(headers) = headers else {
            return Err(TransferError::Contract("upload headers are not an object"));
        };
        let headers = headers
            .iter()
            .map(|(name, value)| {
                let Value::Text(value) = value else {
                    return Err(TransferError::Contract("upload header is not a string"));
                };
                Ok((name.to_ascii_lowercase(), value.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let compress =
            key_text.ends_with(".zst") && !path.as_os_str().as_encoded_bytes().ends_with(b".zst");
        let mut body = Body::open(path, compress)?;
        let response = self.request(
            url::Url::parse(url)?,
            ureq::http::Method::PUT,
            headers,
            Some(&mut body),
        )?;
        Ok(UploadResponse {
            status: response.status,
            content_length: response.length,
        })
    }
}
impl Transfer for HttpTransfer {
    fn upload(
        &mut self,
        key: &Path,
        path: &Path,
        events: &mut dyn EventSink,
    ) -> Result<UploadResponse, TransferError> {
        self.do_upload(key, path, events)
    }
}
