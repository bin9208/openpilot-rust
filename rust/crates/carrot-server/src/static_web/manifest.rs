use crate::{Error, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

type FileKey = (PathBuf, SystemTime, u64);
struct CachedManifest {
    key: FileKey,
    manifest: Value,
    assets: Vec<FileKey>,
}

#[derive(Default)]
pub(super) struct ManifestLoader {
    cache: Mutex<Option<CachedManifest>>,
}

fn failure(message: &str) -> Error {
    Error::Source(message.into())
}

fn file_key(path: &Path) -> Result<FileKey, Error> {
    let metadata = fs::metadata(path)?;
    Ok((path.into(), metadata.modified()?, metadata.len()))
}

fn contained(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    if relative.starts_with('/')
        || relative.contains('\\')
        || relative.split('/').any(|part| part == "..")
    {
        return Err(failure("Asset path must be relative"));
    }
    let path =
        fs::canonicalize(root.join(relative)).map_err(|_| failure("Asset path is unavailable"))?;
    if !path.starts_with(root) {
        return Err(failure("Asset path is unavailable"));
    }
    if !path.is_file() {
        return Err(failure("Asset path is not a regular file"));
    }
    Ok(path)
}

fn string(value: &Value, message: &str) -> Result<String, Error> {
    match value {
        Value::Text(_) => value.string().map_err(Error::from),
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => Err(failure(message)),
    }
}

fn logical_id(id: &str) -> bool {
    id.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
        && id.split(['.', '_', '-']).all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn content_hash(bytes: &[u8]) -> String {
    let normalized: Vec<u8> = bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| {
            (!(*byte == b'\r' && bytes.get(index + 1) == Some(&b'\n'))).then_some(*byte)
        })
        .collect();
    format!("{:x}", Sha256::digest(normalized))
}

fn parse_asset(root: &Path, value: &Value) -> Result<Value, Error> {
    if !matches!(value, Value::Object(_)) {
        return Err(failure("Asset entry must be an object"));
    }
    let id = string(value.get("id"), "Asset id is invalid")?;
    if !logical_id(&id) {
        return Err(failure("Asset id is invalid"));
    }
    let bundle = value.get("kind").text_eq("bundle");
    if !bundle && !value.get("kind").text_eq("worker") {
        return Err(failure("Asset kind is invalid"));
    }
    let path = string(value.get("path"), "Asset path or hash is invalid")?;
    let hash = string(value.get("hash"), "Asset path or hash is invalid")?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(failure("Asset hash is invalid"));
    }
    let source = match value.get("source") {
        Value::Text(_) => Some(value.get("source").string()?),
        Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Array(_)
        | Value::Object(_) => None,
    };
    if bundle != source.is_some() {
        return Err(failure("Bundle source contract is invalid"));
    }
    let actual = content_hash(&fs::read(contained(root, &path)?)?);
    if actual != hash {
        eprintln!("Asset manifest hash is stale for {id}; using the current file content");
    }
    let mut asset = Value::object([
        ("id", Value::text(&id)),
        ("kind", value.get("kind").clone()),
        ("path", Value::text(&path)),
        ("hash", Value::text(&actual)),
    ]);
    if let (Value::Object(fields), Some(source)) = (&mut asset, source) {
        fields.push((
            "source".chars().map(u32::from).collect(),
            Value::text(&source),
        ));
    }
    Ok(asset)
}

