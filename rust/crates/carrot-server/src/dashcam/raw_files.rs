use super::{catalog, metadata, mime::DownloadMime, Failure, Service};
use crate::{Error, Value};
use hyper::header::{self, HeaderMap, HeaderValue};
use std::{fs, path::PathBuf};

fn header(value: &str) -> Result<HeaderValue, Failure> {
    HeaderValue::from_str(value).map_err(|error| Error::Source(error.to_string()).into())
}
pub(super) fn replay(
    service: &Service,
    segment: &Value,
    kind: &str,
) -> Result<(PathBuf, HeaderMap), Failure> {
    let segment = super::paths::safe_segment(segment)?;
    let directory = metadata::directory(service, &segment)?;
    let (path, name) = match kind {
        "rlog" => catalog::source_rlog(&directory)?,
        "qlog" => catalog::source_qlog(&directory)?,
        "video" => catalog::source_video(&directory)?,
        _ => return Err(Failure::http(404, "replay source not found")),
    };
    let content_type = if kind == "video" {
        if name.ends_with(".mp4") {
            "video/mp4"
        } else {
            "video/mp2t"
        }
    } else if name.ends_with(".zst") {
        "application/zstd"
    } else if name.ends_with(".bz2") {
        "application/x-bzip2"
    } else {
        "application/octet-stream"
    };
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok((path, headers))
}
pub(super) fn download(
    service: &Service,
    mime: &DownloadMime,
    segment: &Value,
    kind: &str,
) -> Result<(PathBuf, HeaderMap), Failure> {
    let directory = metadata::directory(service, segment)?;
    let names: &[&str] = match kind {
        "qcamera" => &["qcamera.ts", "qcamera.mp4"],
        "rlog" => &["rlog.zst", "rlog.bz2", "rlog"],
        "qlog" => &["qlog.zst", "qlog.bz2", "qlog"],
        _ => &[],
    };
    for name in names {
        let path = directory.join(name);
        if !fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let content_type = mime.content_type(name)?;
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, header(&content_type)?);
        let segment = segment.string().map_err(Error::from)?;
        headers.insert(
            header::CONTENT_DISPOSITION,
            header(&format!("attachment; filename=\"{segment}--{name}\""))?,
        );
        return Ok((path, headers));
    }
    Err(Failure::http(404, "artifact not found"))
}
