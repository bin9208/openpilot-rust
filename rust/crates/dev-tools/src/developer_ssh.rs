//! Source port of tools/scripts/{ssh,setup_ssh_keys}.py at 329e4d519.
//! Copyright (c) 2018, Comma.ai, Inc.; repository MIT license applies.
//! These opt-in entrypoints do not replace any caller or activate runtime SSH.
use openpilot_log_format::{JsonValue, JsonView};
use std::{fmt, io::Write, path::Path, time::Duration};

mod auth;
mod cli;
pub mod http;
mod integer;
pub use auth::{clear_token_at, config_root, get_token_at, set_token_at};
pub use cli::{parse_options, parse_options_with_limit, setup_cli, ssh_cli};
pub use integer::{
    parse_decimal, DecimalError, IntStringLimit, StartupLimitError, DEFAULT_MAX_STR_DIGITS,
    STR_DIGITS_CHECK_THRESHOLD,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub device: String,
    pub host: String,
    /// Normalized Python int text; do not constrain this to a TCP port range.
    pub port: String,
    pub key: String,
    pub debug: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Get {
    pub url: String,
    pub user_agent: &'static str,
    pub authorization: Option<String>,
    pub timeout: Option<Duration>,
}
pub struct Response {
    pub status: u16,
    /// Network content bytes after HTTP decompression, not character decoding.
    pub body: Vec<u8>,
    pub content_type: String,
}
/// Text access is separate from transport/status and may fail after earlier effects.
pub trait TextDecoder {
    fn decode(&mut self, bytes: &[u8], content_type: &str) -> Result<String>;
}
pub struct RequestsText;
impl TextDecoder for RequestsText {
    fn decode(&mut self, bytes: &[u8], content_type: &str) -> Result<String> {
        // Requests.text returns immediately for an empty body, before codec lookup.
        if bytes.is_empty() {
            return Ok(String::new());
        }
        // Shared UTF/single-byte paths replace malformed text; unknown codecs fall
        // back to UTF-8 replacement. Do not turn ordinary malformed bytes into
        // the injected dependency exception used to test side-effect ordering.
        // Broader codec/headers/JSON limitations remain in HANDOFF.md.
        Ok(openpilot_registration::response_text(bytes, content_type)?)
    }
}
pub trait Transport {
    fn get(&mut self, request: &Get) -> Result<Response>;
}
#[derive(Debug, PartialEq, Eq)]
pub struct LookupFailure {
    pub stderr: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    /// Includes trailing newlines from print, before process replacement.
    pub stdout: String,
    /// execvp's argv, including argv[0]; no local shell parsing.
    pub argv: Vec<String>,
}
impl Plan {
    /// Explicit invocation boundary. Synthetic tests must only inspect the plan.
    pub fn exec(self) -> Result<()> {
        use std::os::unix::process::CommandExt;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(self.stdout.as_bytes())?;
        stdout.flush()?;
        drop(stdout);
        let error = std::process::Command::new(&self.argv[0])
            .args(&self.argv[1..])
            .exec();
        Err(error.into())
    }
}

fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value.trim_start_matches('-').bytes().any(|b| b != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
fn python_str(value: &JsonValue) -> Result<String> {
    openpilot_runtime_version::python_str(value)?
        .into_iter()
        .map(|point| char::from_u32(point).ok_or_else(|| "unencodable surrogate in output".into()))
        .collect()
}

/// Minimal CommaApi GET dependency. No login/provider/set-token/clear-token port.
/// Even a non-2xx response is returned if it has no truthy JSON `error` field.
fn devices(transport: &mut impl Transport, host: &str, token: &JsonValue) -> Result<JsonValue> {
    let authorization = if truth(token) {
        Some(format!(
            "JWT {}",
            token
                .to_utf8()
                .ok_or("can only concatenate str to JWT prefix")?
        ))
    } else {
        None
    };
    let response = transport.get(&Get {
        url: format!("{host}/v1/me/devices"),
        user_agent: "OpenpilotTools",
        authorization,
        // CommaApi doesn't pass timeout to requests for this call.
        timeout: None,
    })?;
    let text = RequestsText.decode(&response.body, &response.content_type)?;
    let value = JsonValue::parse(&text)?;
    if let Some(api_error) = value.get("error").filter(truth) {
        if matches!(response.status, 401 | 403) {
            return Err("Unauthorized. Authenticate with tools/lib/auth.py".into());
        }
        // str(status) + ':' + description is a string concatenation in Python.
        let description = match value.get("description") {
            Some(value) => value.to_utf8().ok_or("API description must be str")?,
            None => python_str(&api_error)?,
        };
        return Err(format!("{}:{description}", response.status).into());
    }
    Ok(value)
}

pub fn prepare(
    transport: &mut impl Transport,
    api_host: &str,
    token: &JsonValue,
    options: &Options,
) -> Result<std::result::Result<Plan, LookupFailure>> {
    // Do not bypass the API or dict construction for direct IDs.
    plan(&devices(transport, api_host, token)?, options)
}

fn normalized(value: &str) -> String {
    // Python replaces U+0020 only, then lower() (not strip/casefold).
    value.replace(' ', "").to_lowercase()
}
pub fn plan(
    response: &JsonValue,
    options: &Options,
) -> Result<std::result::Result<Plan, LookupFailure>> {
    let JsonView::Array(rows) = response.view() else {
        return Err("device response must be a list".into());
    };
    // dict comprehension: duplicate IDs replace the value at the original position.
    let mut devices: Vec<(String, JsonValue)> = Vec::new();
    for row in rows {
        let id = row
            .get("dongle_id")
            .ok_or("missing dongle_id")?
            .to_utf8()
            .ok_or("dongle_id must be Unicode text")?;
        let alias = row.get("alias").ok_or("missing alias")?;
        if let Some((_, old)) = devices.iter_mut().find(|(key, _)| key == &id) {
            *old = alias;
        } else {
            devices.push((id, alias));
        }
    }
    // Actual re.match('[0-9a-zA-Z]{16}', input), with no end anchor.
    let direct = options
        .device
        .as_bytes()
        .get(..16)
        .is_some_and(|prefix| prefix.iter().all(u8::is_ascii_alphanumeric));
    let dongle_id = if direct {
        options.device.clone()
    } else {
        let input = normalized(&options.device);
        let matches = devices
            .iter()
            .filter(|(_, alias)| {
                alias
                    .to_utf8()
                    .is_some_and(|alias| normalized(&alias).contains(&input))
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            matches[0].0.clone()
        } else {
            let mut stderr = format!("failed to look up dongle id for \"{}\"\n", options.device);
            if matches.len() > 1 {
                stderr.push_str("found multiple matches:\n");
                for (id, alias) in matches {
                    stderr.push_str(&format!("  \"{}\" ({id})\n", python_str(alias)?));
                }
            }
            return Ok(Err(LookupFailure { stderr }));
        }
    };
    let name = match devices.iter().find(|(id, _)| id == &dongle_id) {
        Some((_, alias)) => format!("{} ({dongle_id})", python_str(alias)?),
        None => dongle_id.clone(),
    };
    let mut stdout = format!(
        "connecting to {name} through {}:{} ...\n",
        options.host, options.port
    );
    let mut argv = vec![
        "ssh".into(),
        "-i".into(),
        options.key.clone(),
        "-o".into(),
        format!(
            "ProxyCommand=ssh -i {} -W %h:%p -p %p %h@{}",
            options.key, options.host
        ),
        "-p".into(),
        options.port.clone(),
    ];
    if options.debug {
        argv.push("-v".into());
    }
    argv.push(format!("comma@comma-{dongle_id}"));
    if options.debug {
        stdout.push_str(&debug_command(&argv));
        stdout.push('\n');
    }
    Ok(Ok(Plan { stdout, argv }))
}

pub fn debug_command(argv: &[String]) -> String {
    // Intentionally the source's literal quoting, including unescaped apostrophes.
    argv.iter()
        .map(|arg| {
            if arg.contains(' ') {
                format!("'{arg}'")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub trait KeyParams {
    fn put(&mut self, key: &str, value: &[u8]) -> Result<()>;
}
// params_pyx.pyx discards the native int return code, including disk I/O failure.
// Preserve that behavior; construction and key/type exceptions still propagate.
impl KeyParams for openpilot_params::Params {
    fn put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        match openpilot_params::Params::put(self, key, value) {
            Ok(()) | Err(openpilot_params::Error::Io(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
/// Open Params only after HTTP 200; write separately, in original order.
pub fn setup_keys<P: KeyParams, F: FnOnce() -> Result<P>>(
    transport: &mut impl Transport,
    username: &str,
    open_params: F,
) -> Result<&'static str> {
    setup_keys_with_decoder(transport, username, &mut RequestsText, open_params)
}

/// Keep source evaluation order: bytes -> status -> Params -> enable -> text -> keys -> username.
/// A decoder exception must leave the enable effect and skip both string writes.
pub fn setup_keys_with_decoder<P: KeyParams, F: FnOnce() -> Result<P>>(
    transport: &mut impl Transport,
    username: &str,
    decoder: &mut impl TextDecoder,
    open_params: F,
) -> Result<&'static str> {
    let response = transport.get(&Get {
        url: format!("https://github.com/{username}.keys"),
        user_agent: "python-requests/2.34.2",
        authorization: None,
        timeout: Some(Duration::from_secs(10)),
    })?;
    if response.status != 200 {
        return Ok("Error getting public keys from github\n");
    }
    let mut params = open_params()?;
    params.put("SshEnabled", b"1")?;
    let text = decoder.decode(&response.body, &response.content_type)?;
    params.put("GithubSshKeys", text.as_bytes())?;
    params.put("GithubUsername", username.as_bytes())?;
    Ok("Set up ssh keys successfully\n")
}

#[derive(Debug)]
struct CliError(String);
impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CliError {}

/// The checkout root is a source path only, never a key-file read.
pub fn default_key(source_root: &Path) -> Result<String> {
    source_root
        .join("openpilot/system/hardware/tici/id_rsa")
        .into_os_string()
        .into_string()
        .map_err(|_| "source root is not UTF-8".into())
}
