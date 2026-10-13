use super::{
    format::{self, Files},
    multipart, snapshot, text, Config,
};
use crate::{diagnostic_metadata, stream_proxy::History, Error, Value};
use openpilot_dashcam_upload::metadata;
use openpilot_params::Params;
use std::time::Duration;

include!(concat!(env!("OUT_DIR"), "/vision_diag_defaults.rs"));

pub(super) struct Upload {
    pub bundle: String,
    pub filename: String,
    pub console: String,
    pub console_filename: String,
    pub source: String,
}
impl Upload {
    pub fn parse(body: &Value) -> Result<Self, Error> {
        if !matches!(body, Value::Object(_)) {
            return Err(Error::Source("diagnostic upload must be an object".into()));
        }
        let fallback = |first: &str, second: &str| {
            if body.get(first).truth() {
                body.get(first)
            } else {
                body.get(second)
            }
        };
        let source = text::input(body.get("source"))?;
        Ok(Self {
            bundle: text::input(fallback("bundle", "text"))?,
            filename: text::input(body.get("filename"))?,
            console: text::input(body.get("console"))?,
            console_filename: text::input(fallback("consoleFilename", "console_filename"))?,
            source: if source.is_empty() {
                "web".into()
            } else {
                source
            },
        })
    }
}
fn url(params: Option<&Params>) -> Result<String, Error> {
    for key in [
        "CARROT_VISION_DIAG_DISCORD_WEBHOOK_URL",
        "CARROT_DISCORD_WEBHOOK_URL",
        "DISCORD_WEBHOOK_URL",
    ] {
        let value = std::env::var(key).unwrap_or_default();
        let value = value.trim_matches(crate::tools::text::whitespace);
        if !value.is_empty() {
            return Ok(value.into());
        }
    }
    let mut logger = openpilot_logging::producer::Factory::for_runtime()
        .map_err(|error| Error::Source(error.to_string()))?
        .logger();
    for key in [
        "CarrotVisionDiagDiscordWebhookUrl",
        "CarrotVisionDiagDiscordWebhookURL",
        "CarrotDiscordWebhookUrl",
        "CarrotDiscordWebhookURL",
        "DiscordWebhookUrl",
        "DiscordWebhookURL",
    ] {
        let value = metadata::param_text(params, key, "", &mut logger);
        if !value.is_empty() {
            return Ok(value);
        }
    }
    let disabled = std::env::var("CARROT_VISION_DIAG_DISCORD_WEBHOOK_DISABLE").unwrap_or_default();
    if matches!(
        disabled
            .trim_matches(crate::tools::text::whitespace)
            .to_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    ) {
        return Ok(String::new());
    }
    Ok(metadata::decode_obfuscated(DEFAULT_WEBHOOK, DEFAULT_KEY))
}
fn result(files: &Files, outcome: Result<u16, String>, source: &str) -> Value {
    let mut fields = vec![
        ("configured", Value::Bool(true)),
        ("ok", Value::Bool(outcome.is_ok())),
        ("filename", Value::text(&files.name)),
        ("bytes", Value::integer(files.bytes.len())),
        (
            "console_filename",
            if files.console_name.is_empty() {
                Value::Null
            } else {
                Value::text(&files.console_name)
            },
        ),
        ("console_bytes", Value::integer(files.console_bytes.len())),
    ];
    match outcome {
        Ok(status) => fields.extend([
            ("status", Value::integer(status)),
            ("source", Value::text(source)),
        ]),
        Err(error) => fields.push(("error", Value::text(&error))),
    }
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.chars().map(u32::from).collect(), value))
            .collect(),
    )
}
async fn post(url: &str, body: multipart::Body) -> Result<(u16, String), Error> {
    let client = crate::native_http::builder()?
        .cookie_store(true)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| Error::Source(error.to_string()))?;
    let response = client
        .post(url)
        .header("Content-Type", body.content_type)
        .header("Accept-Encoding", "gzip, deflate, br")
        .body(body.bytes)
        .send()
        .await
        .map_err(|error| Error::Source(error.to_string()))?;
    let status = response.status().as_u16();
    let encoding = crate::request_text::encoding(response.headers());
    let mut decoder = crate::request_body::decoder::Decoder::new(response.headers());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| Error::Source(error.to_string()))?;
    let bytes = decoder.feed(&bytes)?;
    decoder.finish()?;
    Ok((
        status,
        crate::request_text::decode(&bytes, &encoding)?.into_owned(),
    ))
}
pub(super) async fn send(config: Config, history: History, input: Upload) -> Result<Value, Error> {
    let params = config.params()?;
    let url = url(params.as_ref())?;
    if url.is_empty() {
        return Ok(Value::object([
            ("configured", Value::Bool(false)),
            ("ok", Value::Bool(false)),
            ("skipped", Value::Bool(true)),
        ]));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Ok(Value::object([
            ("configured", Value::Bool(true)),
            ("ok", Value::Bool(false)),
            ("error", Value::text("invalid webhook url")),
        ]));
    }
    let snapshot = snapshot::collect(config.clone(), history).await?;
    let (runner, _running) = config.runner();
    let metadata = diagnostic_metadata::collect(&config.repository, params, &runner).await?;
    let name = format::filename(&input.filename);
    let bundle = format!("{}\n\n# ===== COMMA SERVER SNAPSHOT AT DISCORD UPLOAD =====\n{}\n# ===== END COMMA SERVER SNAPSHOT AT DISCORD UPLOAD =====", input.bundle, format::pretty(&snapshot, 0)?);
    let (console_name, console_bytes) = if input
        .console
        .trim_matches(crate::tools::text::whitespace)
        .is_empty()
    {
        (String::new(), Vec::new())
    } else {
        (
            format::console_filename(&input.console_filename, &name),
            format::limit(input.console).into_bytes(),
        )
    };
    let files = Files {
        name,
        bytes: format::limit(bundle).into_bytes(),
        console_name,
        console_bytes,
    };
    let payload = Value::object([
        ("username", Value::text("Carrot Vision")),
        (
            "content",
            Value::text(&format::message(&snapshot, &metadata, &files)),
        ),
        (
            "allowed_mentions",
            Value::object([("parse", Value::Array(Vec::new()))]),
        ),
        ("flags", Value::integer(4)),
    ]);
    let body = multipart::encode(&format::pretty(&payload, 0)?, &files);
    Ok(match post(&url, body).await {
        Ok((status, _)) if (200..300).contains(&status) => {
            result(&files, Ok(status), &input.source)
        }
        Ok((status, text)) => {
            let mut value = result(
                &files,
                Err(text.chars().take(1000).collect()),
                &input.source,
            );
            crate::json_fields::set(&mut value, "status", Value::integer(status))?;
            value
        }
        Err(error) => result(&files, Err(error.to_string()), &input.source),
    })
}
