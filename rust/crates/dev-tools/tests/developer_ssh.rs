//! Synthetic source tests only: no processes, credential roots, network or runtime Params.
use openpilot_dev_tools::developer_ssh::*;
use openpilot_log_format::JsonValue;
use std::{cell::RefCell, path::Path, time::Duration};

fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn options(device: &str) -> Options {
    Options {
        device: device.into(),
        host: "ssh.comma.ai".into(),
        port: "22".into(),
        key: "/source/key".into(),
        debug: false,
    }
}
struct Fake {
    calls: RefCell<Vec<Get>>,
    status: u16,
    text: String,
    fail: bool,
}
impl Fake {
    fn new(status: u16, text: &str) -> Self {
        Self {
            calls: RefCell::default(),
            status,
            text: text.into(),
            fail: false,
        }
    }
}
impl Transport for Fake {
    fn get(&mut self, request: &Get) -> Result<Response> {
        self.calls.borrow_mut().push(request.clone());
        if self.fail {
            return Err("synthetic transport failure".into());
        }
        Ok(Response {
            status: self.status,
            body: self.text.as_bytes().to_vec(),
            content_type: "application/json; charset=utf-8".into(),
        })
    }
}

#[test]
fn prefix_sixteen_is_not_fullmatch_and_fetch_still_happens() {
    let mut fake = Fake::new(200, "[]");
    let result = prepare(
        &mut fake,
        "https://example.invalid/",
        &json("null"),
        &options("0123456789ABCDEF-suffix"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        result.argv.last().unwrap(),
        "comma@comma-0123456789ABCDEF-suffix"
    );
    let calls = fake.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].url, "https://example.invalid//v1/me/devices");
    assert_eq!(calls[0].timeout, None);
    assert_eq!(calls[0].user_agent, "OpenpilotTools");
    assert_eq!(calls[0].authorization, None);
    assert!(plan(&json("[]"), &options("0123456789ABCDE"))
        .unwrap()
        .is_err());
    assert!(plan(&json("[]"), &options("é123456789ABCDEF"))
        .unwrap()
        .is_err());
}

#[test]
fn duplicate_ids_overwrite_alias_without_reordering_and_ignore_nonstring_aliases() {
    let devices = json(
        r#"[
      {"dongle_id":"first","alias":"old"},
      {"dongle_id":"second","alias":"Test Two"},
      {"dongle_id":"first","alias":"Test One"},
      {"dongle_id":"null","alias":null},
      {"dongle_id":"number","alias":123},
      {"dongle_id":"bool","alias":true} ]"#,
    );
    let failure = plan(&devices, &options("T e s t")).unwrap().unwrap_err();
    assert_eq!(
        failure.stderr,
        "failed to look up dongle id for \"T e s t\"\nfound multiple matches:\n  \"Test One\" (first)\n  \"Test Two\" (second)\n"
    );
    let unique = plan(&devices, &options("t e s t o n e")).unwrap().unwrap();
    assert_eq!(
        unique.stdout,
        "connecting to Test One (first) through ssh.comma.ai:22 ...\n"
    );
    assert_eq!(
        plan(&devices, &options("old")).unwrap().unwrap_err().stderr,
        "failed to look up dongle id for \"old\"\n"
    );
    assert!(plan(&devices, &options("123")).unwrap().is_err());
}

#[test]
fn alias_spaces_are_ascii_only_lower_is_unicode_not_casefold() {
    let devices = json(
        r#"[
      {"dongle_id":"greek","alias":"ΟΣ"},
      {"dongle_id":"turkish","alias":"İSTANBUL"},
      {"dongle_id":"sharp","alias":"Straße"},
      {"dongle_id":"tab","alias":"a\tb"},
      {"dongle_id":"nbsp","alias":"c\u00a0d"},
      {"dongle_id":"korean","alias":"테 스트"} ]"#,
    );
    for (input, expected) in [
        ("ος", "greek"),
        ("i\u{307}stan", "turkish"),
        ("straße", "sharp"),
        ("a\tb", "tab"),
        ("c\u{a0}d", "nbsp"),
        ("테스트", "korean"),
    ] {
        assert_eq!(
            plan(&devices, &options(input))
                .unwrap()
                .unwrap()
                .argv
                .last()
                .unwrap(),
            &format!("comma@comma-{expected}")
        );
    }
    for input in ["οσ", "strasse", "ab", "cd"] {
        assert!(plan(&devices, &options(input)).unwrap().is_err(), "{input}");
    }
}

