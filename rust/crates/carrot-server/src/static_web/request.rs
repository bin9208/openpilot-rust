use hyper::{header, HeaderMap, Method, Request, StatusCode};
use std::{
    borrow::Cow,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) struct FileRequest {
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub head: bool,
    pub get: bool,
}

impl FileRequest {
    pub fn from_request<T>(request: &Request<T>) -> Self {
        let mut headers = request.headers().clone();
        if let Some(original) = request
            .extensions()
            .get::<hyper::ext::RawConditionalHeaders>()
        {
            for name in [
                header::RANGE,
                header::IF_RANGE,
                header::IF_MATCH,
                header::IF_NONE_MATCH,
                header::IF_MODIFIED_SINCE,
                header::IF_UNMODIFIED_SINCE,
            ] {
                if let Some(value) = original.get(&name) {
                    headers.insert(name, value.clone());
                }
            }
        }
        Self {
            path: percent_encoding::percent_decode_str(request.uri().path())
                .decode_utf8_lossy()
                .into_owned(),
            query: request.uri().query().map(str::to_owned),
            headers,
            head: request.method() == Method::HEAD,
            get: request.method() == Method::GET,
        }
    }

    pub fn header(&self, name: header::HeaderName) -> Option<Cow<'_, str>> {
        Some(String::from_utf8_lossy(self.headers.get(name)?.as_bytes()))
    }

    pub fn date(&self, name: header::HeaderName) -> Option<SystemTime> {
        parse_date(&self.header(name)?)
    }
}

fn parse_date(text: &str) -> Option<SystemTime> {
    if let Ok(date) = httpdate::parse_http_date(text) {
        return Some(date);
    }
    let text = text.split_once(',').map_or(text, |(_, date)| date).trim();
    let text = if let Some((date, zone)) = text.rsplit_once(' ') {
        if zone.bytes().all(|c| c.is_ascii_alphabetic()) || zone.starts_with(['+', '-']) {
            date
        } else {
            text
        }
    } else {
        text
    };
    let text = text
        .split_once(' ')
        .filter(|(weekday, _)| ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(weekday))
        .map_or(text, |(_, date)| date);
    for format in [
        "%d %b %Y %H:%M:%S",
        "%d %b %y %H:%M:%S",
        "%d %b %Y %H:%M",
        "%d-%b-%y %H:%M:%S",
        "%b %d %H:%M:%S %Y",
    ] {
        if let Ok(date) = chrono::NaiveDateTime::parse_from_str(text, format) {
            let seconds = date.and_utc().timestamp();
            let interval = Duration::from_secs(seconds.unsigned_abs());
            return if seconds < 0 {
                UNIX_EPOCH.checked_sub(interval)
            } else {
                UNIX_EPOCH.checked_add(interval)
            };
        }
    }
    None
}

fn etags_match(header: &str, etag: &str, weak: bool) -> bool {
    if header == "*" {
        return true;
    }
    let mut remaining = header;
    while !remaining.is_empty() {
        let is_weak = remaining.starts_with("W/");
        let token = remaining.strip_prefix("W/").unwrap_or(remaining);
        let Some(quoted) = token.strip_prefix('"') else {
            break;
        };
        let Some(end) = quoted.find('"') else {
            break;
        };
        let value = &quoted[..end];
        if value.is_empty()
            || !value
                .chars()
                .all(|c| c == '!' || ('#'..='~').contains(&c) || ('\u{80}'..='\u{ff}').contains(&c))
        {
            break;
        }
        let tail = &quoted[end + 1..];
        let following = if tail.is_empty() {
            ""
        } else if let Some(rest) = tail.trim_start().strip_prefix(',') {
            rest.trim_start()
        } else {
            break;
        };
        if (weak || !is_weak) && value == etag.trim_matches('"') {
            return true;
        }
        remaining = following;
    }
    false
}

pub(super) fn condition(
    request: &FileRequest,
    etag: &str,
    modified: SystemTime,
) -> Option<StatusCode> {
    let if_match = request
        .header(header::IF_MATCH)
        .filter(|value| !value.is_empty());
    if if_match
        .as_deref()
        .is_some_and(|value| !etags_match(value, etag, false))
    {
        return Some(StatusCode::PRECONDITION_FAILED);
    }
    if if_match.is_none()
        && request
            .date(header::IF_UNMODIFIED_SINCE)
            .is_some_and(|date| modified > date)
    {
        return Some(StatusCode::PRECONDITION_FAILED);
    }
    let if_none = request
        .header(header::IF_NONE_MATCH)
        .filter(|value| !value.is_empty());
    if if_none
        .as_deref()
        .is_some_and(|value| etags_match(value, etag, true))
        || (if_none.is_none()
            && request
                .date(header::IF_MODIFIED_SINCE)
                .is_some_and(|date| modified <= date))
    {
        return Some(StatusCode::NOT_MODIFIED);
    }
    None
}

pub(super) fn byte_range(
    request: &FileRequest,
    size: u64,
    modified: SystemTime,
) -> Result<Option<(u64, u64)>, ()> {
    if request
        .date(header::IF_RANGE)
        .is_some_and(|date| modified > date)
    {
        return Ok(None);
    }
    let Some(raw) = request.header(header::RANGE) else {
        return Ok(None);
    };
    let value = raw.strip_prefix("bytes=").ok_or(())?;
    let (start, end) = value.split_once('-').ok_or(())?;
    if !start.bytes().all(|c| c.is_ascii_digit()) || !end.bytes().all(|c| c.is_ascii_digit()) {
        return Err(());
    }
    let number = |text: &str| -> u64 { text.parse().unwrap_or(u64::MAX) };
    if start.is_empty() {
        if end.is_empty() {
            return Err(());
        }
        let suffix = number(end);
        let offset = if suffix == 0 {
            0
        } else {
            size.saturating_sub(suffix)
        };
        if offset >= size {
            return Err(());
        }
        return Ok(Some((offset, size - offset)));
    }
    let offset = number(start);
    let last = if end.is_empty() {
        u64::MAX
    } else {
        number(end)
    };
    if offset > last || offset >= size {
        return Err(());
    }
    Ok(Some((offset, last.saturating_add(1).min(size) - offset)))
}
