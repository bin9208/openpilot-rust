use crate::{
    policy::{strip_zst, UploadItem},
    Error,
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

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
}
impl<F: FnMut(u64, u64) -> io::Result<()>> Read for Callback<'_, F> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.body.read(bytes)?;
        self.current +=
            u64::try_from(count).map_err(|_| io::Error::other("upload length overflow"))?;
        (self.callback)(self.size, self.current)?;
        Ok(count)
    }
}
pub fn upload(
    agent: &ureq::Agent,
    item: &UploadItem,
    mut callback: impl FnMut(u64, u64) -> io::Result<()>,
) -> Result<u16, Error> {
    let mut body = Body::open(&item.path)?;
    let size = body.seek(SeekFrom::End(0))?;
    body.rewind()?;
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
        let mut request = ureq::http::Request::builder()
            .uri(url.as_str())
            .method(method.clone());
        for (name, value) in &headers {
            request = request.header(name, value);
        }
        let mut response = if use_body {
            body.rewind()?;
            let mut reader = Callback {
                body: &mut body,
                callback: &mut callback,
                size,
                current: 0,
            };
            agent.run(request.body(ureq::SendBody::from_reader(&mut reader))?)?
        } else {
            agent.run(request.body(())?)?
        };
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        io::copy(&mut response.body_mut().as_reader(), &mut io::sink())?;
        if let Some(location) = location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308)) {
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
}
