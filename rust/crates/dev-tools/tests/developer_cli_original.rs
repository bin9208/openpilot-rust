//! Fresh CLI/source oracle; functional execution belongs to the parent's Actions gate.
//! JSON records intentionally retain raw original/native channels and explicit criteria.
#![cfg(feature = "developer-ssh-native")]
use openpilot_dev_tools::developer_ssh::{
    parse_options, prepare, setup_keys, setup_keys_with_decoder, Get, KeyParams, RequestsText,
    Response, Result, TextDecoder, Transport,
};
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
use std::{cell::RefCell, path::Path, rc::Rc};

const API_HOST: &str = "https://api.example.invalid";
const ROOT: &str = "/source";

struct Case {
    input: Value,
    criteria: Value,
}
fn ssh(id: &str, args: &[&str], devices: Value) -> Case {
    Case {
        input: json!({"id":id,"tool":"ssh","args":args,"devices":devices,"token":"inert-token","source_root":ROOT,"api_host":API_HOST,"fail":""}),
        criteria: json!({"kind":"exact"}),
    }
}
fn setup(id: &str, args: &[&str], status: u16, body: &[u8], content_type: &str) -> Case {
    Case {
        input: json!({"id":id,"tool":"setup","args":args,"status":status,"body":body,"content_type":content_type,"fail":""}),
        criteria: json!({"kind":"exact"}),
    }
}
fn blank() -> Value {
    json!({"stdout":"","stderr":"","exit_code":0,"exception":null,"effects":[],"writes":[],"exec":null})
}
#[derive(Clone)]
struct Capture(Rc<RefCell<Value>>);
impl Capture {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(blank())))
    }
    fn event(&self, event: Value) {
        self.0.borrow_mut()["effects"]
            .as_array_mut()
            .unwrap()
            .push(event);
    }
    fn exception(&self, error: Box<dyn std::error::Error>) {
        let kind = error
            .downcast_ref::<Fault>()
            .map_or("NativeError", |error| error.kind);
        let mut out = self.0.borrow_mut();
        out["exit_code"] = Value::Null;
        out["exception"] = json!({"type":kind,"message":error.to_string()});
    }
    fn value(&self) -> Value {
        self.0.borrow().clone()
    }
}
#[derive(Debug)]
struct Fault {
    kind: &'static str,
    message: &'static str,
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Fault {}
fn fault(kind: &'static str, message: &'static str) -> Box<dyn std::error::Error> {
    Box::new(Fault { kind, message })
}
struct InertTransport<'a> {
    input: &'a Value,
    capture: Capture,
}
impl Transport for InertTransport<'_> {
    fn get(&mut self, request: &Get) -> Result<Response> {
        self.capture.event(json!({"kind":"http","url":request.url,"timeout":request.timeout.map(|d| d.as_secs()),"authorization":request.authorization}));
        if self.input["fail"] == "http" {
            return Err(fault("OSError", "synthetic HTTP failure"));
        }
        let ssh = self.input["tool"] == "ssh";
        Ok(Response {
            status: if ssh {
                200
            } else {
                self.input["status"].as_u64().unwrap() as u16
            },
            body: if ssh {
                self.input["devices"].to_string().into_bytes()
            } else {
                serde_json::from_value(self.input["body"].clone()).unwrap()
            },
            content_type: if ssh {
                "application/json; charset=utf-8".into()
            } else {
                self.input["content_type"].as_str().unwrap().into()
            },
        })
    }
}
struct InertParams<'a> {
    input: &'a Value,
    capture: Capture,
}
impl KeyParams for InertParams<'_> {
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        self.capture
            .event(json!({"kind":"put","key":key,"bytes":bytes}));
        if self.input["fail"] == format!("put:{key}") {
            return Err(fault("OSError", "synthetic write failure"));
        }
        self.capture.0.borrow_mut()["writes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"key":key,"bytes":bytes}));
        Ok(())
    }
}
struct ObservedText<'a> {
    input: &'a Value,
    capture: Capture,
}
impl TextDecoder for ObservedText<'_> {
    fn decode(&mut self, bytes: &[u8], content_type: &str) -> Result<String> {
        self.capture.event(json!({"kind":"decode"}));
        if self.input["fail"] == "decode" {
            return Err(fault("LookupError", "synthetic decoder failure"));
        }
        RequestsText.decode(bytes, content_type)
    }
}
fn candidate(input: &Value, observe_decode: bool) -> Value {
    let capture = Capture::new();
    let args: Vec<String> = serde_json::from_value(input["args"].clone()).unwrap();
    let mut transport = InertTransport {
        input,
        capture: capture.clone(),
    };
    if input["tool"] == "ssh" {
        let options = match parse_options(&args, Path::new(input["source_root"].as_str().unwrap()))
        {
            Ok(Some(options)) => options,
            Ok(None) => return capture.value(), // Pure parser emits no help text.
            Err(error) => {
                let mut out = capture.0.borrow_mut();
                out["exit_code"] = json!(2);
                out["exception"] = json!({"type":"NativeParseError","message":error.to_string()});
                return out.clone();
            }
        };
        // Only the original CLI's token-read boundary is simulated here; the real
        // native ssh_cli/get_token_at/Plan::exec entrypoints are never called.
        capture.event(json!({"kind":"get_token"}));
        let token = JsonValue::parse(&input["token"].to_string()).unwrap();
        match prepare(
            &mut transport,
            input["api_host"].as_str().unwrap(),
            &token,
            &options,
        ) {
            Ok(Ok(plan)) => {
                capture.0.borrow_mut()["stdout"] = json!(plan.stdout);
                capture.event(json!({"kind":"exec","file":"ssh","argv":plan.argv}));
                let mut out = capture.0.borrow_mut();
                out["exec"] = json!({"file":"ssh","argv":plan.argv});
                out["exit_code"] = Value::Null; // process-replacement boundary, not exit0
            }
            Ok(Err(failure)) => {
                let mut out = capture.0.borrow_mut();
                out["stderr"] = json!(failure.stderr);
                out["exit_code"] = json!(1);
            }
            Err(error) => capture.exception(error),
        }
    } else {
        let Some(username) = args.first() else {
            let mut out = capture.0.borrow_mut();
            out["stdout"] = json!("setup_ssh_keys.py <github username>\n");
            out["exit_code"] = json!(1);
            return out.clone();
        };
        let open = || -> Result<InertParams<'_>> {
            capture.event(json!({"kind":"Params"}));
            if input["fail"] == "Params" {
                return Err(fault("OSError", "synthetic Params failure"));
            }
            Ok(InertParams {
                input,
                capture: capture.clone(),
            })
        };
        let result = if observe_decode {
            setup_keys_with_decoder(
                &mut transport,
                username,
                &mut ObservedText {
                    input,
                    capture: capture.clone(),
                },
                open,
            )
        } else {
            setup_keys(&mut transport, username, open)
        };
        match result {
            Ok(message) => capture.0.borrow_mut()["stdout"] = json!(message),
            Err(error) => capture.exception(error),
        }
    }
    capture.value()
}

