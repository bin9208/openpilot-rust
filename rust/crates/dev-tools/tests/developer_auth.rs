//! Synthetic-only login boundary regressions; run by the parent's Actions gate.
#![cfg(feature = "developer-ssh-native")]
use openpilot_dev_tools::developer_ssh::login::{
    auth_redirect_link, login_with, parse_json_options, Callback, LoginIo, LoginStatus, Provider,
};
use openpilot_dev_tools::developer_ssh::Result;
use openpilot_log_format::JsonValue;
use std::time::Duration;

struct InertLogin {
    query: Callback,
    token: JsonValue,
    fail: Option<&'static str>,
    events: Vec<String>,
    now: Duration,
}
impl InertLogin {
    fn new(query: &str) -> Self {
        let mut callback = Callback::default();
        callback.receive(query);
        Self {
            query: callback,
            token: JsonValue::text("inert-token"),
            fail: None,
            events: vec![],
            now: Duration::ZERO,
        }
    }
    fn event(&mut self, name: &str) -> Result<()> {
        self.events.push(name.into());
        if self.fail == Some(name) {
            Err("synthetic dependency failure".into())
        } else {
            Ok(())
        }
    }
}
impl LoginIo for InertLogin {
    fn start(&mut self, provider: Provider) -> Result<()> {
        self.event(&format!("start:{}", provider.id()))
    }
    fn now(&self) -> Duration {
        self.now
    }
    fn poll_callback(&mut self, wait: Duration) -> Result<Callback> {
        assert!(wait <= Duration::from_millis(100));
        self.now += wait;
        self.event("poll")?;
        Ok(self.query.clone())
    }
    fn browser_failed(&mut self) -> Result<bool> {
        self.event("browser")?;
        Ok(self.fail == Some("browser-exit"))
    }
    fn exchange(&mut self, code: &str, provider: &str) -> Result<JsonValue> {
        self.event(&format!("exchange:{code}:{provider}"))?;
        Ok(self.token.clone())
    }
    fn verify(&mut self, token: &str) -> Result<()> {
        self.event(&format!("verify:{token}"))
    }
}
fn run(io: &mut InertLogin, provider: Provider) -> (LoginStatus, Vec<String>) {
    let mut saved = vec![];
    let status = login_with(io, provider, Duration::from_secs(1), |token| {
        saved.push(token.to_owned());
        Ok(())
    });
    (status, saved)
}

#[test]
fn callback_admission_keeps_first_result_and_ignores_noise_paths() {
    let mut callback = Callback::default();
    callback.receive("/favicon.ico?error=no");
    callback.receive("/auth?provider=g");
    assert!(callback.is_empty());
    callback.receive("/auth/?code=one&provider=g");
    callback.receive("/auth?code=two&provider=h");
    let mut io = InertLogin::new("/ignored");
    io.query = callback;
    assert_eq!(
        run(&mut io, Provider::Google),
        (LoginStatus::Success, vec!["inert-token".into()])
    );
    assert!(io.events.contains(&"exchange:one:g".into()));
}

#[test]
fn all_providers_verify_before_saving_without_polling_browser_after_callback() {
    for (provider, id) in [
        (Provider::Google, "g"),
        (Provider::Apple, "a"),
        (Provider::Github, "h"),
    ] {
        let mut io = InertLogin::new(&format!("/auth?code=synthetic%2Bcode&provider={id}"));
        io.fail = Some("browser-exit");
        assert_eq!(
            run(&mut io, provider),
            (LoginStatus::Success, vec!["inert-token".into()])
        );
        assert_eq!(
            io.events,
            [
                format!("start:{id}"),
                "poll".into(),
                format!("exchange:synthetic+code:{id}"),
                "verify:inert-token".into()
            ]
        );
    }
}

#[test]
fn malformed_or_declined_callback_never_reaches_exchange_or_storage() {
    for query in [
        "error=",
        "error=denied&code=valid&provider=g",
        "code=&provider=g",
        "code=%20%1C&provider=g",
        "code=a&code=b&provider=g",
        "code=x&provider=h",
        "code=x",
        "code=x&provider=g&provider=g",
    ] {
        let mut io = InertLogin::new(&format!("/auth?{query}"));
        let (status, saved) = run(&mut io, Provider::Google);
        assert!(matches!(status, LoginStatus::Error(_)), "{query}");
        assert!(saved.is_empty());
        assert_eq!(io.events, ["start:g", "poll"]);
    }
}

