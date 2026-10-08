use super::request::{byte_range, condition, FileRequest};
use crate::{
    http::Body,
    http_response::{response, text},
    static_assets, Error,
};
use hyper::{header, Response, StatusCode};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

fn insert(result: &mut Response<Body>, name: header::HeaderName, value: &str) -> Result<(), Error> {
    result.headers_mut().insert(
        name,
        value
            .parse()
            .map_err(|_| Error::Source("invalid static header".into()))?,
    );
    Ok(())
}

fn stream_error(status: StatusCode, head: bool) -> Response<Body> {
    let mut result = response(status, Vec::new(), "application/octet-stream", head);
    result.headers_mut().remove(header::CONTENT_LENGTH);
    if !head {
        result.headers_mut().insert(
            header::TRANSFER_ENCODING,
            header::HeaderValue::from_static("chunked"),
        );
    }
    result
}

fn selected_file(path: &Path, request: &FileRequest) -> (PathBuf, Option<&'static str>) {
    let coding = request
        .header(header::ACCEPT_ENCODING)
        .unwrap_or_default()
        .to_lowercase();
    for (extension, encoding) in [("br", "br"), ("gz", "gzip")] {
        let candidate = path.with_file_name(format!(
            "{}.{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            extension
        ));
        if coding.contains(encoding)
            && fs::symlink_metadata(&candidate).is_ok_and(|meta| meta.is_file())
        {
            return (candidate, Some(encoding));
        }
    }
    (path.to_path_buf(), None)
}

pub(super) fn file_response(path: &Path, request: &FileRequest) -> Result<Response<Body>, Error> {
    let (selected, encoding) = selected_file(path, request);
    let metadata = match fs::metadata(&selected) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Ok(stream_error(
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::NOT_FOUND
                },
                request.head,
            ));
        }
    };
    if !metadata.is_file() {
        return Ok(stream_error(StatusCode::FORBIDDEN, request.head));
    }
    let etag = static_assets::etag(&metadata)?;
    if let Some(status) = condition(request, &etag, metadata.modified()?) {
        let mut result = response(status, Vec::new(), "application/octet-stream", request.head);
        result.headers_mut().remove(header::CONTENT_TYPE);
        if status == StatusCode::NOT_MODIFIED {
            result.headers_mut().remove(header::CONTENT_LENGTH);
            result.headers_mut().remove(header::CONTENT_TYPE);
            insert(&mut result, header::ETAG, &etag)?;
            insert(
                &mut result,
                header::LAST_MODIFIED,
                &static_assets::last_modified(&metadata)?,
            )?;
        }
        return Ok(result);
    }
    let mut file = match fs::File::open(&selected) {
        Ok(file) => file,
        Err(error) => {
            return Ok(stream_error(
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::NOT_FOUND
                },
                request.head,
            ));
        }
    };
    let metadata = file.metadata()?;
    let range = match byte_range(request, metadata.len(), metadata.modified()?) {
        Ok(range) => range,
        Err(()) => {
            let mut result = stream_error(StatusCode::RANGE_NOT_SATISFIABLE, request.head);
            insert(
                &mut result,
                header::CONTENT_RANGE,
                &format!("bytes */{}", metadata.len()),
            )?;
            return Ok(result);
        }
    };
    let (offset, count) = range.unwrap_or((0, metadata.len()));
    let mut bytes = Vec::new();
    if !request.head && count != 0 {
        file.seek(SeekFrom::Start(offset))?;
        file.take(count).read_to_end(&mut bytes)?;
    }
    let mut result = response(
        if range.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        },
        bytes,
        static_assets::content_type(path),
        request.head,
    );
    insert(&mut result, header::CONTENT_LENGTH, &count.to_string())?;
    insert(&mut result, header::ETAG, &static_assets::etag(&metadata)?)?;
    insert(
        &mut result,
        header::LAST_MODIFIED,
        &static_assets::last_modified(&metadata)?,
    )?;
    result.headers_mut().insert(
        header::ACCEPT_RANGES,
        header::HeaderValue::from_static("bytes"),
    );
    if let Some((start, count)) = range {
        insert(
            &mut result,
            header::CONTENT_RANGE,
            &format!("bytes {}-{}/{}", start, start + count - 1, metadata.len()),
        )?;
    }
    if let Some(encoding) = encoding {
        insert(&mut result, header::CONTENT_ENCODING, encoding)?;
        result.headers_mut().insert(
            header::VARY,
            header::HeaderValue::from_static("Accept-Encoding"),
        );
    }
    Ok(result)
}

