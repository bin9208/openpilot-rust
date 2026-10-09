//! urlsplit/urlunsplit preserves path/query text; Url is used only for host/port validation.
use crate::Error;

pub(super) fn split(value: &str) -> String {
    value
        .trim_start_matches(|character: char| u32::from(character) <= 32)
        .replace(['\r', '\n', '\t'], "")
}
pub(super) fn prepare(value: &str, local_port: u16) -> Result<(String, String), Error> {
    let (_, authority_path) = value
        .split_once("://")
        .ok_or_else(|| Error::Source("RTMPS host is missing".into()))?;
    let end = authority_path
        .find(['/', '?', '#'])
        .unwrap_or(authority_path.len());
    let authority = &authority_path[..end];
    let path_query = authority_path[end..].split('#').next().unwrap_or_default();
    let path_query = match path_query.split_once('?') {
        Some((path, "")) => path,
        _ => path_query,
    };
    let path = path_query.split('?').next().unwrap_or_default();
    let app = path
        .split('/')
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    Ok((
        format!("rtmp://127.0.0.1:{local_port}{path_query}"),
        format!("rtmps://{authority}/{app}"),
    ))
}