#[test]
fn nonstring_blank_and_unverified_tokens_never_reach_storage() {
    for text in ["null", "123", "false", "[]", "{}", "\"\"", "\" \\u001c\""] {
        let mut io = InertLogin::new("/auth?code=x&provider=g");
        io.token = JsonValue::parse(text).unwrap();
        let (status, saved) = run(&mut io, Provider::Google);
        assert!(matches!(status, LoginStatus::Error(_)));
        assert!(saved.is_empty());
        assert_eq!(io.events.len(), 3);
    }
    let mut io = InertLogin::new("/auth?code=x&provider=g");
    io.fail = Some("verify:inert-token");
    assert!(
        matches!(run(&mut io, Provider::Google), (LoginStatus::Error(_), saved) if saved.is_empty())
    );
}

#[test]
fn timeout_browser_and_dependency_errors_are_json_status_not_success() {
    let mut io = InertLogin::new("/auth?code=x&provider=g");
    let status = login_with(&mut io, Provider::Google, Duration::ZERO, |_| {
        panic!("must not save")
    });
    assert_eq!(
        status,
        LoginStatus::Error("Sign-in timed out. Choose a provider to try again.")
    );
    assert_eq!(io.events, ["start:g"]);
    let mut io = InertLogin::new("/noise");
    io.fail = Some("browser-exit");
    let (status, saved) = run(&mut io, Provider::Google);
    assert!(matches!(status, LoginStatus::Error(text) if text.contains("browser")));
    assert!(saved.is_empty());
    for fail in ["start:g", "poll", "exchange:x:g"] {
        let mut io = InertLogin::new("/auth?code=x&provider=g");
        io.fail = Some(fail);
        assert!(
            matches!(run(&mut io, Provider::Google), (LoginStatus::Error(_), saved) if saved.is_empty())
        );
    }
    let mut io = InertLogin::new("/auth?code=x&provider=g");
    let status = login_with(&mut io, Provider::Google, Duration::from_secs(1), |_| {
        Err("synthetic storage failure".into())
    });
    assert!(matches!(status, LoginStatus::Error(_)));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&status.to_json()).unwrap()["error"],
        "Could not complete sign-in. Check your connection and try again."
    );
}

#[test]
fn json_cli_parser_defaults_provider_and_rejects_jwt_and_plain_mode() {
    let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        parse_json_options(&args(&["--json"])).unwrap(),
        Some(Provider::Google)
    );
    assert_eq!(
        parse_json_options(&args(&["github", "--js"])).unwrap(),
        Some(Provider::Github)
    );
    assert_eq!(
        parse_json_options(&args(&["apple", "unused", "--json"])).unwrap(),
        Some(Provider::Apple)
    );
    assert_eq!(parse_json_options(&args(&["--help"])).unwrap(), None);
    for items in [
        vec![],
        vec!["jwt", "inert", "--json"],
        vec!["bad", "--json"],
        vec!["--json=true"],
        vec!["--unknown"],
    ] {
        assert!(parse_json_options(&args(&items)).is_err());
    }
}

#[test]
fn provider_urls_preserve_source_redirect_state_and_form_values() {
    for (provider, host, id) in [
        (Provider::Google, "accounts.google.com", "g"),
        (Provider::Apple, "appleid.apple.com", "a"),
        (Provider::Github, "github.com", "h"),
    ] {
        let parsed = url::Url::parse(&auth_redirect_link(provider, 43210)).unwrap();
        assert_eq!(parsed.host_str(), Some(host));
        let query = parsed
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(query["state"], "service,localhost:43210");
        assert_eq!(
            query["redirect_uri"],
            format!("https://api.comma.ai/v2/auth/{id}/redirect/")
        );
        if provider == Provider::Apple {
            assert_eq!(query["scope"], "name email");
        }
    }
}