fn resolve_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let root = fs::canonicalize(root).ok()?;
    let requested = root.join(relative);
    let mut suffix = Vec::new();
    let mut ancestor = requested.as_path();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut path) => {
                for part in suffix.iter().rev() {
                    path.push(part);
                }
                let mut normalized = PathBuf::new();
                for part in path.components() {
                    match part {
                        Component::ParentDir => {
                            normalized.pop();
                        }
                        Component::CurDir => {}
                        Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                            normalized.push(part.as_os_str())
                        }
                    }
                }
                return normalized.starts_with(root).then_some(normalized);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(ancestor.file_name()?.to_os_string());
                ancestor = ancestor.parent()?;
            }
            Err(_) => return None,
        }
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn quote_path(text: &str) -> String {
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const PATH: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'`')
        .add(b'{')
        .add(b'}');
    utf8_percent_encode(text, PATH).to_string()
}

fn directory(path: &Path, root: &Path, prefix: &str) -> Result<String, Error> {
    let root = fs::canonicalize(root)?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| Error::Source("static directory outside root".into()))?;
    let index = format!("Index of /{}", html_escape(&relative.to_string_lossy()));
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    let items = entries
        .into_iter()
        .map(|entry| {
            let relative = relative.join(entry.file_name());
            let quoted = quote_path(&format!("{prefix}/{}", relative.to_string_lossy()));
            let mut name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                name.push('/');
            }
            format!("<li><a href=\"{quoted}\">{}</a></li>", html_escape(&name))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "<html>\n<head>\n<title>{index}</title>\n</head>\n<body>\n<h1>{index}</h1>\n<ul>\n{items}\n</ul>\n</body>\n</html>"
    ))
}

pub(super) fn handle(
    config: &crate::config::Config,
    request: &FileRequest,
) -> Result<Response<Body>, Error> {
    let (root, relative, prefix, show_index) =
        if let Some(path) = request.path.strip_prefix("/shared-assets/") {
            (&config.shared_assets, path, "/shared-assets", false)
        } else if request.path == "/shared-assets" {
            (&config.shared_assets, "", "/shared-assets", false)
        } else if config.training_assets.is_dir()
            && (request.path == "/training" || request.path.starts_with("/training/"))
        {
            (
                &config.training_assets,
                request.path.strip_prefix("/training/").unwrap_or(""),
                "/training",
                false,
            )
        } else if config.shared_assets.is_dir()
            && (request.path == "/sound-assets" || request.path.starts_with("/sound-assets/"))
        {
            (
                &config.shared_assets,
                request.path.strip_prefix("/sound-assets/").unwrap_or(""),
                "/sound-assets",
                false,
            )
        } else {
            (
                &config.web,
                request.path.strip_prefix('/').unwrap_or(&request.path),
                "",
                true,
            )
        };
    let Some(path) = resolve_path(root, relative) else {
        return Ok(text(StatusCode::NOT_FOUND, "404: Not Found", request.head));
    };
    if path.is_dir() {
        if !show_index {
            return Ok(text(StatusCode::FORBIDDEN, "403: Forbidden", request.head));
        }
        return match directory(&path, root, prefix) {
            Ok(html) => Ok(response(
                StatusCode::OK,
                html.into_bytes(),
                "text/html; charset=utf-8",
                request.head,
            )),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                Ok(text(StatusCode::FORBIDDEN, "403: Forbidden", request.head))
            }
            Err(error) => Err(error),
        };
    }
    file_response(&path, request)
}