const ORIGINAL: &str = r#"
import ast, contextlib, hashlib, io, json, os, sys
from types import ModuleType
import requests as real_requests
assert sys.version_info[:3] == (3, 12, 15), 'the prepared Python runtime must stay pinned after env_clear'
root, cases = sys.argv[1], json.loads(sys.argv[2])
paths = {'ssh':'tools/scripts/ssh.py', 'setup':'tools/scripts/setup_ssh_keys.py'}
source_bytes = {tool: open(os.path.join(root, path), 'rb').read() for tool, path in paths.items()}
sources = {tool: data.decode('utf-8') for tool, data in source_bytes.items()}
metadata = {'python':sys.version, 'requests':real_requests.__version__,
            'sources':{paths[k]:hashlib.sha256(v).hexdigest() for k,v in source_bytes.items()}}
native_paths = ['rust/crates/dev-tools/src/developer_ssh.rs',
                'rust/crates/dev-tools/src/developer_ssh/unicode_lower15.rs',
                'rust/crates/dev-tools/src/developer_ssh/unicode_lower15.provenance.json',
                'rust/crates/dev-tools/src/developer_ssh/unicode_lower15.LICENSE-UNICODE-V3.txt',
                'rust/crates/dev-tools/src/developer_ssh/unicode_lower15_original.rs']
