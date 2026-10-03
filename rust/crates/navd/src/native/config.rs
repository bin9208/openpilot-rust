use super::{diagnostics, Options};
use crate::{route::Config, Error};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openpilot_logging::{producer::Logger, record::Level};
use openpilot_params::Params;
use openpilot_uploader::http::SigningKey;
use serde::Serialize;
use std::{
    fmt::Write,
    io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
struct Claims<'a> {
    identity: Option<&'a str>,
    nbf: u64,
    iat: u64,
    exp: u64,
}

fn unsigned_source_token(claims: &Claims<'_>) -> Result<String, Error> {
    let mut payload = String::new();
    for character in serde_json::to_string(claims)?.chars() {
        if character < '\u{7f}' {
            payload.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                write!(&mut payload, "\\u{unit:04x}")?;
            }
        }
    }
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    Ok(format!("{header}.{}.", URL_SAFE_NO_PAD.encode(payload)))
}

pub fn load(params: &Params, logger: &mut Logger, options: &Options) -> Result<Config, Error> {
    let (host, token) = match std::env::var("MAPBOX_TOKEN") {
        Ok(token) => ("https://api.mapbox.com", Some(token)),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Error::Runtime("MAPBOX_TOKEN is not UTF-8"))
        }
        Err(std::env::VarError::NotPresent) => {
            let raw = match params.get("PrimeType") {
                Ok(value) => value.unwrap_or_default(),
                Err(openpilot_params::Error::Io(_)) => Vec::new(),
                Err(error) => return Err(error.into()),
            };
            if openpilot_beepd::integer(&raw)? == 0 {
                (
                    "https://api.mapbox.com",
                    openpilot_params_typed::get_string(params, "MapboxPublicKey", logger)?,
                )
            } else {
                let persist = match &options.persist_root {
                    Some(path) => path.clone(),
                    None => PathBuf::from(
                        openpilot_hardware_info::paths::Paths::default().persist_root()?,
                    ),
                };
                let identity = openpilot_params_typed::get_string(params, "DongleId", logger)?;
                let token = match SigningKey::load(&persist) {
                    Ok(key) => {
                        let seconds = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_err(|_| Error::Runtime("JWT clock precedes epoch"))?
                            .as_secs();
                        let expiry = seconds
                            .checked_add(4 * 7 * 24 * 3600)
                            .ok_or(Error::Runtime("JWT expiration overflow"))?;
                        let claims = Claims {
                            identity: identity.as_deref(),
                            nbf: seconds,
                            iat: seconds,
                            exp: expiry,
                        };
                        match key {
                            Some(key) => key.token_claims(&claims)?,
                            None => unsigned_source_token(&claims)?,
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        diagnostics::exception(logger, "Failed to generate mapbox token due to missing private key. Ensure device is registered.", &error.to_string());
                        String::new()
                    }
                    Err(error) => return Err(error.into()),
                };
                ("https://maps.comma.ai", Some(token))
            }
        }
    };
    if options.mapbox_host.is_some() {
        diagnostics::text(
            logger,
            Level::Info,
            "navd HTTP fixture endpoint selected".into(),
        );
    }
    Ok(Config {
        host: options
            .mapbox_host
            .clone()
            .unwrap_or_else(|| host.to_owned()),
        token,
    })
}