fn parse_manifest(root: &Path, payload: &[u8]) -> Result<Value, Error> {
    let text =
        std::str::from_utf8(payload).map_err(|_| failure("Asset manifest JSON is invalid"))?;
    let value = Value::parse(text).map_err(|_| failure("Asset manifest JSON is invalid"))?;
    if !matches!(&value, Value::Object(fields) if fields.len() == 2 && value.has("schemaVersion") && value.has("assets"))
    {
        return Err(failure("Asset manifest shape is invalid"));
    }
    let Value::Array(assets) = value.get("assets") else {
        return Err(failure("Asset manifest schema is invalid"));
    };
    if !value.get("schemaVersion").number_eq(1) {
        return Err(failure("Asset manifest schema is invalid"));
    }
    let assets = assets
        .iter()
        .map(|asset| parse_asset(root, asset))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    for asset in &assets {
        if !ids.insert(asset.get("id").string()?) {
            return Err(failure("Asset ids must be unique"));
        }
        if !paths.insert(asset.get("path").string()?) {
            return Err(failure("Asset paths must be unique"));
        }
    }
    Ok(Value::object([
        ("schemaVersion", Value::integer(1)),
        ("assets", Value::Array(assets)),
    ]))
}

fn asset_keys(root: &Path, manifest: &Value) -> Result<Vec<FileKey>, Error> {
    let Value::Array(assets) = manifest.get("assets") else {
        return Err(failure("Asset manifest schema is invalid"));
    };
    assets
        .iter()
        .map(|asset| file_key(&contained(root, &asset.get("path").string()?)?))
        .collect()
}

impl ManifestLoader {
    pub fn load(&self, root: &Path) -> Result<Value, Error> {
        let root = fs::canonicalize(root).map_err(|_| failure("Asset manifest is missing"))?;
        let manifest_path = fs::canonicalize(root.join("generated/asset-manifest.json"))
            .map_err(|_| failure("Asset manifest is missing"))?;
        if !manifest_path.starts_with(&root) {
            return Err(failure("Asset manifest is missing"));
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| failure("asset manifest lock poisoned"))?;
        for _ in 0..3 {
            let before =
                file_key(&manifest_path).map_err(|_| failure("Asset manifest cannot be read"))?;
            if let Some(cached) = &*cache {
                if cached.key == before
                    && asset_keys(&root, &cached.manifest).is_ok_and(|keys| keys == cached.assets)
                {
                    return Ok(cached.manifest.clone());
                }
            }
            let payload =
                fs::read(&manifest_path).map_err(|_| failure("Asset manifest cannot be read"))?;
            let after =
                file_key(&manifest_path).map_err(|_| failure("Asset manifest cannot be read"))?;
            if before != after {
                continue;
            }
            let manifest = parse_manifest(&root, &payload)?;
            let assets = asset_keys(&root, &manifest)
                .map_err(|_| failure("Asset path changed after validation"))?;
            *cache = Some(CachedManifest {
                key: before,
                manifest: manifest.clone(),
                assets,
            });
            return Ok(manifest);
        }
        Err(failure("Asset manifest changed while reading"))
    }
}

pub(super) fn compact_json(value: &Value) -> Result<String, Error> {
    let encoded = json_utf8(value)?;
    let mut quoted = false;
    let mut escaped = false;
    Ok(encoded
        .chars()
        .filter(|c| {
            if quoted {
                if escaped {
                    escaped = false;
                } else if *c == '\\' {
                    escaped = true;
                } else if *c == '"' {
                    quoted = false;
                }
                true
            } else if *c == '"' {
                quoted = true;
                true
            } else {
                !c.is_whitespace()
            }
        })
        .collect())
}

pub(super) fn json_utf8(value: &Value) -> Result<String, Error> {
    openpilot_logmessaged::JsonValue::parse(&value.encode()?)
        .map_err(|error| Error::Source(error.to_string()))?
        .to_json_utf8()
        .map_err(|error| Error::Source(error.to_string()))
}

pub(super) fn inject(html: &str, manifest: &Value) -> Result<String, Error> {
    const PLACEHOLDER: &str =
        "<script id=\"carrotAssetManifest\" type=\"application/json\"></script>";
    if html.matches(PLACEHOLDER).count() != 1 {
        return Err(failure(
            "Asset manifest placeholder is missing or duplicated",
        ));
    }
    let payload = compact_json(manifest)?.replace('<', "\\u003c");
    Ok(html.replacen(
        PLACEHOLDER,
        &format!("<script id=\"carrotAssetManifest\" type=\"application/json\">{payload}</script>"),
        1,
    ))
}
