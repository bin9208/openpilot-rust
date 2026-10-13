use openpilot_manager::Error;
use std::ffi::OsString;

pub fn host(value: Option<OsString>) -> Result<String, Error> {
    match value {
        None => Ok("https://api.commadotai.com".into()),
        Some(value) => value
            .into_string()
            .map_err(|_| Error::Contract("API_HOST is not UTF-8")),
    }
}

#[cfg(test)]
mod tests {
    use super::host;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn default_registration_endpoint_matches_source() {
        assert_eq!(host(None).unwrap(), "https://api.commadotai.com");
    }

    #[test]
    fn owned_recipient_override_is_preserved_exactly() {
        let value = "http://127.0.0.1:35000/owned-prefix/";
        assert_eq!(host(Some(value.into())).unwrap(), value);
    }

    #[test]
    fn unused_malformed_host_is_not_eagerly_parsed() {
        for value in ["", "invalid host", ":://not-a-url"] {
            assert_eq!(host(Some(value.into())).unwrap(), value);
        }
    }

    #[test]
    fn non_unicode_host_is_an_explicit_error() {
        assert!(matches!(
            host(Some(std::ffi::OsString::from_vec(vec![0xff]))),
            Err(openpilot_manager::Error::Contract("API_HOST is not UTF-8"))
        ));
    }
}