metadata['native_sources'] = {path:hashlib.sha256(open(os.path.join(root,path),'rb').read()).hexdigest()
                              for path in native_paths}
class ExecIntercept(BaseException): pass
rows = []
for case in cases:
    events, writes, execution = [], [], None
    api = ModuleType('openpilot.tools.lib.api')
    config = ModuleType('openpilot.tools.lib.auth_config')
    basedir = ModuleType('openpilot.common.basedir')
    params_module = ModuleType('openpilot.common.params')
    requests_module = ModuleType('requests')
    basedir.BASEDIR = case.get('source_root')
    def get_token():
        events.append({'kind':'get_token'})
        return case['token']
    class CommaApi:
        def __init__(self, token=None): self.token = token
        def get(self, endpoint, **kwargs):
            assert endpoint == 'v1/me/devices' and not kwargs
            authorization = 'JWT ' + self.token if self.token else None
            events.append(dict(kind='http', url=case['api_host'] + '/' + endpoint,
                               timeout=None, authorization=authorization))
            if case['fail'] == 'http': raise OSError('synthetic HTTP failure')
            return case['devices']
    class Params:
        def __init__(self):
            events.append({'kind':'Params'})
            if case['fail'] == 'Params': raise OSError('synthetic Params failure')
        def put_bool(self, key, value): self.put(key, b'1' if value else b'0')
        def put(self, key, value):
            data = value.encode('utf-8') if isinstance(value, str) else value
            effect = dict(key=key, bytes=list(data))
            events.append(dict(kind='put', **effect))
            if case['fail'] == 'put:' + key: raise OSError('synthetic write failure')
            writes.append(effect)
    class InertResponse(real_requests.Response):
        @property
        def text(self):
            events.append({'kind':'decode'})
            if case['fail'] == 'decode': raise LookupError('synthetic decoder failure')
            return super().text
    def get(url, timeout):
        events.append(dict(kind='http', url=url, timeout=timeout, authorization=None))
        if case['fail'] == 'http': raise OSError('synthetic HTTP failure')
        response = InertResponse()
        response.status_code = case['status']
        response._content = bytes(case['body'])
        if case['content_type']: response.headers['content-type'] = case['content_type']
        response.encoding = real_requests.utils.get_encoding_from_headers(response.headers)
        return response
    def execvp(file, argv):
        global execution
        execution = dict(file=file, argv=list(argv))
        events.append(dict(kind='exec', **execution))
        raise ExecIntercept()
    api.CommaApi, config.get_token = CommaApi, get_token
    params_module.Params, requests_module.get = Params, get
    for name, module in [('openpilot.tools.lib.api',api), ('openpilot.tools.lib.auth_config',config),
                         ('openpilot.common.basedir',basedir), ('openpilot.common.params',params_module),
                         ('requests',requests_module)]: sys.modules[name] = module
    out, err = io.StringIO(), io.StringIO()
    code, exception = 0, None
    old_execvp = os.execvp
    os.execvp = execvp
    sys.argv = [os.path.basename(paths[case['tool']])] + case['args']
    # Site's exit helper would close stdin. Model only SystemExit, never real stdin.
    namespace = {'__name__':'__main__', '__file__':paths[case['tool']], 'exit':sys.exit}
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try: exec(compile(ast.parse(sources[case['tool']]), paths[case['tool']], 'exec'), namespace)
        except ExecIntercept: code = None
        except SystemExit as e: code = e.code
        except Exception as e: code, exception = None, dict(type=type(e).__name__, message=str(e))
        finally: os.execvp = old_execvp
    rows.append(dict(stdout=out.getvalue(), stderr=err.getvalue(), exit_code=code,
                     exception=exception, effects=events, writes=writes, exec=execution))
