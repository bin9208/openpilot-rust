//! Browser sign-in and native auth CLI candidate from tools/lib/auth.py at 7a3727e44.
//! Copyright (c) 2018, Comma.ai, Inc.; repository MIT license applies.
//! Original Python and callers remain pending exact-source qualification.
use super::Result;
use openpilot_log_format::JsonValue;
use std::{collections::BTreeMap, time::Duration};

mod cli;
mod native;
mod pretty;
pub use native::auth_cli;
pub use native::auth_cli as auth_json_cli;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    Apple,
    Github,
}
impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Google => "g",
            Self::Apple => "a",
            Self::Github => "h",
        }
    }
}
pub fn auth_redirect_link(provider: Provider, port: u16) -> String {
    let redirect = format!("https://api.comma.ai/v2/auth/{}/redirect/", provider.id());
    let state = format!("service,localhost:{port}");
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("redirect_uri", &redirect)
        .append_pair("state", &state);
    let base = match provider {
        Provider::Google => {
            query.extend_pairs([
                ("type", "web_server"),
                (
                    "client_id",
                    "45471411055-ornt4svd2miog6dnopve7qtmh5mnu6id.apps.googleusercontent.com",
                ),
                ("response_type", "code"),
                ("scope", "https://www.googleapis.com/auth/userinfo.email"),
                ("prompt", "select_account"),
            ]);
            "https://accounts.google.com/o/oauth2/auth?"
        }
        Provider::Github => {
            query.extend_pairs([
                ("client_id", "28c4ecb54bb7272cb5a4"),
                ("scope", "read:user"),
            ]);
            "https://github.com/login/oauth/authorize?"
        }
        Provider::Apple => {
            query.extend_pairs([
                ("client_id", "ai.comma.login"),
                ("response_type", "code"),
                ("response_mode", "form_post"),
                ("scope", "name email"),
            ]);
            "https://appleid.apple.com/auth/authorize?"
        }
    };
    format!("{base}{}", query.finish())
}

#[derive(Clone, Default)]
pub struct Callback {
    query: BTreeMap<String, Vec<String>>,
}
impl Callback {
    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }
    /// Accept only the first /auth or /auth/ result, including empty error/code values.
    /// Decode form data with replacement, retaining blanks and repeated fields like parse_qs.
    pub fn receive(&mut self, target: &str) -> u16 {
        let target = target.split('#').next().unwrap_or(target);
        // HTTP absolute-form requests are accepted by BaseHTTPRequestHandler too.
        let target = if let Some((_, rest)) = target
            .split_once("://")
            .filter(|(scheme, _)| !scheme.contains(['/', '?']))
        {
            rest.find('/').map_or("/", |index| &rest[index..])
        } else {
            target
        };
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if !matches!(path, "/auth" | "/auth/") {
            return 204;
        }
        if !self.is_empty() {
            return 200;
        }
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            values
                .entry(key.into_owned())
                .or_default()
                .push(value.into_owned());
        }
        if values.contains_key("code") || values.contains_key("error") {
            self.query = values;
        }
        200
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum LoginStatus {
    Success,
    Error(&'static str),
}
impl LoginStatus {
    pub fn to_json(&self) -> String {
        match self {
            Self::Success => "{\"success\": true}".into(),
            Self::Error(error) => format!(
                "{{\"error\": {}}}",
                JsonValue::text(error).to_json().expect("static text")
            ),
        }
    }
}
/// External effects are supplied separately so CI never discovers real credentials,
/// launches a browser, authenticates remotely or changes a user's auth.json.
pub trait LoginIo {
    fn start(&mut self, provider: Provider) -> Result<()>;
    fn now(&self) -> Duration;
    fn poll_callback(&mut self, wait: Duration) -> Result<Callback>;
    fn browser_failed(&mut self) -> Result<bool>;
    fn exchange(&mut self, code: &str, provider: &str) -> Result<JsonValue>;
    fn verify(&mut self, token: &str) -> Result<()>;
}
fn python_blank(value: &str) -> bool {
    // CPython str.strip additionally treats U+001C..U+001F as whitespace.
    value
        .chars()
        .all(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}
pub fn login_with(
    io: &mut impl LoginIo,
    provider: Provider,
    timeout: Duration,
    mut save: impl FnMut(&str) -> Result<()>,
) -> LoginStatus {
    let mut attempt = || -> Result<LoginStatus> {
        io.start(provider)?;
        let deadline = io.now().saturating_add(timeout);
        while io.now() < deadline {
            let callback = io.poll_callback(
                deadline
                    .saturating_sub(io.now())
                    .min(Duration::from_millis(100)),
            )?;
            if callback.query.contains_key("error") {
                return Ok(LoginStatus::Error(
                    "Sign-in was declined. Choose a provider to try again.",
                ));
            }
            if let Some(codes) = callback.query.get("code") {
                if codes.len() != 1
                    || python_blank(&codes[0])
                    || callback.query.get("provider").map(Vec::as_slice)
                        != Some(&[provider.id().to_string()][..])
                {
                    return Ok(LoginStatus::Error(
                        "Invalid sign-in response. Please try again.",
                    ));
                }
                let response = io.exchange(&codes[0], provider.id())?;
                let Some(token) = response.to_utf8().filter(|token| !python_blank(token)) else {
                    return Ok(LoginStatus::Error(
                        "Sign-in did not return an access token. Please try again.",
                    ));
                };
                io.verify(&token)?;
                save(&token)?;
                return Ok(LoginStatus::Success);
            }
            if io.browser_failed()? {
                return Ok(LoginStatus::Error(
                    "Could not open your browser. Check your default browser and try again.",
                ));
            }
        }
        Ok(LoginStatus::Error(
            "Sign-in timed out. Choose a provider to try again.",
        ))
    };
    attempt().unwrap_or(LoginStatus::Error(
        "Could not complete sign-in. Check your connection and try again.",
    ))
}

/// Legacy JSON-only options helper retained for earlier callers/tests.
/// Native entrypoints use the private full-mode CLI parser instead.
pub fn parse_json_options(args: &[String]) -> Result<Option<Provider>> {
    let mut positionals = vec![];
    let mut json = false;
    let mut literal = false;
    for arg in args {
        if !literal && arg == "--" {
            literal = true;
        } else if !literal && matches!(arg.as_str(), "-h" | "--help") {
            return Ok(None);
        } else if !literal && arg.starts_with("--") && "--json".starts_with(arg) {
            json = true;
        } else if !literal && arg.starts_with('-') && arg != "-" {
            return Err("unrecognized auth option".into());
        } else {
            positionals.push(arg.as_str());
        }
    }
    if !json {
        return Err("this browser adapter requires --json; plain/JWT auth is not ported".into());
    }
    if positionals.len() > 2 {
        return Err("unrecognized auth arguments".into());
    }
    Ok(Some(
        match positionals.first().copied().unwrap_or("google") {
            "google" => Provider::Google,
            "apple" => Provider::Apple,
            "github" => Provider::Github,
            "jwt" => return Err("--json requires a browser sign-in provider".into()),
            _ => return Err("invalid provider: choose google, apple or github".into()),
        },
    ))
}
