//! Independent owned-recipient representation of the unchanged upload
//! controller's JSON and multipart observations.

use super::{CapturedRequest, Reply, Result};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn parameter(disposition: &str, name: &str) -> Option<String> {
    disposition.split(';').skip(1).find_map(|entry| {
        let (key, value) = entry.trim().split_once('=')?;
        (key == name).then(|| value.trim_matches('"').to_owned())
    })
}

fn multipart(body: &[u8], content_type: &str) -> Result<Value> {
    let boundary = content_type
        .split(';')
        .find_map(|entry| entry.trim().strip_prefix("boundary="))
        .ok_or("missing recovery upload multipart boundary")?;
    let boundary = boundary.trim_matches('"');
    if boundary.is_empty() || boundary.len() > 200 || !boundary.is_ascii() {
        return Err("invalid recovery upload multipart boundary".into());
    }
    let marker = format!("--{boundary}").into_bytes();
    let filename_timestamp = Regex::new(r"\d{8}-\d{6}")?;
    let content_timestamp = Regex::new(r"- Time: \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}")?;
    let mut rest = body;
    let mut parts = Vec::new();
    loop {
        if !rest.starts_with(&marker) {
            return Err("missing recovery upload multipart marker".into());
        }
        rest = &rest[marker.len()..];
        if rest.starts_with(b"--\r\n") || rest == b"--" {
            break;
        }
        if !rest.starts_with(b"\r\n") {
            return Err("invalid recovery upload multipart part".into());
        }
        rest = &rest[2..];
        let head_end = rest
            .windows(4)
            .position(|slice| slice == b"\r\n\r\n")
            .ok_or("missing recovery upload part headers")?;
        let head = std::str::from_utf8(&rest[..head_end])?;
        rest = &rest[head_end + 4..];
        let mut delimiter = b"\r\n".to_vec();
        delimiter.extend_from_slice(&marker);
        let end = rest
            .windows(delimiter.len())
            .position(|slice| slice == delimiter)
            .ok_or("missing recovery upload part terminator")?;
        let raw = &rest[..end];
        rest = &rest[end + 2..];
        let disposition = head
            .split("\r\n")
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("Content-Disposition")
                    .then(|| value.trim())
            })
            .ok_or("missing recovery upload content disposition")?;
        let name = parameter(disposition, "name").ok_or("missing recovery upload part name")?;
        if let Some(filename) = parameter(disposition, "filename") {
            let filename = filename_timestamp
                .replace_all(&filename, "TIMESTAMP")
                .to_string();
            parts.push(json!({"name":name,"filename":filename,"size":raw.len(),"sha256":format!("{:x}",Sha256::digest(raw))}));
        } else if name == "payload_json" {
            let mut payload: Value = serde_json::from_slice(raw)?;
            let content = payload["content"]
                .as_str()
                .ok_or("missing upload content")?;
            let normalized = content_timestamp
                .replace_all(content, "- Time: TIMESTAMP")
                .to_string();
            payload["content"] = normalized.into();
            parts.push(json!({"name":name,"json":payload}));
        } else {
            parts.push(json!({"name":name,"text":std::str::from_utf8(raw)?}));
        }
    }
    Ok(Value::Array(parts))
}

pub fn normalized(request: &CapturedRequest) -> Result<Value> {
    let body = if request.method == "GET" {
        Value::Null
    } else {
        let content_type = header(&request.headers, "content-type")
            .ok_or("missing recovery upload content type")?;
        if content_type.starts_with("application/json") {
            serde_json::from_slice(&request.body)?
        } else {
            multipart(&request.body, content_type)?
        }
    };
    Ok(json!({
        "path":request.path,
        "authorization":header(&request.headers,"authorization"),
        "agent":header(&request.headers,"user-agent"),
        "accept":header(&request.headers,"accept"),
        "body":body,
    }))
}

pub fn scripted_response(mode: &str, method: &str, path: &str) -> Reply {
    let mut status = 200;
    let mut body = b"{\"ok\":true,\"token\":\"owned-token\"}".to_vec();
    if method != "GET" {
        if mode == "session-denied" && path.ends_with("/session") {
            status = 403;
            body = b"session denied".to_vec();
        }
        if mode == "missing-token" && path.ends_with("/session") {
            body = b"{\"ok\":true}".to_vec();
        }
        if mode == "non-json" {
            body = b"accepted".to_vec();
        }
        if mode == "partial" && path == "/logs" {
            status = 503;
            body = b"logs unavailable".to_vec();
        }
        if mode == "all-failed" {
            status = 500;
            body = "거부".repeat(600).into_bytes();
        }
        if mode == "empty-error" {
            status = 502;
            body.clear();
        }
        if matches!(mode, "redirect301" | "redirect307") {
            status = if mode == "redirect301" { 301 } else { 307 };
            body.clear();
        }
        if mode == "large-response" {
            body = b"{\"ok\":true,\"token\":\"".to_vec();
            body.extend(std::iter::repeat_n(b'x', 1024 * 1024));
            body.extend_from_slice(b"\"}");
        }
        if mode == "invalid-utf8-response" {
            body = vec![0xff];
        }
    }
    let mut reply = Reply::json(status, body);
    if status == 301 || status == 307 {
        reply
            .extra_headers
            .push(("Location".into(), format!("/redirect{path}")));
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn multipart_observation_preserves_binary_hash_and_only_normalizes_timestamp() {
        let mut body = b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"log-20260101-010203.bin\"\r\n\r\n".to_vec();
        body.extend_from_slice(&[0, 255]);
        body.extend_from_slice(b"\r\n--owned\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{\"content\":\"- Time: 2026-01-01 01:02:03\",\"other\":263}\r\n--owned--\r\n");
        let request = CapturedRequest {
            method: "POST".into(),
            path: "/logs".into(),
            headers: vec![(
                "content-type".into(),
                "multipart/form-data; boundary=owned".into(),
            )],
            body,
            elapsed: Duration::ZERO,
        };
        let value = normalized(&request).unwrap();
        let parts = value["body"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["filename"], "log-TIMESTAMP.bin");
        assert_eq!(parts[0]["size"], 2);
        assert_eq!(
            parts[0]["sha256"],
            format!("{:x}", Sha256::digest([0_u8, 255]))
        );
        assert_eq!(parts[1]["json"]["content"], "- Time: TIMESTAMP");
        assert_eq!(parts[1]["json"]["other"], 263);
    }

    #[test]
    fn redirect_scripts_only_admit_literal_owned_location() {
        let redirect = scripted_response("redirect307", "POST", "/logs");
        assert_eq!(redirect.status, 307);
        assert_eq!(
            redirect.extra_headers,
            vec![("Location".into(), "/redirect/logs".into())]
        );
        assert_eq!(
            scripted_response("redirect307", "GET", "/redirect/logs").status,
            200
        );
    }
}
