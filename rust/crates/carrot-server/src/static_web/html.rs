use crate::static_assets::Assets;
use std::path::Path;

fn ampersands(url: &str) -> String {
    let mut result = String::new();
    let mut remaining = url;
    while let Some(start) = remaining.find('&') {
        result.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let Some(end) = remaining.find(';') else {
            break;
        };
        let entity = remaining[1..end].to_ascii_lowercase();
        let numeric = entity
            .strip_prefix("#x")
            .is_some_and(|digits| digits.trim_start_matches('0') == "26")
            || entity
                .strip_prefix('#')
                .is_some_and(|digits| digits.trim_start_matches('0') == "38");
        if entity == "amp" || numeric {
            result.push('&');
            remaining = &remaining[end + 1..];
        } else {
            result.push('&');
            remaining = &remaining[1..];
        }
    }
    result.push_str(remaining);
    result
}

fn policy_path(path: &str) -> String {
    let replaced = path.trim_start_matches('/').replace('\\', "/");
    let mut pieces = Vec::new();
    for part in replaced.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                pieces.pop();
            }
            _ => pieces.push(part),
        }
    }
    format!("/{}", pieces.join("/"))
}

fn fingerprint_url(url: &str, root: &Path, assets: &Assets) -> Option<String> {
    let url = ampersands(url);
    let url = url
        .trim_start_matches(|c: char| u32::from(c) <= 32)
        .replace(['\t', '\r', '\n'], "");
    if url.starts_with("//") {
        return None;
    }
    if let Some((scheme, _)) = url.split_once(':') {
        if scheme
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"+-.".contains(&c))
        {
            return None;
        }
    }
    let (main, fragment) = url
        .split_once('#')
        .map_or((url.as_str(), None), |(main, part)| (main, Some(part)));
    let (path, query) = main.split_once('?').unwrap_or((main, ""));
    let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
    let policy = policy_path(&decoded);
    if ["/support-terminal-assets/", "/js/vendor/", "/css/vendor/"]
        .iter()
        .any(|prefix| policy.starts_with(prefix))
    {
        return None;
    }
    let fingerprint = assets.fingerprint(root, &decoded)?;
    let mut pairs = query
        .split('&')
        .filter(|pair| {
            let key = pair.split_once('=').map_or(*pair, |(key, _)| key);
            let decoded = url::form_urlencoded::parse(format!("{key}=").as_bytes())
                .next()
                .map(|(key, _)| key.into_owned());
            decoded.as_deref() != Some("v")
        })
        .filter(|pair| !query.is_empty() || !pair.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    pairs.push(format!("v={fingerprint}"));
    let mut rewritten = format!("{path}?{}", pairs.join("&"));
    if let Some(fragment) = fragment {
        rewritten.push('#');
        rewritten.push_str(fragment);
    }
    Some(rewritten.replace('&', "&amp;"))
}

fn rewrite_tag(tag: &str, root: &Path, assets: &Assets) -> String {
    let raw = tag.as_bytes();
    let mut index = 1;
    while index < raw.len() && !raw[index].is_ascii_whitespace() && !b"/>".contains(&raw[index]) {
        index += 1;
    }
    let mut replacements = Vec::new();
    while index < raw.len() {
        while index < raw.len() && raw[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= raw.len() || b"/>".contains(&raw[index]) {
            break;
        }
        let start = index;
        while index < raw.len()
            && !raw[index].is_ascii_whitespace()
            && !b"/>=".contains(&raw[index])
        {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let name = &tag[start..index];
        while index < raw.len() && raw[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= raw.len() || raw[index] != b'=' {
            continue;
        }
        index += 1;
        while index < raw.len() && raw[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= raw.len() {
            break;
        }
        let value_start;
        let value_end;
        if b"'\"".contains(&raw[index]) {
            let quote = raw[index];
            index += 1;
            value_start = index;
            while index < raw.len() && raw[index] != quote {
                index += 1;
            }
            value_end = index;
            if index < raw.len() {
                index += 1;
            }
        } else {
            value_start = index;
            while index < raw.len() && !raw[index].is_ascii_whitespace() && raw[index] != b'>' {
                index += 1;
            }
            value_end = index;
        }
        if name.eq_ignore_ascii_case("src") || name.eq_ignore_ascii_case("href") {
            if let Some(url) = fingerprint_url(&tag[value_start..value_end], root, assets) {
                replacements.push((value_start, value_end, url));
            }
        }
    }
    let mut result = String::new();
    let mut previous = 0;
    for (start, end, value) in replacements {
        result.push_str(&tag[previous..start]);
        result.push_str(&value);
        previous = end;
    }
    result.push_str(&tag[previous..]);
    result
}

pub(super) fn rewrite(html: &str, root: &Path, assets: &Assets) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    let lower = html.to_ascii_lowercase();
    while let Some(relative) = html[cursor..].find('<') {
        let start = cursor + relative;
        result.push_str(&html[cursor..start]);
        if html[start..].starts_with("<!--") {
            let end = html[start + 4..]
                .find("-->")
                .map_or(html.len(), |i| start + 4 + i + 3);
            result.push_str(&html[start..end]);
            cursor = end;
            continue;
        }
        if !html
            .as_bytes()
            .get(start + 1)
            .is_some_and(u8::is_ascii_alphabetic)
        {
            result.push('<');
            cursor = start + 1;
            continue;
        }
        let mut end = start + 1;
        let mut quote = None;
        while end < html.len() {
            let c = html.as_bytes()[end];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
            } else if b"'\"".contains(&c) {
                quote = Some(c);
            } else if c == b'>' {
                end += 1;
                break;
            }
            end += 1;
        }
        if end == html.len() && !html.ends_with('>') {
            result.push_str(&html[start..]);
            cursor = end;
            break;
        }
        let tag = &html[start..end];
        result.push_str(&rewrite_tag(tag, root, assets));
        cursor = end;
        let name = tag[1..]
            .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
            .next()
            .unwrap_or("");
        if (name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style"))
            && !tag.ends_with("/>")
        {
            let closing = format!("</{}", name.to_ascii_lowercase());
            let raw_end = lower[cursor..]
                .find(&closing)
                .map_or(html.len(), |i| cursor + i);
            result.push_str(&html[cursor..raw_end]);
            cursor = raw_end;
        }
    }
    result.push_str(&html[cursor..]);
    result
}
