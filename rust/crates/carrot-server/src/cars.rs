use crate::{http::Body, http_response, state::trim, Error, Value};
use hyper::{header, Method, Request, Response, StatusCode};
use std::{
    borrow::Cow, collections::BTreeSet, fs, io::Read, os::unix::ffi::OsStrExt, path::PathBuf,
    sync::Arc,
};

pub const BRANDS: [&str; 7] = [
    "hyundai",
    "gm",
    "toyota",
    "mazda",
    "ford",
    "volkswagen",
    "tesla",
];
const CATALOGS: [&str; 7] = [
    include_str!("../../manager/data/cars/hyundai.json"),
    include_str!("../../manager/data/cars/gm.json"),
    include_str!("../../manager/data/cars/toyota.json"),
    include_str!("../../manager/data/cars/mazda.json"),
    include_str!("../../manager/data/cars/ford.json"),
    include_str!("../../manager/data/cars/volkswagen.json"),
    include_str!("../../manager/data/cars/tesla.json"),
];
type Makers = Vec<(Vec<u32>, BTreeSet<Vec<u32>>)>;

pub struct Cars {
    directory: PathBuf,
    catalogs: [Cow<'static, str>; 7],
}

fn decoded(bytes: &[u8], surrogate_escape: bool) -> Vec<u32> {
    let mut remaining = bytes;
    let mut points = Vec::new();
    loop {
        match std::str::from_utf8(remaining) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                return points;
            }
            Err(error) => {
                let prefix = &remaining[..error.valid_up_to()];
                if let Ok(text) = std::str::from_utf8(prefix) {
                    points.extend(text.chars().map(u32::from));
                }
                let length = error.error_len().unwrap_or(remaining.len() - prefix.len());
                if surrogate_escape {
                    points.extend(
                        remaining[prefix.len()..prefix.len() + length]
                            .iter()
                            .map(|byte| 0xdc00 + u32::from(*byte)),
                    );
                }
                remaining = &remaining[prefix.len() + length..];
            }
        }
    }
}

fn add(makers: &mut Makers, points: &[u32], file: bool) {
    let line = trim(points);
    let Some(space) = line.iter().position(|point| *point == u32::from(' ')) else {
        return;
    };
    let maker = line[..space].to_vec();
    let full = if file {
        let mut full = maker.clone();
        full.push(u32::from(' '));
        full.extend(trim(&line[space + 1..]));
        full
    } else {
        line.to_vec()
    };
    if let Some((_, cars)) = makers.iter_mut().find(|(key, _)| *key == maker) {
        cars.insert(full);
    } else {
        makers.push((maker, BTreeSet::from([full])));
    }
}

fn read_file(path: &std::path::Path, makers: &mut Makers) {
    let Ok(file) = fs::File::open(path) else {
        return;
    };
    let mut line = Vec::new();
    for byte in std::io::BufReader::new(file).bytes() {
        let Ok(byte) = byte else {
            return;
        };
        if matches!(byte, b'\r' | b'\n') {
            add(makers, &decoded(&line, false), true);
            line.clear();
        } else {
            line.push(byte);
        }
    }
    add(makers, &decoded(&line, false), true);
}

fn load_brand(catalog: &str, makers: &mut Makers) -> Result<(), Error> {
    let Value::Array(names) = Value::parse(catalog)? else {
        return Err(Error::Source("brand car docs are not an array".into()));
    };
    for name in names {
        if let Value::Text(points) = name.py_string()? {
            add(makers, &points, false);
        }
    }
    Ok(())
}

impl Cars {
    pub fn original() -> Arc<Self> {
        Self::at(PathBuf::from("/data/params/d"))
    }

    pub fn at(directory: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            directory,
            catalogs: CATALOGS.map(Cow::Borrowed),
        })
    }

    pub fn with_catalogs_for_fixture(directory: PathBuf, catalogs: [String; 7]) -> Arc<Self> {
        Arc::new(Self {
            directory,
            catalogs: catalogs.map(Cow::Owned),
        })
    }

    pub fn load_supported_cars(&self) -> Result<(Value, Value), Error> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.directory) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.as_bytes().starts_with(b"SupportedCars") {
                    files.push((decoded(name.as_bytes(), true), entry.path()));
                }
            }
        }
        files.sort_by(|(left, _), (right, _)| left.cmp(right));
        let mut makers = Vec::new();
        for (_, path) in &files {
            read_file(path, &mut makers);
        }
        for catalog in &self.catalogs {
            if load_brand(catalog, &mut makers).is_err() {
                continue;
            }
        }
        Ok((
            Value::Array(
                files
                    .into_iter()
                    .map(|(name, _)| Value::Text(name))
                    .collect(),
            ),
            Value::Object(
                makers
                    .into_iter()
                    .map(|(name, cars)| {
                        (
                            name,
                            Value::Array(cars.into_iter().map(Value::Text).collect()),
                        )
                    })
                    .collect(),
            ),
        ))
    }

    pub fn payload(&self) -> Result<Value, Error> {
        let (sources, makers) = self.load_supported_cars()?;
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("sources", sources),
            ("makers", makers),
        ]))
    }
}

pub async fn handle<T>(request: &Request<T>, cars: Arc<Cars>) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut response = http_response::text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    match tokio::task::spawn_blocking(move || cars.payload()).await {
        Ok(Ok(payload)) => http_response::json_response(StatusCode::OK, payload, head, "")
            .unwrap_or_else(|error| http_response::error_response(error.to_string(), head)),
        Ok(Err(error)) => http_response::error_response(error.to_string(), head),
        Err(error) => http_response::error_response(error.to_string(), head),
    }
}
