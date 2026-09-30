use crate::catalog::{decimal_digit, segment_index};
use percent_encoding::percent_decode_str;
use serde_json::Value;

fn text<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}
pub fn public_url(value: &str) -> String {
    let value = value.trim();
    let Some((scheme, rest)) = value.split_once("://") else {
        return value.into();
    };
    let scheme = scheme.to_ascii_lowercase();
    if !["http", "https"].contains(&scheme.as_str()) {
        return value.into();
    }
    let boundary = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..boundary];
    if authority.is_empty() || (authority.contains('[') != authority.contains(']')) {
        return value.into();
    }
    let tail = &rest[boundary..];
    let path_end = tail.find(['?', '#']).unwrap_or(tail.len());
    let decoded = percent_decode_str(&tail[..path_end]).decode_utf8_lossy();
    let mut quoted = String::new();
    for byte in decoded.bytes() {
        if byte.is_ascii_alphanumeric() || b"/:@-._~|".contains(&byte) {
            quoted.push(char::from(byte));
        } else {
            quoted.push('%');
            quoted.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
            quoted.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 15)]));
        }
    }
    let suffix = &tail[path_end..];
    let (query_part, fragment) = suffix.split_once('#').unwrap_or((suffix, ""));
    let query = query_part.strip_prefix('?').unwrap_or("");
    let mut output = format!("{scheme}://{authority}{quoted}");
    if !query.is_empty() {
        output.push('?');
        output.push_str(query);
    }
    if !fragment.is_empty() {
        output.push('#');
        output.push_str(fragment);
    }
    output
}
fn item_url(payload: &Value, item: &Value) -> String {
    let remote = text(item, "remotePath", "").trim();
    if !remote.is_empty() {
        return public_url(remote);
    }
    let base = text(payload, "remoteBasePath", "")
        .trim()
        .trim_end_matches('/');
    let segment = text(item, "segment", "").trim();
    if !base.is_empty() && !segment.is_empty() {
        public_url(&format!("{base}/{segment}"))
    } else {
        String::new()
    }
}
fn parts(item: &Value) -> Option<(&str, i64)> {
    let segment = text(item, "segment", "").trim();
    let (route, index) = segment.rsplit_once("--")?;
    if route.len() != 20
        || !route
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.|-".contains(&byte))
        || index.is_empty()
    {
        return None;
    }
    let parsed = segment_index(segment);
    let route_field = text(item, "route", route).trim();
    let index_field = item.get("segmentIndex").map_or(Some(parsed), |value| {
        value.as_i64().or_else(|| value.as_str()?.parse().ok())
    })?;
    if route_field != route || index_field != parsed {
        return None;
    }
    if !index.chars().all(|c| decimal_digit(c).is_some()) {
        return None;
    }
    Some((route, parsed))
}
fn success(item: &Value) -> bool {
    item.get("ok").and_then(Value::as_bool) == Some(true)
}
pub fn message_lines(payload: &Value, maximum: Option<usize>) -> Vec<String> {
    let meta = &payload["meta"];
    let commit = text(meta, "commit", "").trim();
    let commit_text = if commit.is_empty() || commit == "unknown" {
        "unknown".into()
    } else {
        format!("[{commit}](https://github.com/bin9208/openpilot-rust/commit/{commit})")
    };
    let all = payload
        .get("results")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut sorted: Vec<_> = all.iter().filter(|item| success(item)).collect();
    sorted.extend(all.iter().filter(|item| !success(item)));
    let visible = &sorted[..maximum.unwrap_or(sorted.len()).min(sorted.len())];
    let mut lines = vec![
        "# Carrot Dashcam Upload".into(),
        "### Upload".into(),
        format!("- Time: {}", text(payload, "uploadedAt", "")),
        format!(
            "- Path: {}",
            public_url(text(payload, "remoteBasePath", ""))
        ),
        "### Device".into(),
        format!("- Car name: {}", text(meta, "carName", "none")),
        format!("- DongleId: {}", text(meta, "dongleId", "unknown")),
        format!("- Serial: {}", text(meta, "serial", "unknown")),
        format!("- Branch: {}", text(meta, "branch", "unknown")),
        format!(
            "- Commit: {commit_text} ({})",
            text(meta, "commitDate", "unknown")
        ),
    ];
    let mut runs: Vec<Vec<&Value>> = Vec::new();
    let mut current: Vec<&Value> = Vec::new();
    for item in visible.iter().filter(|item| success(item)) {
        let consecutive = parts(item).is_some_and(|(route, index)| {
            current
                .last()
                .and_then(|last| parts(last))
                .is_some_and(|(last_route, last_index)| {
                    last_route == route && last_index.checked_add(1) == Some(index)
                })
        });
        if !consecutive {
            if current.len() > 1 {
                runs.push(std::mem::take(&mut current));
            }
            current.clear();
        }
        if parts(item).is_some() {
            current.push(item);
        }
    }
    if current.len() > 1 {
        runs.push(current);
    }
    if !runs.is_empty() {
        lines.push("### Open & Analyze".into());
        for run in runs {
            if let (Some((route, first)), Some((_, last))) =
                (parts(run[0]), parts(run[run.len() - 1]))
            {
                let first_url = item_url(payload, run[0]);
                if let (Some((prefix, _)), Some(end)) =
                    (first_url.rsplit_once('/'), last.checked_add(1))
                {
                    let url = public_url(&format!("{prefix}/{route}--{first}:{end}"));
                    lines.push(format!(
                        "- [Segments {first}–{last} ({} logs) · Web/Video/Tools]({url})",
                        run.len()
                    ));
                }
            }
        }
    }
    lines.push("### Result".into());
    for item in visible {
        let segment = text(item, "segment", "unknown");
        if success(item) {
            let url = item_url(payload, item);
            lines.push(if url.is_empty() {
                format!("- {segment} OK")
            } else {
                format!("- [{segment} OK · Open]({url})")
            });
        } else {
            let error = text(item, "error", "").trim();
            lines.push(if error.is_empty() {
                format!("- {segment} FAILED")
            } else {
                format!("- {segment} FAILED: {error}")
            });
        }
    }
    if sorted.len() > visible.len() {
        lines.push(format!("- ... +{} more", sorted.len() - visible.len()));
    }
    if sorted.is_empty() {
        lines.push("- none".into());
    }
    lines
}
pub fn share_text(payload: &Value) -> String {
    message_lines(payload, None).join("\n").trim().into()
}
pub fn discord_content(payload: &Value) -> String {
    for maximum in [24, 10, 3] {
        let content = message_lines(payload, Some(maximum))
            .join("\n")
            .trim()
            .to_owned();
        if content.chars().count() <= 1900 {
            return content;
        }
        if maximum == 3 {
            return content.chars().take(1900).collect();
        }
    }
    String::new()
}
