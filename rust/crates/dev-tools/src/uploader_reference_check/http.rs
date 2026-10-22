//! Independent loopback peer and primitive JOSE verifier; no uploader implementation import.
use super::{require, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use rsa::{
    pkcs1::EncodeRsaPrivateKey,
    pkcs8::{DecodePublicKey, EncodePublicKey, LineEnding},
    Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub id: String,
    pub key_name: String,
    pub settings: Value,
    source_requests: Vec<Request>,
    public_key_pem: String,
    source_api_host: String,
    source_assertions_completed: bool,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body_hex: String,
}
pub(crate) struct Keys {
    rsa: RsaPrivateKey,
    ec: p256::SecretKey,
}
impl Keys {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            rsa: RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)?,
            ec: p256::SecretKey::random(&mut rsa::rand_core::OsRng),
        })
    }
    pub(crate) fn write(&self, root: &Path, name: &str) -> Result<()> {
        let dir = root.join("comma");
        fs::create_dir_all(&dir)?;
        let (private, public) = match name {
            "id_rsa" => (
                self.rsa.to_pkcs1_pem(LineEnding::LF)?.to_string(),
                RsaPublicKey::from(&self.rsa).to_public_key_pem(LineEnding::LF)?,
            ),
            "id_ecdsa" => (
                self.ec.to_sec1_pem(LineEnding::LF)?.to_string(),
                self.ec.public_key().to_public_key_pem(LineEnding::LF)?,
            ),
            _ => return Err("unknown uploader signing key kind".into()),
        };
        fs::write(dir.join(name), private)?;
        fs::write(dir.join(format!("{name}.pub")), public)?;
        Ok(())
    }
    fn public(&self, name: &str) -> Result<String> {
        match name {
            "id_rsa" => Ok(RsaPublicKey::from(&self.rsa).to_public_key_pem(LineEnding::LF)?),
            "id_ecdsa" => Ok(self.ec.public_key().to_public_key_pem(LineEnding::LF)?),
            _ => Err("unknown uploader signing key kind".into()),
        }
    }
}
pub(crate) fn write_ecdsa(root: &Path) -> Result<()> {
    let key = p256::SecretKey::random(&mut rsa::rand_core::OsRng);
    let dir = root.join("comma");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("id_ecdsa"),
        key.to_sec1_pem(LineEnding::LF)?.as_bytes(),
    )?;
    fs::write(
        dir.join("id_ecdsa.pub"),
        key.public_key().to_public_key_pem(LineEnding::LF)?,
    )?;
    Ok(())
}
fn unhex(text: &str) -> Result<Vec<u8>> {
    require(
        text.len().is_multiple_of(2) && text.len() <= 8 << 20,
        "HTTP fixture body hex outside bound",
    )?;
    text.as_bytes()
        .chunks_exact(2)
        .map(|b| Ok(u8::from_str_radix(std::str::from_utf8(b)?, 16)?))
        .collect()
}
fn digest_info(message: &[u8]) -> Vec<u8> {
    let mut bytes = vec![
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    bytes.extend_from_slice(&Sha256::digest(message));
    bytes
}
fn token(request: &Request) -> Result<&str> {
    let values = request
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v)
        .collect::<Vec<_>>();
    require(
        values.len() == 1,
        "uploader JWT Authorization missing or duplicated",
    )?;
    values[0]
        .strip_prefix("JWT ")
        .ok_or_else(|| "uploader authorization scheme differs".into())
}
fn verify_jwt(request: &Request, public: &str, key_name: &str, fresh: bool) -> Result<()> {
    let token = token(request)?;
    let parts = token.split('.').collect::<Vec<_>>();
    require(parts.len() == 3, "uploader JWT is not JOSE compact form")?;
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0])?)?;
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1])?)?;
    let signature = URL_SAFE_NO_PAD.decode(parts[2])?;
    let message = format!("{}.{}", parts[0], parts[1]);
    let algorithm = if key_name == "id_rsa" {
        "RS256"
    } else if key_name == "id_ecdsa" {
        "ES256"
    } else {
        return Err("unknown original JWT key kind".into());
    };
    require(
        header["alg"] == algorithm && header["typ"] == "JWT",
        "uploader JOSE header differs",
    )?;
    match algorithm {
        "RS256" => RsaPublicKey::from_public_key_pem(public)?.verify(
            Pkcs1v15Sign::new_unprefixed(),
            &digest_info(message.as_bytes()),
            &signature,
        )?,
        _ => VerifyingKey::from_public_key_pem(public)?
            .verify(message.as_bytes(), &Signature::from_slice(&signature)?)?,
    }
    let issued = claims["iat"]
        .as_i64()
        .ok_or("uploader iat is not an integer")?;
    let not_before = claims["nbf"]
        .as_i64()
        .ok_or("uploader nbf is not an integer")?;
    let expiry = claims["exp"]
        .as_i64()
        .ok_or("uploader exp is not an integer")?;
    require(
        claims["identity"] == "0000000000000000"
            && not_before == issued
            && expiry.checked_sub(issued) == Some(3600),
        "signed uploader claims differ from original policy",
    )?;
    if fresh {
        let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
        require(
            not_before <= now && now < expiry,
            "native uploader JWT is outside its validity interval",
        )?;
    }
    Ok(())
}
pub(crate) fn verify_observation(
    original: &Observation,
    actual: &[Request],
    keys: &Keys,
) -> Result<()> {
    require(
        original.source_assertions_completed
            && original.source_api_host.starts_with("http://127.0.0.1:")
            && actual.len() == original.source_requests.len(),
        "original/native HTTP request roster differs",
    )?;
    let public = keys.public(&original.key_name)?;
    for (expected, received) in original.source_requests.iter().zip(actual) {
        require(
            expected.method == received.method
                && expected.path == received.path
                && expected.body_hex == received.body_hex,
            "uploader HTTP method/path/compressed bytes differ from original",
        )?;
        if expected.method == "GET" && expected.path.starts_with("/v1.4/") {
            verify_jwt(
                expected,
                &original.public_key_pem,
                &original.key_name,
                false,
            )?;
            verify_jwt(received, &public, &original.key_name, true)?;
            require(
                expected.path.contains("qlog.zst"),
                "compressed upload API key missing",
            )?;
        } else if expected.method == "PUT" && !expected.body_hex.is_empty() {
            let body = unhex(&received.body_hex)?;
            let mut decoded = Vec::new();
            zstd::stream::read::Decoder::new(Cursor::new(body))?
                .take(524_289)
                .read_to_end(&mut decoded)?;
            require(
                decoded.len() == 524_288 && decoded.iter().all(|b| *b == 0),
                "uploader zstd payload differs from original synthetic zeros",
            )?;
        }
    }
    Ok(())
}