print(json.dumps(dict(metadata=metadata, outcomes=rows)))
"#;

fn oracle(cases: &[Case]) -> Value {
    assert!(!cases.is_empty() && cases.len() < 100);
    let input = cases.iter().map(|case| &case.input).collect::<Vec<_>>();
    let interpreter = std::env::var_os("DEVELOPER_CLI_ORACLE_PYTHON")
        .or_else(|| std::env::var_os("AUTH_CLI_ORACLE_PYTHON"))
        .expect("prepared absolute developer CLI interpreter required");
    assert!(Path::new(&interpreter).is_absolute());
    let temp = tempfile::tempdir().unwrap();
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = std::process::Command::new(interpreter)
        // requests.Response is the independent default-decoding oracle. Use the
        // parent's prepared Python/requests environment; never call requests.get.
        .args(["-I", "-B", "-X"])
        .arg(format!(
            "pycache_prefix={}/fresh-pycache",
            temp.path().display()
        ))
        .args(["-c", ORIGINAL])
        .arg(source_root)
        .arg(serde_json::to_string(&input).unwrap())
        .stdin(std::process::Stdio::null())
        .env_clear()
        .env(
            "LD_LIBRARY_PATH",
            std::env::var_os("OPENPILOT_ORACLE_PYTHON_LIBRARY_PATH")
                .expect("prepared Python runtime library directory required"),
        )
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
fn compare(group: &str, cases: &[Case]) {
    let original = oracle(cases);
    let outcomes = original["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), cases.len());
    let records = cases
        .iter()
        .zip(outcomes)
        .map(|(case, original)| {
            json!({
            "input":case.input, "criteria":case.criteria, "expected_original":original,
            })
        })
        .collect::<Vec<_>>();
    let envelope = json!({"schema":"openpilot.developer-cli-original.v1", "group":group,
        "metadata":original["metadata"], "records":records});
    // --nocapture in Actions preserves the complete source-independent JSON shape.
    // No fixture is generated/frozen locally and no output path is implicitly written.
    println!(
        "DEVELOPER_CLI_ORIGINAL_JSON {}",
        serde_json::to_string(&envelope).unwrap()
    );
    verify_envelope(&envelope);
}

// This checker consumes only JSON expectations, with no Python/source dependency.
// The default tests always obtain fresh originals; future fixture loading belongs
// to the parent's separately qualified retirement step, with these same criteria.
fn verify_envelope(envelope: &Value) {
    assert_eq!(envelope["schema"], "openpilot.developer-cli-original.v1");
    let (expected_count, tool) = match envelope["group"].as_str().unwrap() {
        "ssh-plan" => (26, "ssh"),
        "ssh-parser" => (13, "ssh"),
        "setup-effects" => (21, "setup"),
        group => panic!("unknown original expectation group: {group}"),
    };
    let records = envelope["records"].as_array().unwrap();
    assert_eq!(
        records.len(),
        expected_count,
        "incomplete expectation group"
    );
    let mut ids = std::collections::BTreeSet::new();
    for expected in records {
        assert_eq!(expected["input"]["tool"], tool);
        assert!(
            ids.insert(expected["input"]["id"].as_str().unwrap()),
            "duplicate case id"
        );
        let mut record = expected.clone();
        record["observed_native"] = candidate(&expected["input"], true);
        let (source, native, criteria) = (
            &record["expected_original"],
            &record["observed_native"],
            &record["criteria"],
        );
        match criteria["kind"].as_str().unwrap() {
            "exact" => assert_eq!(native, source, "{record}"),
            "help-boundary" => {
                assert_eq!(
                    native,
                    &blank(),
                    "native parse_options emits no help bytes: {record}"
                );
                assert_eq!(source["exit_code"], 0, "{record}");
                assert_eq!(source["exception"], Value::Null, "{record}");
                assert_eq!(source["stderr"], "", "{record}");
                assert!(
                    source["stdout"]
                        .as_str()
                        .unwrap()
                        .starts_with("usage: ssh.py "),
                    "{record}"
                );
                assert!(
                    source["stdout"]
                        .as_str()
                        .unwrap()
                        .contains("device name or dongle id"),
                    "{record}"
                );
                for field in ["effects", "writes", "exec"] {
                    assert_eq!(native[field], source[field], "{record}");
                }
            }
            "parser-error-boundary" => {
                assert_eq!(source["exit_code"], 2, "{record}");
                assert_eq!(source["exception"], Value::Null, "{record}");
                assert_eq!(source["stdout"], "", "{record}");
                assert!(
                    source["stderr"]
                        .as_str()
                        .unwrap()
                        .starts_with("usage: ssh.py "),
                    "{record}"
                );
                assert!(
                    source["stderr"]
                        .as_str()
                        .unwrap()
                        .ends_with(criteria["source_suffix"].as_str().unwrap()),
                    "{record}"
                );
                let mut expected = blank();
                expected["exit_code"] = json!(2);
                expected["exception"] =
                    json!({"type":"NativeParseError","message":criteria["native_message"]});
                assert_eq!(native, &expected, "{record}");
                for field in ["effects", "writes", "exec"] {
                    assert_eq!(native[field], source[field], "{record}");
                }
            }
            "exception-pair" => {
                assert_eq!(
                    source["exception"], criteria["source_exception"],
                    "{record}"
                );
                assert_eq!(
                    native["exception"], criteria["native_exception"],
                    "{record}"
                );
                for field in ["stdout", "stderr", "exit_code", "effects", "writes", "exec"] {
                    assert_eq!(native[field], source[field], "{record}");
                }
            }
            _ => panic!("unknown criteria: {record}"),
        }
        if record["input"]["tool"] == "setup"
            && (record["input"]["fail"] != "decode" || record["input"]["status"] != 200)
        {
            // A single documented projection removes ONLY the observer marker,
            // cross-checking actual setup_keys against its RequestsText wrapper.
            let default = candidate(&record["input"], false);
            let mut observed = native.clone();
            observed["effects"]
                .as_array_mut()
                .unwrap()
                .retain(|event| event["kind"] != "decode");
            assert_eq!(
                default, observed,
                "default setup_keys cross-check: {record}"
            );
        }
    }
}

#[test]
fn ssh_cli_original_aliases_prefix_unicode_duplicate_order_and_exec() {
    let rows = json!([
        {"dongle_id":"first","alias":"old"}, {"dongle_id":"second","alias":"Test Two"},
        {"dongle_id":"first","alias":"Test One"}, {"dongle_id":"null","alias":null},
        {"dongle_id":"integer","alias":123}, {"dongle_id":"boolean","alias":false}
    ]);
    let mut cases = vec![
        ssh("alias-space-lower", &["t E s T o N e"], rows.clone()),
        ssh("duplicate-order", &["test"], rows.clone()),
        ssh("overwritten-alias-absent", &["old"], rows.clone()),
        ssh("blank-string-alias-filter", &["  "], rows),
        ssh(
            "direct-not-fullmatch",
            &["0123456789ABCDEF-suffix"],
            json!([]),
        ),
        ssh("direct-fifteen-falls-back", &["0123456789ABCDE"], json!([])),
        ssh(
            "nonascii-prefix-falls-back",
            &["é123456789ABCDEF"],
            json!([]),
        ),
        ssh(
            "known-direct-nonstring-name",
            &["0123456789ABCDEF"],
            json!([{"dongle_id":"0123456789ABCDEF","alias":null}]),
        ),
        ssh(
            "unicode-hangul",
            &["한 글 차"],
            json!([{"dongle_id":"unicode","alias":"한글차"}]),
        ),
        ssh(
            "unicode-dotted-i",
            &["i\u{307}"],
            json!([{"dongle_id":"unicode","alias":"İ"}]),
        ),
        ssh(
            "unicode-lower-not-casefold",
            &["STRASSE"],
            json!([{"dongle_id":"unicode","alias":"Straße"}]),
        ),
        ssh(
            "nbsp-not-ascii-space",
            &["testcar"],
            json!([{"dongle_id":"unicode","alias":"Test\u{a0}Car"}]),
        ),
        ssh(
            "unicode16-rams-horn-alias-false-match",
            &["\u{264}"],
            json!([{"dongle_id":"0000000000000001","alias":"\u{a7cb}"}]),
        ),
        ssh(
            "unicode16-tje-alias-false-match",
            &["\u{1c8a}"],
            json!([{"dongle_id":"0000000000000002","alias":"\u{1c89}"}]),
        ),
        ssh(
            "unicode15-cased-property-sigma-left",
            &["\u{295}\u{3c3}"],
            json!([{"dongle_id":"sigma-left","alias":"\u{295}\u{3a3}"}]),
        ),
        ssh(
            "unicode15-ignorable-property-sigma-left",
            &["a\u{1171e}\u{3c3}"],
            json!([{"dongle_id":"sigma-ignore","alias":"A\u{1171e}\u{3a3}"}]),
        ),
        ssh(
            "unicode15-cased-property-sigma-right",
            &["a\u{3c2}\u{295}"],
            json!([{"dongle_id":"sigma-right","alias":"A\u{3a3}\u{295}"}]),
        ),
        ssh(
            "ascii-space-removed-before-sigma-context",
            &["a\u{3c2}"],
            json!([{"dongle_id":"sigma-space","alias":"A \u{3a3}"}]),
        ),
        ssh(
            "debug-literal-quoting",
            &[
                "0123456789ABCDEF trailing",
                "--key",
                "/key dir/it's.key",
                "--host",
                "jump host",
                "--port=-１２_３",
                "--debug",
            ],
            json!([]),
        ),
        ssh(
            "empty-key-tab-host",
            &["0123456789ABCDEF", "--key=", "--host=a\tb", "--debug"],
            json!([]),
        ),
        ssh(
            "port-repeated-last-wins",
            &["0123456789ABCDEF", "--port=21", "--port=23"],
            json!([]),
        ),
        ssh(
            "port-signed-unicode-space",
            &["0123456789ABCDEF", "--port", " +００２２ "],
            json!([]),
        ),
        ssh(
            "port-outside-tcp-range",
            &["0123456789ABCDEF", "--port=65536"],
            json!([]),
        ),
    ];
    let mut no_token = ssh("false-token", &["0123456789ABCDEF"], json!([]));
    no_token.input["token"] = Value::Null;
    cases.push(no_token);
    let mut failure = ssh("http-failure", &["0123456789ABCDEF"], json!([]));
    failure.input["fail"] = json!("http");
    cases.push(failure);
    let mut malformed = ssh(
        "direct-still-builds-entire-dict",
        &["0123456789ABCDEF"],
        json!([{"dongle_id":"broken"}]),
    );
    malformed.criteria = json!({"kind":"exception-pair", "source_exception":{"type":"KeyError","message":"'alias'"},
        "native_exception":{"type":"NativeError","message":"missing alias"}});
    cases.push(malformed);
    compare("ssh-plan", &cases);
}

#[test]
fn ssh_cli_original_port_argparse_help_and_no_auth_effects() {
    let mut cases = vec![];
    for (id, args) in [
        ("help", vec!["--help"]),
        ("help-before-bad-port", vec!["--help", "--port=bad"]),
        ("help-before-ambiguous", vec!["--help", "--h"]),
        ("valid-port-before-help", vec!["--port=22", "--help"]),
    ] {
        let mut case = ssh(id, &args, json!([]));
        case.criteria = json!({"kind":"help-boundary"});
        cases.push(case);
    }
    for (id, args, source_suffix, native_message) in [
        (
            "port-invalid",
            vec!["device", "--port", "bad"],
            "argument --port: invalid int value: 'bad'",
            "invalid int value: \"bad\"",
        ),
        (
            "bad-port-before-help",
            vec!["--port=bad", "--help"],
            "argument --port: invalid int value: 'bad'",
            "invalid int value: \"bad\"",
        ),
        (
            "overwritten-bad-port-still-fails",
            vec!["device", "--port=bad", "--port=22", "--help"],
            "argument --port: invalid int value: 'bad'",
            "invalid int value: \"bad\"",
        ),
        (
            "missing-device",
            vec![],
            "the following arguments are required: device",
            "unrecognized arguments",
        ),
        (
            "extra-positional",
            vec!["device", "extra"],
            "unrecognized arguments: extra",
            "unrecognized arguments",
        ),
        (
            "host-missing-value",
            vec!["device", "--host"],
            "argument --host: expected one argument",
            "--host requires a value",
        ),
        (
            "unknown-option",
            vec!["device", "--unknown"],
            "unrecognized arguments: --unknown",
            "unrecognized arguments",
        ),
        (
            "bad-port-before-ambiguous-help",
            vec!["--port=bad", "--h", "--help"],
            "argument --port: invalid int value: 'bad'",
            "invalid int value: \"bad\"",
        ),
        (
            "ambiguous-before-help",
            vec!["--h", "--help"],
            "ambiguous option: --h could match --help, --host",
            "ambiguous option: --h",
        ),
    ] {
        let mut case = ssh(id, &args, json!([]));
        case.criteria = json!({"kind":"parser-error-boundary", "source_suffix":format!("ssh.py: error: {source_suffix}\n"),"native_message":native_message});
        cases.push(case);
    }
    compare("ssh-parser", &cases);
}

#[test]
fn setup_cli_original_status_default_decode_and_write_failure_order() {
    let mut cases = vec![
        setup("missing-username", &[], 200, b"unused", "text/plain"),
        setup(
            "normal-keys",
            &["synthetic"],
            200,
            b" ssh-key synthetic\r\n\n",
            "text/plain",
        ),
        setup(
            "empty-body",
            &["synthetic"],
            200,
            b"",
            "text/plain; charset=not-a-codec",
        ),
        setup(
            "default-latin1",
            &["synthetic"],
            200,
            b"key\xff",
            "text/plain",
        ),
        setup(
            "utf8-replacement",
            &["synthetic"],
            200,
            b"key\xff\xe2\x82",
            "text/plain; charset=utf-8",
        ),
        setup(
            "unknown-codec-fallback",
            &["synthetic"],
            200,
            b"key\xff",
            "text/plain; charset=not-a-codec",
        ),
        setup(
            "ascii-replacement",
            &["synthetic"],
            200,
            b"key\xff",
            "text/plain; charset=ascii",
        ),
        setup(
            "extra-args-ignored",
            &["some user/../name", "ignored", "--help"],
            200,
            b"key",
            "text/plain",
        ),
        setup("help-is-username", &["--help"], 200, b"key", "text/plain"),
        setup("unicode-username", &["한글"], 200, b"key", "text/plain"),
    ];
    for status in [201, 204, 301, 404, 500] {
        let mut case = setup(
            &format!("non200-{status}"),
            &["synthetic"],
            status,
            b"never decoded",
            "text/plain",
        );
        case.input["fail"] = json!("decode");
        cases.push(case);
    }
    for fail in [
        "http",
        "Params",
        "put:SshEnabled",
        "decode",
        "put:GithubSshKeys",
        "put:GithubUsername",
    ] {
        let mut case = setup(
            &format!("failure-{fail}"),
            &["synthetic"],
            200,
            b"key\r\n",
            "text/plain",
        );
        case.input["fail"] = json!(fail);
        cases.push(case);
    }
    compare("setup-effects", &cases);
}