#[test]
fn original_login_and_urls_match_synthetic_native_status_and_effect_order() {
    // Execute the unchanged original only inside CI, with both imported credential/API
    // dependencies replaced before module loading. -I -S omits user/site initialization.
    // No socket binding, real browser, SSH, credential discovery or real API is possible.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../openpilot/tools/lib/auth.py");
    let script = r#"
import importlib.util, json, sys
from types import ModuleType
from unittest.mock import Mock
api = ModuleType('openpilot.tools.lib.api')
api.CommaApi = Mock()
api.UnauthorizedError = type('UnauthorizedError', (Exception,), {})
config = ModuleType('openpilot.tools.lib.auth_config')
config.set_token = Mock()
config.get_token = Mock(side_effect=AssertionError('credential access forbidden'))
sys.modules[api.__name__] = api
sys.modules[config.__name__] = config
spec = importlib.util.spec_from_file_location('_inert_auth', sys.argv[1])
auth = importlib.util.module_from_spec(spec)
spec.loader.exec_module(auth)
rows = []
for method, query, token, fail in json.loads(sys.argv[2]):
    events, saved = [], []
    server = Mock(server_port=43210, query_params={})
    def start(*args, **kwargs):
        events.append('start:' + {'google':'g','apple':'a','github':'h'}[method])
        return server
    server.__enter__ = Mock(return_value=server)
    server.__exit__ = Mock(return_value=False)
    def poll():
        from urllib.parse import parse_qs, urlsplit
        events.append('poll')
        server.query_params = parse_qs(urlsplit(query).query, keep_blank_values=True)
    server.handle_request = poll
    auth.ClientRedirectServer = start
    auth.subprocess.Popen = Mock(return_value=Mock(poll=Mock(return_value=0)))
    def post(endpoint, data, timeout):
        assert endpoint == 'v2/auth/' and timeout == 30
        events.append('exchange:' + data['code'][0] + ':' + data['provider'][0])
        return {'access_token': token}
    def constructor(token=None):
        instance = Mock()
        instance.post = post
        def get(endpoint, timeout):
            assert endpoint == 'v1/me' and timeout == 30
            events.append('verify:' + token)
            if fail: raise api.UnauthorizedError()
        instance.get = get
        return instance
    auth.CommaApi = constructor
    auth.set_token = saved.append
    rows.append({'status':auth.login(method), 'events':events, 'saved':saved,
                 'url':auth.auth_redirect_link(method, 43210)})
print(json.dumps(rows))
"#;
    let cases = [
        ("google", "g", Provider::Google, "inert-token", false),
        ("apple", "a", Provider::Apple, "inert-token", false),
        ("github", "h", Provider::Github, "inert-token", true),
    ];
    let mut inputs = vec![];
    for (name, id, _, token, fail) in cases {
        for query in [
            format!("/auth?code=synthetic%2Bcode&provider={id}"),
            "/auth?code=&provider=g".into(),
            "/auth?error=&code=x&provider=g".into(),
            "/auth?code=x&code=y&provider=g".into(),
            "/auth?code=%20%1C&provider=g".into(),
            "/auth?code=x&provider=wrong".into(),
        ] {
            inputs.push(serde_json::json!([name, query, token, fail]));
        }
    }
    for token in [
        serde_json::Value::Null,
        serde_json::json!(123),
        serde_json::json!(" \u{1c}"),
    ] {
        inputs.push(serde_json::json!([
            "google",
            "/auth?code=synthetic%2Bcode&provider=g",
            token,
            false
        ]));
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new("python3")
        .args(["-I", "-S", "-c", script])
        .arg(source)
        .arg(serde_json::to_string(&inputs).unwrap())
        .current_dir(temp.path())
        .env_clear()
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), inputs.len());
    for (input, original) in inputs.iter().zip(rows) {
        let provider = match input[0].as_str().unwrap() {
            "google" => Provider::Google,
            "apple" => Provider::Apple,
            _ => Provider::Github,
        };
        let mut io = InertLogin::new(input[1].as_str().unwrap());
        io.token = JsonValue::parse(&input[2].to_string()).unwrap();
        if input[3].as_bool().unwrap() {
            io.fail = Some("verify:inert-token");
        }
        let (status, saved) = run(&mut io, provider);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&status.to_json()).unwrap(),
            original["status"],
            "{input}"
        );
        assert_eq!(serde_json::json!(io.events), original["events"], "{input}");
        assert_eq!(serde_json::json!(saved), original["saved"], "{input}");
        assert_eq!(
            auth_redirect_link(provider, 43210),
            original["url"].as_str().unwrap()
        );
    }
}
