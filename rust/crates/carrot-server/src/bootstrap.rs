use crate::{
    http::Application,
    web_settings::{capability_client_spec, resolve_capabilities, WebSettings},
    Error, Value,
};

fn device_languages(app: &Application) -> Value {
    let result = (|| {
        let path = app
            .config
            .repository
            .join("openpilot/selfdrive/ui/translations/languages.json");
        let Value::Object(mapping) = Value::parse(&std::fs::read_to_string(path).ok()?).ok()?
        else {
            return None;
        };
        Some(Value::Array(
            mapping
                .into_iter()
                .map(|(name, code)| Value::object([("code", code), ("name", Value::Text(name))]))
                .collect(),
        ))
    })();
    result.unwrap_or_else(|| Value::Array(Vec::new()))
}

pub(crate) fn payload(app: &Application) -> Result<Value, Error> {
    let (device_language, sound_language, intro) = {
        let params = app
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        (
            params.get("LanguageSetting", &Value::text("")),
            params.get("SoundLanguageSetting", &Value::text("auto")),
            app.intro.bootstrap(&params).unwrap_or_else(|_| {
                Value::object([
                    ("shouldShow", Value::Bool(false)),
                    ("reason", Value::text("bootstrap_error")),
                ])
            }),
        )
    };
    let settings = WebSettings::new(
        &app.config.state.join("web_settings.json"),
        &app.config
            .web
            .join("src/features/drive/core/content_catalog.json"),
    );
    let web_settings = settings.read()?;
    Ok(Value::object([
        ("webSettings", web_settings.clone()),
        ("webSettingsSpec", settings.client_spec()),
        ("webCapabilities", resolve_capabilities(&web_settings)),
        ("webCapabilitiesSpec", capability_client_spec()),
        ("deviceLanguage", device_language),
        ("soundLanguage", sound_language),
        ("deviceLanguages", device_languages(app)),
        ("intro", intro),
    ]))
}
