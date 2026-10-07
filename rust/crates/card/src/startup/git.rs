pub fn format_git_source(remote: Option<&str>, branch: Option<&str>) -> String {
    let remote = remote.unwrap_or("");
    let remote = if remote.contains("://") {
        remote.to_owned()
    } else {
        format!("ssh://{}", remote.replacen(':', "/", 1))
    };
    let cleaned: String = remote
        .chars()
        .filter(|ch| !matches!(ch, '\t' | '\r' | '\n'))
        .collect();
    let owner = owner(cleaned.trim_start_matches(|ch: char| ch <= '\u{20}')).unwrap_or("unknown");
    format!(
        "{owner}/{}",
        branch
            .filter(|value| !value.is_empty())
            .unwrap_or("unknown")
    )
}

fn owner(remote: &str) -> Option<&str> {
    let (scheme, rest) = remote.split_once("://")?;
    if !scheme.chars().next()?.is_ascii_alphabetic()
        || !scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
    {
        return None;
    }
    let netloc_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if rest[..netloc_end].chars().any(nfkc_delimiter) {
        return None;
    }
    let host = rest[..netloc_end].rsplit('@').next()?;
    let host = if host.starts_with('[') {
        let end = host.find(']')?;
        let suffix = &host[end + 1..];
        if !suffix.is_empty() && !suffix.starts_with(':') {
            return None;
        }
        let address = &host[1..end];
        let address_value = address.split_once('%').map_or(address, |(value, _)| value);
        let scope_valid = address
            .split_once('%')
            .is_none_or(|(_, scope)| !scope.is_empty() && !scope.contains('%'));
        if !(ipv_future(address)
            || scope_valid && address_value.parse::<std::net::Ipv6Addr>().is_ok())
        {
            return None;
        }
        &host[1..end]
    } else {
        if host.contains(['[', ']']) {
            return None;
        }
        host.split(':').next()?
    };
    if host.is_empty() {
        return None;
    }
    let path = rest[netloc_end..]
        .split(['?', '#'])
        .next()?
        .trim_matches('/');
    let (owner, _) = path.split_once('/')?;
    Some(owner)
}

fn nfkc_delimiter(ch: char) -> bool {
    matches!(
        ch,
        '\u{2047}'
            | '\u{2048}'
            | '\u{2049}'
            | '\u{2100}'
            | '\u{2101}'
            | '\u{2105}'
            | '\u{2106}'
            | '\u{2a74}'
            | '\u{fe13}'
            | '\u{fe16}'
            | '\u{fe55}'
            | '\u{fe56}'
            | '\u{fe5f}'
            | '\u{fe6b}'
            | '\u{ff03}'
            | '\u{ff0f}'
            | '\u{ff1a}'
            | '\u{ff1f}'
            | '\u{ff20}'
    )
}

fn ipv_future(address: &str) -> bool {
    address
        .strip_prefix('v')
        .and_then(|value| value.split_once('.'))
        .is_some_and(|(version, value)| {
            !version.is_empty()
                && version.chars().all(|ch| ch.is_ascii_hexdigit())
                && !value.is_empty()
        })
}

#[cfg(test)]
mod tests {
    use super::format_git_source;

    #[test]
    fn bracketed_host_suffix_preserves_source_unknown_owner() {
        for host in ["[::1]suffix", "[::1]]", "[vF.test]suffix"] {
            assert_eq!(
                format_git_source(Some(&format!("https://{host}/owner/repo")), Some("dev")),
                "unknown/dev"
            );
        }
        assert_eq!(
            format_git_source(Some("https://[::1]:nonnumeric/owner/repo"), Some("dev")),
            "owner/dev"
        );
    }
}
