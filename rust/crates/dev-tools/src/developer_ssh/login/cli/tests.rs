//! Original CLI oracle is synthetic and runs only in the parent's Actions gate.
use super::*;
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug)]
struct DependencyError(&'static str);
impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for DependencyError {}
struct InertCli<'a> {
    case: &'a Value,
    events: Vec<Value>,
    stored: Vec<Value>,
}
impl CliIo for InertCli<'_> {
    fn login(&mut self, provider: Provider) -> LoginStatus {
        self.events.push(json!(["login", provider.id()]));
        if self.case["login_error"].as_bool().unwrap_or(false) {
            LoginStatus::Error("Sign-in was declined. Choose a provider to try again.")
        } else {
            LoginStatus::Success
        }
    }
    fn set_token(&mut self, token: &str) -> Result<()> {
        self.events.push(json!(["set", token]));
        if self.case["fail"] == "set" {
            return Err(DependencyError("synthetic set failure").into());
        }
        self.stored.push(json!(token));
        Ok(())
    }
    fn get_token(&mut self) -> Result<JsonValue> {
        self.events.push(json!(["get"]));
        Ok(JsonValue::parse(&self.case["saved_token"].to_string()).unwrap())
    }
    fn me(&mut self, token: &JsonValue) -> Result<JsonValue> {
        self.events.push(json!([
            "me",
            serde_json::from_str::<Value>(&token.to_json().unwrap()).unwrap()
        ]));
        match self.case["fail"].as_str().unwrap_or("") {
            "unauthorized" => Err(Unauthorized.into()),
            "api" => Err(DependencyError("synthetic API failure").into()),
            _ => Ok(JsonValue::parse(self.case["me"].as_str().unwrap()).unwrap()),
        }
    }
}
fn candidate(case: &Value) -> Value {
    let args: Vec<String> = serde_json::from_value(case["args"].clone()).unwrap();
    let mut io = InertCli {
        case,
        events: vec![],
        stored: vec![],
    };
    let (mut out, mut err) = (vec![], vec![]);
    let result = run_cli_with("auth.py", &args, &mut io, &mut out, &mut err);
    let (code, exception) = match result {
        Ok(code) => (json!(code), Value::Null),
        Err(error) => (Value::Null, json!(error.to_string())),
    };
    json!({"stdout": String::from_utf8(out).unwrap(), "stderr": String::from_utf8(err).unwrap(),
        "code":code, "exception":exception, "events":io.events, "stored":io.stored})
}
fn case(args: &[&str], me: &str) -> Value {
    json!({"args":args, "me":me, "saved_token":"inert-reloaded", "fail":"", "login_error":false})
}
fn oracle(cases: &[Value]) -> Vec<Value> {
    // Execute the unchanged main AST after importing the original with inert dependencies.
    // No original login(), filesystem credential operation, API, browser or stdin is used.
    let script = r#"
import ast, contextlib, io, json, sys
from types import ModuleType
api = ModuleType('openpilot.tools.lib.api')
api.UnauthorizedError = type('UnauthorizedError', (Exception,), {})
api.APIError = type('APIError', (Exception,), {})
api.CommaApi = None
config = ModuleType('openpilot.tools.lib.auth_config')
config.set_token = config.get_token = None
sys.modules[api.__name__] = api
sys.modules[config.__name__] = config
source = open(sys.argv[1], encoding='utf-8').read()
tree = ast.parse(source, filename=sys.argv[1])
main = tree.body[-1]
assert isinstance(main, ast.If) and ast.unparse(main.test) == "__name__ == '__main__'"
# -S omits site's exit helper; inject only SystemExit semantics so the oracle
# never closes or otherwise accesses the runner's real stdin.
namespace = {'__name__':'_inert_auth', '__file__':sys.argv[1], 'exit':sys.exit}
exec(compile(tree, sys.argv[1], 'exec'), namespace)
rows = []
for case in json.loads(sys.argv[2]):
    events, stored = [], []
    def login(method):
        events.append(['login', {'google':'g','apple':'a','github':'h'}[method]])
        return {'error':'Sign-in was declined. Choose a provider to try again.'} if case['login_error'] else {'success':True}
    def set_token(token):
        events.append(['set', token])
        if case['fail'] == 'set': raise OSError('synthetic set failure')
        stored.append(token)
    def get_token():
        events.append(['get'])
        return case['saved_token']
    class CommaApi:
        def __init__(self, token=None): self.token = token
        def get(self, endpoint, **kwargs):
            assert endpoint == '/v1/me' and not kwargs
            events.append(['me', self.token])
            if case['fail'] == 'unauthorized': raise api.UnauthorizedError()
            if case['fail'] == 'api': raise api.APIError('synthetic API failure')
            return json.loads(case['me'])
    namespace.update(login=login, set_token=set_token, get_token=get_token, CommaApi=CommaApi, __name__='__main__')
    sys.argv = ['auth.py'] + case['args']
    out, err = io.StringIO(), io.StringIO()
    code, exception = 0, None
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try: exec(compile(ast.Module(body=[main], type_ignores=[]), namespace['__file__'], 'exec'), namespace)
        except SystemExit as e: code = e.code
        except Exception as e: code, exception = None, str(e)
    rows.append(dict(stdout=out.getvalue(), stderr=err.getvalue(), code=code, exception=exception, events=events, stored=stored))
print(json.dumps(rows))
"#;
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../openpilot/tools/lib/auth.py");
    let interpreter =
        std::env::var_os("AUTH_CLI_ORACLE_PYTHON").unwrap_or_else(|| "python3".into());
    let output = std::process::Command::new(interpreter)
        .args(["-I", "-S", "-c", script])
        .arg(source)
        .arg(serde_json::to_string(cases).unwrap())
        .env_clear()
        .env("COLUMNS", "80")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn compare(cases: &[Value]) {
    let rows = oracle(cases);
    assert_eq!(rows.len(), cases.len());
    for (case, original) in cases.iter().zip(rows) {
        assert_eq!(candidate(case), original, "case: {case}");
    }
}

#[test]
fn cli_modes_match_original_storage_reload_and_unauthorized_order() {
    let mut cases = vec![];
    for args in [
        vec![],
        vec!["google"],
        vec!["apple", "unused"],
        vec!["github"],
        vec!["jwt"],
        vec!["jwt", "inert-direct"],
        vec!["jwt", ""],
        vec!["--json"],
        vec!["github", "--json"],
        vec!["jwt", "inert", "--json"],
    ] {
        cases.push(case(&args, r#"{"z":false,"a":"inert user"}"#));
    }
    for args in [vec!["jwt", "inert"], vec!["google"]] {
        for fail in ["unauthorized", "api", "set"] {
            let mut input = case(&args, "{}");
            input["fail"] = json!(fail);
            cases.push(input);
        }
    }
    for args in [vec![], vec!["--json"]] {
        let mut input = case(&args, "{}");
        input["login_error"] = json!(true);
        cases.push(input);
    }
    compare(&cases);
    let missing = candidate(&case(&["jwt"], "{}"));
    assert_eq!(
        missing["stdout"],
        "method JWT selected, but no JWT was provided\n"
    );
    assert_eq!(missing["events"], json!([]));
    let unauthorized = {
        let mut c = case(&["jwt", "inert"], "{}");
        c["fail"] = json!("unauthorized");
        candidate(&c)
    };
    assert_eq!(unauthorized["stdout"], "Got invalid JWT\n");
    assert_eq!(unauthorized["stored"], json!(["inert"]));
}

#[test]
fn cli_argument_actions_and_diagnostics_match_original() {
    let args = [
        vec!["--help"],
        vec!["-h"],
        vec!["--he"],
        vec!["-hh"],
        vec!["-hhh"],
        vec!["-hvalue"],
        vec!["-h=explicit"],
        vec!["-hh=explicit"],
        vec!["-h--"],
        vec!["--js"],
        vec!["--json", "apple"],
        vec!["--json", "apple", "unused"],
        vec!["apple", "--json", "unused"],
        vec!["--", "jwt", "--json"],
        vec!["jwt", "--", "-inert"],
        vec!["--json", "--", "github"],
        vec!["no-such-provider"],
        vec!["-1"],
        vec!["-0.5"],
        vec!["--unknown"],
        vec!["google", "jwt", "extra"],
        vec!["--json=value"],
        vec!["--help=value"],
        vec!["--unknown", "--help"],
        vec!["bad", "--help"],
        vec!["--help", "bad"],
        vec!["--json=value", "--help"],
        vec!["--json", "--json"],
        vec!["--j", "--help"],
        vec![""],
        vec!["jwt", "-"],
        vec!["jwt", "-1"],
        vec!["jwt", "--unknown"],
    ];
    compare(&args.iter().map(|args| case(args, "{}")).collect::<Vec<_>>());
    for args in [
        vec!["--help"],
        vec!["--help", "bad"],
        vec!["--unknown", "--help"],
    ] {
        let help = candidate(&case(&args, "{}"));
        assert_eq!(help["code"], 0);
        assert_eq!(help["events"], json!([]));
        assert_eq!(help["stored"], json!([]));
        assert_eq!(help["stderr"], "");
    }
}

#[test]
fn cli_pprint_json_domain_matches_original_sorting_wrapping_and_repr() {
    let mut values = vec![
        "null".to_string(), "true".into(), "123456789012345678901234567890".into(),
        "[NaN,Infinity,-Infinity,-0.0,1e-7,1e16]".into(),
        r#"{"z":0,"a":{"b":false,"a":null},"é":"한😀","quote":"it's both \"quoted\"","control":"\u001c\u0085\ud800"}"#.into(),
        json!(["word ".repeat(25), "line one\nline two\n", "x".repeat(100)]).to_string(),
        json!({"nested": {"z": (0..40).collect::<Vec<_>>(), "a": "space ".repeat(30)}, "a": "short"}).to_string(),
        json!("word ".repeat(40)).to_string(),
        json!("a\r\nb\u{1c}c\u{85}d\u{2028}e\u{2029}tail ".repeat(8)).to_string(),
    ];
    for width in [76, 77, 78, 79, 80, 81] {
        values.push(json!({"x":"a".repeat(width)}).to_string());
        values.push(json!(vec!["word ".repeat(width / 5), "tail".into()]).to_string());
    }
    compare(
        &values
            .iter()
            .map(|value| case(&["jwt", "inert"], value))
            .collect::<Vec<_>>(),
    );
    let sorted = candidate(&case(&["jwt", "inert"], r#"{"z":0,"a":1}"#));
    assert_eq!(sorted["stdout"], "Authenticated!\n{'a': 1, 'z': 0}\n");
}
