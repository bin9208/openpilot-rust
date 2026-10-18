use crate::{config::Config, Error, Value};
use openpilot_dashcam_upload::metadata;
use openpilot_params::Params;
use openpilot_web_upload::{Environment, SessionMode};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

pub struct UploadHealth {
    repository: PathBuf,
    settings: PathBuf,
    params: Option<Params>,
}

fn source(error: impl std::fmt::Display) -> Error {
    Error::Source(error.to_string())
}

fn optional(name: &str) -> Result<String, Error> {
    match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(String::new()),
        Err(error) => Err(source(error)),
    }
}

fn environment() -> Result<Environment, Error> {
    Ok(Environment {
        upload_url: optional("CARROT_WEB_UPLOAD_URL")?,
        upload_token: optional("CARROT_WEB_UPLOAD_TOKEN")?,
        tmux_url: String::new(),
    })
}

impl UploadHealth {
    pub fn original(config: &Config, params: Option<Params>) -> Arc<Self> {
        Arc::new(Self {
            repository: config.repository.clone(),
            settings: config.state.join("web_settings.json"),
            params,
        })
    }

    pub(super) fn test(&self) -> Result<(u16, Value), Error> {
        if let Some(webdav) =
            openpilot_web_upload::WebDav::load(&self.settings.with_file_name("private_upload.json"))
                .map_err(source)?
        {
            let result = webdav.health();
            let ok = result.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
            return Ok((
                if ok { 200 } else { 502 },
                Value::parse(&result.to_string()).map_err(source)?,
            ));
        }
        let (base, token) =
            metadata::target_settings(&self.settings, &environment()?).map_err(source)?;
        let mut result = openpilot_web_upload::health(&base, &token);
        let ok = result.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
        if ok && token.is_empty() {
            let mut logger =
                openpilot_logging::producer::Logger::for_runtime_retained().map_err(source)?;
            let environment = ["CARROT_DEVICE_SERIAL", "DEVICE_SERIAL", "SERIAL"]
                .into_iter()
                .map(|key| optional(key).map(|value| (key.to_owned(), value)))
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let metadata = metadata::upload_metadata(
                self.params.as_ref(),
                &self.repository,
                &environment,
                &metadata::hardware_serial(),
                &mut logger,
            );
            openpilot_web_upload::create_session_with_purpose(
                &base,
                &metadata::fields(&metadata).map_err(source)?,
                "test",
                SessionMode::Async,
            )
            .map_err(source)?;
            result["session"] = serde_json::json!("automatic");
        }
        result["target"] = serde_json::json!("web");
        result["url"] = serde_json::json!(base);
        let payload =
            Value::parse(&serde_json::to_string(&result).map_err(source)?).map_err(source)?;
        Ok((if ok { 200 } else { 502 }, payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, net::TcpListener, os::unix::fs::PermissionsExt};

    #[test]
    fn private_config_failure_never_tests_a_default_or_legacy_api() {
        let root = tempfile::tempdir().unwrap();
        let trap = TcpListener::bind("127.0.0.1:0").unwrap();
        trap.set_nonblocking(true).unwrap();
        let settings = root.path().join("web_settings.json");
        fs::write(
            &settings,
            serde_json::json!({"web_upload_url":format!("http://{}",trap.local_addr().unwrap())})
                .to_string(),
        )
        .unwrap();
        let private = root.path().join("private_upload.json");
        fs::write(&private, "fixture-secret malformed JSON").unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
        let service = UploadHealth {
            repository: root.path().to_owned(),
            settings,
            params: None,
        };
        let error = service.test().unwrap_err();
        assert!(!error.to_string().contains("fixture-secret"));
        assert!(
            matches!(trap.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