pub(crate) struct Server {
    pub url: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<std::result::Result<Vec<Request>, String>>>,
}
fn receive(stream: &mut TcpStream) -> Result<Request> {
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let started = Instant::now();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        require(
            head.len() < 65536 && started.elapsed() < Duration::from_secs(2),
            "uploader peer HTTP header bound exceeded",
        )?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    let text = std::str::from_utf8(&head)?;
    let mut lines = text.split("\r\n");
    let start = lines
        .next()
        .ok_or("HTTP request line missing")?
        .split_whitespace()
        .collect::<Vec<_>>();
    require(
        start.len() == 3 && start[2].starts_with("HTTP/1."),
        "HTTP request line differs",
    )?;
    let mut headers = BTreeMap::new();
    let mut length = None;
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or("malformed uploader HTTP header")?;
        if key.eq_ignore_ascii_case("content-length") {
            require(length.is_none(), "duplicate content length")?;
            length = Some(value.trim().parse::<usize>()?);
        }
        require(
            headers
                .insert(key.to_ascii_lowercase(), value.trim().to_owned())
                .is_none(),
            "duplicate uploader HTTP header",
        )?;
    }
    let body = if let Some(transfer) = headers.get("transfer-encoding") {
        require(
            transfer.eq_ignore_ascii_case("chunked") && length.is_none(),
            "unsupported or ambiguous uploader request framing",
        )?;
        let mut body = Vec::new();
        loop {
            let mut line = Vec::new();
            while !line.ends_with(b"\r\n") {
                require(
                    line.len() < 64 && started.elapsed() < Duration::from_secs(2),
                    "uploader chunk header bound exceeded",
                )?;
                let mut byte = [0];
                stream.read_exact(&mut byte)?;
                line.push(byte[0]);
            }
            let text = std::str::from_utf8(&line[..line.len() - 2])?;
            require(
                !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "invalid uploader chunk size",
            )?;
            let size = usize::from_str_radix(text, 16)?;
            require(
                size <= (4 << 20) - body.len(),
                "uploader chunked body bound exceeded",
            )?;
            if size > 0 {
                let offset = body.len();
                body.resize(offset + size, 0);
                stream.read_exact(&mut body[offset..])?;
            }
            let mut end = [0; 2];
            stream.read_exact(&mut end)?;
            require(
                end == *b"\r\n",
                "invalid uploader chunk terminator or trailers",
            )?;
            if size == 0 {
                break;
            }
        }
        body
    } else {
        let length = length.unwrap_or(0);
        require(length <= 4 << 20, "uploader peer body bound exceeded")?;
        let mut body = vec![0; length];
        stream.read_exact(&mut body)?;
        body
    };
    Ok(Request {
        method: start[0].into(),
        path: start[1].into(),
        headers,
        body_hex: body.iter().map(|b| format!("{b:02x}")).collect(),
    })
}
fn setting(settings: &Value, key: &str, default: u16) -> Result<u16> {
    settings.get(key).map_or(Ok(default), |v| {
        Ok(u16::try_from(
            v.as_u64().ok_or("invalid HTTP status setting")?,
        )?)
    })
}
fn reply(stream: &mut TcpStream, request: &Request, url: &str, settings: &Value) -> Result<()> {
    let (status, body, location) = if request.method == "GET" && request.path.starts_with("/v1.4/")
    {
        if settings["stall"].as_bool() == Some(true) {
            thread::sleep(Duration::from_millis(400));
        }
        let body = if settings["bad_json"].as_bool() == Some(true) {
            b"invalid-json".to_vec()
        } else {
            serde_json::to_vec(
                &json!({"url":format!("{url}/put"),"headers":settings.get("headers").cloned().unwrap_or_else(||json!({"X-Fixture":"synthetic"}))}),
            )?
        };
        (setting(settings, "api_status", 200)?, body, None)
    } else if request.method == "PUT"
        && request.path == "/put"
        && settings.get("redirect").is_some()
    {
        (
            setting(settings, "redirect", 307)?,
            Vec::new(),
            Some("/final"),
        )
    } else {
        (
            setting(settings, "put_status", 200)?,
            if request.method == "PUT" {
                b"uploaded".to_vec()
            } else {
                b"done".to_vec()
            },
            None,
        )
    };
    let response = (|| -> io::Result<()> {
        write!(
            stream,
            "HTTP/1.0 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        )?;
        if let Some(location) = location {
            write!(stream, "Location: {location}\r\n")?;
        }
        stream.write_all(b"\r\n")?;
        if settings["slow_body"].as_bool() == Some(true) {
            for chunk in body.chunks((body.len() / 4).max(1)) {
                stream.write_all(chunk)?;
                stream.flush()?;
                thread::sleep(Duration::from_millis(80));
            }
        } else {
            stream.write_all(&body)?;
        }
        Ok(())
    })();
    match response {
        Ok(()) => Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
impl Server {
    pub(crate) fn start(settings: &Value) -> Result<Self> {
        let object = settings
            .as_object()
            .ok_or("HTTP settings must be an object")?;
        require(
            object.keys().all(|k| {
                [
                    "api_status",
                    "bad_json",
                    "fake",
                    "put_status",
                    "redirect",
                    "stall",
                    "slow_body",
                    "headers",
                ]
                .contains(&k.as_str())
            }),
            "unknown original HTTP setting",
        )?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let url = format!("http://{}", listener.local_addr()?);
        let peer_url = url.clone();
        let settings = settings.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let running = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            while !running.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = receive(&mut stream).map_err(|e| e.to_string())?;
                        reply(&mut stream, &request, &peer_url, &settings)
                            .map_err(|e| e.to_string())?;
                        requests.push(request);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
            Ok(requests)
        });
        Ok(Self {
            url,
            stop,
            worker: Some(worker),
        })
    }
    pub(crate) fn finish(mut self) -> Result<Vec<Request>> {
        self.stop.store(true, Ordering::Release);
        self.worker
            .take()
            .ok_or("HTTP peer worker missing")?
            .join()
            .map_err(|_| "HTTP peer panicked")?
            .map_err(Into::into)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