#[test]
fn direct_known_id_formats_even_nonstring_alias_and_unknown_id_is_allowed() {
    for (alias, display) in [
        ("null", "None"),
        ("true", "True"),
        ("123", "123"),
        (r#"["x", false]"#, "['x', False]"),
    ] {
        let devices = json(&format!(
            r#"[{{"dongle_id":"0123456789ABCDEF","alias":{alias}}}]"#
        ));
        assert_eq!(
            plan(&devices, &options("0123456789ABCDEF"))
                .unwrap()
                .unwrap()
                .stdout,
            format!("connecting to {display} (0123456789ABCDEF) through ssh.comma.ai:22 ...\n")
        );
    }
    let value = plan(&json("[]"), &options("0123456789ABCDEF"))
        .unwrap()
        .unwrap();
    assert_eq!(
        value.stdout,
        "connecting to 0123456789ABCDEF through ssh.comma.ai:22 ...\n"
    );
    assert!(plan(
        &json(r#"[{"dongle_id":"id"}]"#),
        &options("0123456789ABCDEF")
    )
    .is_err());
}

#[test]
fn proxy_argv_and_debug_are_literal_source_formatting() {
    let mut args = options("0123456789ABCDEF trailing");
    args.key = "/key dir/it's.key".into();
    args.host = "jump host".into();
    args.port = "-123".into();
    args.debug = true;
    let result = plan(&json("[]"), &args).unwrap().unwrap();
    assert_eq!(
        result.argv,
        [
            "ssh",
            "-i",
            "/key dir/it's.key",
            "-o",
            "ProxyCommand=ssh -i /key dir/it's.key -W %h:%p -p %p %h@jump host",
            "-p",
            "-123",
            "-v",
            "comma@comma-0123456789ABCDEF trailing"
        ]
    );
    assert_eq!(
        result.stdout,
        "connecting to 0123456789ABCDEF trailing through jump host:-123 ...\nssh -i '/key dir/it's.key' -o 'ProxyCommand=ssh -i /key dir/it's.key -W %h:%p -p %p %h@jump host' -p -123 -v 'comma@comma-0123456789ABCDEF trailing'\n"
    );
    assert_eq!(
        debug_command(&["ssh".into(), "a\tb".into(), "a'b".into(), "".into()]),
        "ssh a\tb a'b "
    );
}

#[test]
fn auth_config_uses_only_supplied_private_roots_and_failures_become_null() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.json");
    assert!(matches!(
        get_token_at(dir.path()).view(),
        openpilot_log_format::JsonView::Null
    ));
    for content in ["bad json", "{}", "[]", "null", r#"{"access_token":null}"#] {
        std::fs::write(&auth, content).unwrap();
        assert!(matches!(
            get_token_at(dir.path()).view(),
            openpilot_log_format::JsonView::Null
        ));
    }
    for token in [r#""synthetic-token""#, "false", "0", "[]", r#""""#] {
        std::fs::write(&auth, format!(r#"{{"access_token":{token}}}"#)).unwrap();
        assert_eq!(
            get_token_at(dir.path()).to_json().unwrap(),
            json(token).to_json().unwrap()
        );
    }
    assert_eq!(
        config_root(Path::new("/private/home"), "-test", false),
        Path::new("/private/home/.comma-test")
    );
    assert_eq!(
        config_root(Path::new("/private/home"), "-test", true),
        Path::new("/tmp/.comma")
    );
}

#[test]
fn comma_api_checks_json_error_truthiness_instead_of_status_alone() {
    for token in ["null", "false", "0", r#""""#, "[]", "{}"] {
        let mut fake = Fake::new(401, "[]");
        assert!(prepare(
            &mut fake,
            "https://example.invalid",
            &json(token),
            &options("0123456789ABCDEF")
        )
        .unwrap()
        .is_ok());
        assert!(fake.calls.borrow()[0].authorization.is_none());
    }
    let mut fake = Fake::new(200, "[]");
    assert!(prepare(
        &mut fake,
        "https://example.invalid",
        &json(r#""synthetic-token""#),
        &options("0123456789ABCDEF"),
    )
    .unwrap()
    .is_ok());
    assert_eq!(
        fake.calls.borrow()[0].authorization.as_deref(),
        Some("JWT synthetic-token")
    );
    for status in [401, 403] {
        let mut fake = Fake::new(status, r#"{"error":"bad"}"#);
        assert_eq!(
            prepare(&mut fake, "x", &json("null"), &options("id"))
                .err()
                .unwrap()
                .to_string(),
            "Unauthorized. Authenticate with tools/lib/auth.py"
        );
    }
    let mut fake = Fake::new(400, r#"{"error":true,"description":"detail"}"#);
    assert_eq!(
        prepare(&mut fake, "x", &json("null"), &options("id"))
            .err()
            .unwrap()
            .to_string(),
        "400:detail"
    );
    let mut fake = Fake::new(200, "not json");
    assert!(prepare(&mut fake, "x", &json("null"), &options("id")).is_err());
    let mut fake = Fake::new(200, "[]");
    assert!(prepare(&mut fake, "x", &json("true"), &options("id")).is_err());
    assert!(fake.calls.borrow().is_empty());
    fake.fail = true;
    assert!(prepare(&mut fake, "x", &json("null"), &options("0123456789ABCDEF")).is_err());
}

#[test]
fn setup_preserves_status_200_empty_text_and_exact_sequential_writes() {
    let mut fake = Fake::new(200, "");
    let mut writes = Vec::new();
    let mut opened = false;
    let message = setup_keys(&mut fake, "some user/../name", || {
        opened = true;
        Ok(Recording {
            writes: &mut writes,
            fail_at: None,
        })
    })
    .unwrap();
    assert!(opened);
    assert_eq!(message, "Set up ssh keys successfully\n");
    assert_eq!(
        writes,
        [
            ("SshEnabled".into(), b"1".to_vec()),
            ("GithubSshKeys".into(), vec![]),
            ("GithubUsername".into(), b"some user/../name".to_vec())
        ]
    );
    let calls = fake.calls.borrow();
    assert_eq!(calls[0].url, "https://github.com/some user/../name.keys");
    assert_eq!(calls[0].timeout, Some(Duration::from_secs(10)));
    assert_eq!(calls[0].authorization, None);
    for status in [201, 204, 301, 404, 500] {
        let mut fake = Fake::new(status, "ignored");
        assert_eq!(
            setup_keys::<Recording<'_>, _>(&mut fake, "user", || panic!("must not open Params"))
                .unwrap(),
            "Error getting public keys from github\n"
        );
    }
}
struct Recording<'a> {
    writes: &'a mut Vec<(String, Vec<u8>)>,
    fail_at: Option<usize>,
}
impl KeyParams for Recording<'_> {
    fn put(&mut self, key: &str, bytes: &[u8]) -> Result<()> {
        if self.fail_at == Some(self.writes.len()) {
            return Err("synthetic write failure".into());
        }
        self.writes.push((key.into(), bytes.into()));
        Ok(())
    }
}
#[test]
fn setup_write_failure_stops_without_rollback_and_transport_failure_never_opens_params() {
    for fail_at in 0..3 {
        let mut fake = Fake::new(200, " key\r\n\n");
        let mut writes = Vec::new();
        assert!(setup_keys(&mut fake, "user", || Ok(Recording {
            writes: &mut writes,
            fail_at: Some(fail_at)
        }))
        .is_err());
        assert_eq!(writes.len(), fail_at);
        if fail_at == 2 {
            assert_eq!(writes[1].1, b" key\r\n\n");
        }
    }
    let mut fake = Fake::new(200, "ignored");
    fake.fail = true;
    assert!(
        setup_keys::<Recording<'_>, _>(&mut fake, "user", || panic!("must not open Params"))
            .is_err()
    );
}
#[test]
fn actual_params_adapter_is_confined_to_private_temp_root() {
    let dir = tempfile::tempdir().unwrap();
    let mut fake = Fake::new(200, " key\r\n\n");
    setup_keys(&mut fake, "user", || {
        Ok(openpilot_params::Params::open(dir.path(), "synthetic")?)
    })
    .unwrap();
    let params = openpilot_params::Params::open(dir.path(), "synthetic").unwrap();
    assert_eq!(params.get("SshEnabled").unwrap().unwrap(), b"1");
    assert_eq!(params.get("GithubSshKeys").unwrap().unwrap(), b" key\r\n\n");
    assert_eq!(params.get("GithubUsername").unwrap().unwrap(), b"user");
}
#[test]
fn cli_source_options_and_integer_normalization() {
    let args = [
        "--ho=custom",
        "--po",
        " +００２_２ ",
        "--key",
        "key dir",
        "--debug",
        "--",
        "-device",
    ]
    .map(String::from);
    let parsed = parse_options(&args, Path::new("/source")).unwrap().unwrap();
    assert_eq!(
        parsed,
        Options {
            device: "-device".into(),
            host: "custom".into(),
            port: "22".into(),
            key: "key dir".into(),
            debug: true
        }
    );
    assert!(parse_options(&["--help".into()], Path::new("/source"))
        .unwrap()
        .is_none());
    assert!(parse_options(&[], Path::new("/source")).is_err());
    assert!(parse_options(&["id".into(), "--port=4__2".into()], Path::new("/source")).is_err());
    let parsed = parse_options(&["id".into()], Path::new("/source"))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.key, "/source/openpilot/system/hardware/tici/id_rsa");
}

#[test]
fn params_native_io_return_codes_are_discarded_like_cython_wrapper() {
    let dir = tempfile::tempdir().unwrap();
    let mut params = openpilot_params::Params::open(dir.path(), "synthetic").unwrap();
    // Remove only the private namespace link, forcing an inert private write failure.
    std::fs::remove_file(params.directory()).unwrap();
    KeyParams::put(&mut params, "SshEnabled", b"1").unwrap();
    assert!(!params.directory().exists());
}

#[test]
fn blank_alias_query_matches_all_string_aliases_in_dict_order() {
    let devices = json(
        r#"[{"dongle_id":"first","alias":""},{"dongle_id":"second","alias":"A\"B"},{"dongle_id":"third","alias":null}]"#,
    );
    assert_eq!(plan(&devices, &options("  ")).unwrap().unwrap_err().stderr,
        "failed to look up dongle id for \"  \"\nfound multiple matches:\n  \"\" (first)\n  \"A\"B\" (second)\n");
    let single = json(r#"[{"dongle_id":"first","alias":""}]"#);
    assert_eq!(
        plan(&single, &options(""))
            .unwrap()
            .unwrap()
            .argv
            .last()
            .unwrap(),
        "comma@comma-first"
    );
}

// R2 counterexample sources authored BEFORE the R2 production edits. Not executed.
struct FailingText<'a> {
    events: &'a RefCell<Vec<String>>,
}
impl TextDecoder for FailingText<'_> {
    fn decode(&mut self, _: &[u8], _: &str) -> Result<String> {
        self.events.borrow_mut().push("decode".into());
        Err("injected text decoder failure".into())
    }
}
struct OrderedParams<'a> {
    events: &'a RefCell<Vec<String>>,
}
impl KeyParams for OrderedParams<'_> {
    fn put(&mut self, key: &str, _: &[u8]) -> Result<()> {
        self.events.borrow_mut().push(key.into());
        Ok(())
    }
}
#[test]
fn non200_never_accesses_text_or_constructs_params_even_if_decoder_would_fail() {
    for status in [201, 204, 301, 400, 404, 500] {
        let events = RefCell::new(Vec::new());
        let mut transport = Fake::new(status, "not decoded");
        let mut decoder = FailingText { events: &events };
        let output = setup_keys_with_decoder::<OrderedParams<'_>, _>(
            &mut transport,
            "synthetic",
            &mut decoder,
            || {
                events.borrow_mut().push("Params".into());
                Ok(OrderedParams { events: &events })
            },
        )
        .unwrap();
        assert_eq!(output, "Error getting public keys from github\n");
        assert_eq!(transport.calls.borrow().len(), 1);
        assert!(events.borrow().is_empty());
    }
}
#[test]
fn status200_decoder_failure_occurs_after_params_and_enable_only() {
    let events = RefCell::new(Vec::new());
    let mut transport = Fake::new(200, "not decoded");
    let mut decoder = FailingText { events: &events };
    let error = setup_keys_with_decoder(&mut transport, "synthetic", &mut decoder, || {
        events.borrow_mut().push("Params".into());
        Ok(OrderedParams { events: &events })
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "injected text decoder failure");
    assert_eq!(*events.borrow(), ["Params", "SshEnabled", "decode"]);
}
#[test]
fn params_construction_and_enable_errors_precede_text_access() {
    let events = RefCell::new(Vec::new());
    let mut transport = Fake::new(200, "not decoded");
    let mut decoder = FailingText { events: &events };
    let error = setup_keys_with_decoder::<OrderedParams<'_>, _>(
        &mut transport,
        "synthetic",
        &mut decoder,
        || {
            events.borrow_mut().push("Params".into());
            Err("constructor failure".into())
        },
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "constructor failure");
    assert_eq!(*events.borrow(), ["Params"]);
    struct BadEnable;
    impl KeyParams for BadEnable {
        fn put(&mut self, _: &str, _: &[u8]) -> Result<()> {
            Err("enable failure".into())
        }
    }
    events.borrow_mut().clear();
    let error =
        setup_keys_with_decoder(&mut transport, "synthetic", &mut decoder, || Ok(BadEnable))
            .unwrap_err();
    assert_eq!(error.to_string(), "enable failure");
    assert!(events.borrow().is_empty());
}
#[test]
fn default_text_decoding_replaces_invalid_bytes_and_unknown_codec_falls_back() {
    // Malformed bytes are NOT a synthetic decoder exception: Requests uses errors=replace.
    let mut decoder = RequestsText;
    for (bytes, content_type, expected) in [
        (
            &b"key\xff\xe2\x82"[..],
            "text/plain; charset=utf-8",
            "key\u{fffd}\u{fffd}",
        ),
        (
            &b"key\xff"[..],
            "text/plain; charset=not-a-real-codec",
            "key\u{fffd}",
        ),
        (&b"key\xff"[..], "text/plain; charset=ascii", "key\u{fffd}"),
        (&b"key\xff"[..], "text/plain", "keyÿ"),
        (&b""[..], "text/plain; charset=not-a-real-codec", ""),
    ] {
        assert_eq!(decoder.decode(bytes, content_type).unwrap(), expected);
    }
}
#[test]
fn raw_200_default_decoder_writes_replacement_text_after_enable() {
    struct Raw;
    impl Transport for Raw {
        fn get(&mut self, _: &Get) -> Result<Response> {
            Ok(Response {
                status: 200,
                body: b" key\xff\r\n".to_vec(),
                content_type: "text/plain; charset=utf-8".into(),
            })
        }
    }
    let mut writes = Vec::new();
    setup_keys(&mut Raw, "synthetic", || {
        Ok(Recording {
            writes: &mut writes,
            fail_at: None,
        })
    })
    .unwrap();
    assert_eq!(
        writes,
        [
            ("SshEnabled".into(), b"1".to_vec()),
            (
                "GithubSshKeys".into(),
                " key\u{fffd}\r\n".as_bytes().to_vec()
            ),
            ("GithubUsername".into(), b"synthetic".to_vec()),
        ]
    );
}
#[test]
fn port_type_actions_before_help_still_fail_and_after_help_do_not_run() {
    for arguments in [
        vec!["--port", "bad", "--help"],
        vec!["--port=bad", "--help"],
        vec!["--po=bad", "-h"],
        vec!["device", "--port", "bad", "--port", "22", "--help"],
    ] {
        let arguments = arguments.into_iter().map(String::from).collect::<Vec<_>>();
        let error = parse_options(&arguments, Path::new("/source"))
            .expect_err("port conversion must run before help");
        assert!(error.to_string().contains("invalid int value"));
    }
    for arguments in [
        vec!["--help", "--port", "bad"],
        vec!["-h", "--port=bad"],
        vec!["--port=22", "--help", "--port=bad"],
    ] {
        let arguments = arguments.into_iter().map(String::from).collect::<Vec<_>>();
        assert!(parse_options(&arguments, Path::new("/source"))
            .unwrap()
            .is_none());
    }
    let arguments = ["device", "--port=21", "--port=23"].map(String::from);
    assert_eq!(
        parse_options(&arguments, Path::new("/source"))
            .unwrap()
            .unwrap()
            .port,
        "23"
    );
}

