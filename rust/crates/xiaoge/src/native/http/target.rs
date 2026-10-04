pub struct Target<'a> {
    pub path: &'a str,
    pub query: &'a str,
}

impl<'a> Target<'a> {
    pub fn parse(mut value: &'a str) -> Self {
        if value.starts_with("//") {
            value = &value[value.len() - value.trim_start_matches('/').len() - 1..];
        }
        let mut scheme = "";
        if let Some((head, tail)) = value.split_once(':') {
            if head
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic())
                && head
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
            {
                scheme = head;
                value = tail;
            }
        }
        if let Some(authority) = value.strip_prefix("//") {
            value = authority
                .find(['/', '?', '#'])
                .map_or("", |start| &authority[start..]);
        }
        value = value.split_once('#').map_or(value, |(path, _)| path);
        let (mut path, query) = value.split_once('?').unwrap_or((value, ""));
        if [
            "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtsps",
            "rtspu", "sip", "sips", "mms", "sftp", "tel",
        ]
        .iter()
        .any(|candidate| scheme.eq_ignore_ascii_case(candidate))
        {
            let start = path.rfind('/').unwrap_or(0);
            if let Some(offset) = path[start..].find(';') {
                path = &path[..start + offset];
            }
        }
        Self { path, query }
    }
}

#[cfg(test)]
mod tests {
    use super::Target;

    #[test]
    fn route_keeps_dot_segments_and_encoded_characters() {
        for path in [
            "/x/../api/config",
            "/api/./config",
            "/api/%63onfig",
            "/api;params/config",
        ] {
            assert_eq!(Target::parse(path).path, path);
        }
    }

    #[test]
    fn original_path_parameters_and_request_slashes_are_preserved() {
        for path in [
            "/api/config;params",
            "//api/config",
            "///api/config",
            "http://example.invalid/api/config;params",
        ] {
            assert_eq!(Target::parse(path).path, "/api/config");
        }
        let target = Target::parse("/api/snapshot;ignored?stream=road#fragment");
        assert_eq!(
            (target.path, target.query),
            ("/api/snapshot", "stream=road")
        );
    }
}