// R3 source-first cases: CPython3.12 input limits, not executed locally.
#[test]
fn default_decimal_limit_accepts4300_and_rejects4301_before_bigint() {
    assert_eq!(DEFAULT_MAX_STR_DIGITS, 4300);
    assert_eq!(STR_DIGITS_CHECK_THRESHOLD, 640);
    let limit = IntStringLimit::default();
    assert_eq!(limit.max_digits(), 4300);
    assert_eq!(
        parse_decimal(&"9".repeat(4300), limit).unwrap(),
        "9".repeat(4300)
    );
    let error = parse_decimal(&"9".repeat(4301), limit).unwrap_err();
    assert_eq!(
        error,
        DecimalError::DigitLimit {
            limit: 4300,
            digits: 4301
        }
    );
    assert_eq!(error.to_string(), "Exceeds the limit (4300 digits) for integer string conversion: value has 4301 digits; use sys.set_int_max_str_digits() to increase the limit");
}
#[test]
fn leading_zeroes_are_digits_even_when_result_is_small_or_zero() {
    let limit = IntStringLimit::default();
    assert_eq!(parse_decimal(&"0".repeat(4300), limit).unwrap(), "0");
    assert_eq!(
        parse_decimal(&format!("{}12", "0".repeat(4298)), limit).unwrap(),
        "12"
    );
    for text in [
        "0".repeat(4301),
        format!("{}12", "0".repeat(4299)),
        format!("-{}", "0".repeat(4301)),
    ] {
        assert_eq!(
            parse_decimal(&text, limit).unwrap_err(),
            DecimalError::DigitLimit {
                limit: 4300,
                digits: 4301
            }
        );
    }
}
#[test]
fn signs_whitespace_and_underscores_do_not_count_unicode_decimal_digits_do() {
    let limit = IntStringLimit::default();
    let grouped = (0..4300).map(|_| "９").collect::<Vec<_>>().join("_");
    let text = format!("\t\u{a0}-{}\u{3000}\r\n", grouped);
    assert_eq!(
        parse_decimal(&text, limit).unwrap(),
        format!("-{}", "9".repeat(4300))
    );
    let extra = format!("+{}_０", grouped);
    assert_eq!(
        parse_decimal(&extra, limit).unwrap_err(),
        DecimalError::DigitLimit {
            limit: 4300,
            digits: 4301
        }
    );
    assert_eq!(parse_decimal(" +٠_۰_０_𝟎 ", limit).unwrap(), "0");
    // Numeric-looking non-Nd characters do not become decimal digits.
    for invalid in ["²", "①", "１_²", "\u{1c}12", "12\u{1f}"] {
        assert!(matches!(
            parse_decimal(invalid, limit),
            Err(DecimalError::InvalidLiteral(_))
        ));
    }
}
#[test]
fn invalid_decimal_syntax_precedes_digit_limit() {
    let digits = "1".repeat(4301);
    for text in [
        format!("{digits}_"),
        format!("{digits}x"),
        format!("_{digits}"),
        format!("{digits}__0"),
        format!("{digits} 0"),
    ] {
        assert!(matches!(
            parse_decimal(&text, IntStringLimit::default()),
            Err(DecimalError::InvalidLiteral(_))
        ));
    }
}
#[test]
fn startup_environment_decimal_parser_preserves_cpython_valid_empty_and_zero_rules() {
    use std::ffi::OsStr;
    for (input, expected) in [
        (None, 4300),
        (Some(""), 4300),
        (Some("0"), 0),
        (Some("-0"), 0),
        (Some("+000"), 0),
        (Some("640"), 640),
        (Some("\u{b}640"), 640),
        (Some("\t +00640"), 640),
        (Some("4300"), 4300),
        (Some("2147483647"), 2147483647),
    ] {
        assert_eq!(
            IntStringLimit::from_startup_env(input.map(OsStr::new))
                .unwrap()
                .max_digits(),
            expected
        );
    }
    // strtol does not use the decimal digit guard to parse the startup setting.
    let many_zeroes = "0".repeat(5000);
    assert_eq!(
        IntStringLimit::from_startup_env(Some(OsStr::new(&many_zeroes)))
            .unwrap()
            .max_digits(),
        0
    );
    let leading_zeroes = format!("{}640", many_zeroes);
    assert_eq!(
        IntStringLimit::from_startup_env(Some(OsStr::new(&leading_zeroes)))
            .unwrap()
            .max_digits(),
        640
    );
}
#[test]
fn invalid_startup_environment_is_not_a_port_literal_or_a_default_fallback() {
    use std::ffi::OsStr;
    for input in [
        "1",
        "639",
        "-1",
        "-640",
        "640 ",
        "640\u{b}",
        "  ",
        "+",
        "6_40",
        "６４０",
        "0x280",
        "2147483648",
        "999999999999999999999999999999",
    ] {
        let error = IntStringLimit::from_startup_env(Some(OsStr::new(input))).unwrap_err();
        assert_eq!(
            error.to_string(),
            "PYTHONINTMAXSTRDIGITS: invalid limit; must be >= 640 or 0 for unlimited."
        );
    }
    use std::os::unix::ffi::OsStrExt;
    assert!(IntStringLimit::from_startup_env(Some(OsStr::from_bytes(b"\xff640"))).is_err());
}
#[test]
fn explicit_limits_keep_library_pure_and_zero_disables_the_guard() {
    let custom = IntStringLimit::from_max_digits(640).unwrap();
    assert_eq!(
        parse_decimal(&"8".repeat(640), custom).unwrap(),
        "8".repeat(640)
    );
    assert_eq!(
        parse_decimal(&"8".repeat(641), custom).unwrap_err(),
        DecimalError::DigitLimit {
            limit: 640,
            digits: 641
        }
    );
    let unlimited = IntStringLimit::from_max_digits(0).unwrap();
    assert_eq!(
        parse_decimal(&"7".repeat(5000), unlimited).unwrap(),
        "7".repeat(5000)
    );
    assert!(IntStringLimit::from_max_digits(639).is_err());
    assert!(IntStringLimit::from_max_digits(2147483648).is_err());
    assert_eq!(IntStringLimit::default().max_digits(), 4300);
}
#[test]
fn oversized_port_conversion_keeps_help_and_bootstrap_before_api_order() {
    use std::ffi::OsStr;
    let huge = "1".repeat(4301);
    for args in [
        vec!["--port".into(), huge.clone(), "--help".into()],
        vec![
            "device".into(),
            format!("--port={huge}"),
            "--port=22".into(),
            "--help".into(),
        ],
    ] {
        assert!(parse_options(&args, Path::new("/source")).is_err());
    }
    let help_first = vec!["--help".into(), format!("--port={huge}")];
    assert!(parse_options(&help_first, Path::new("/source"))
        .unwrap()
        .is_none());
    let args = vec!["device".into(), format!("--port={huge}")];
    let unlimited = IntStringLimit::from_startup_env(Some(OsStr::new("0"))).unwrap();
    assert_eq!(
        parse_options_with_limit(&args, Path::new("/source"), unlimited)
            .unwrap()
            .unwrap()
            .port,
        huge
    );
    // Exactly the CLI bootstrap sequencing, without reading/mutating the real environment.
    let result = IntStringLimit::from_startup_env(Some(OsStr::new("bad")))
        .map_err(Box::<dyn std::error::Error>::from)
        .and_then(|limit| parse_options_with_limit(&help_first, Path::new("/source"), limit));
    assert!(result
        .unwrap_err()
        .to_string()
        .starts_with("PYTHONINTMAXSTRDIGITS:"));
}
