//! Source-declared QA inputs. No expected result or runtime fixture is loaded.
use super::{Case, Suite};
use crate::seal::Result;
use openpilot_log_format::JsonValue;
use serde_json::{json, Value};
fn add(rows: &mut Vec<Case>, name: &str, input: Value) -> Result<()> {
    rows.push(Case {
        name: name.into(),
        input: JsonValue::parse(&input.to_string())?,
    });
    Ok(())
}
pub(super) fn cases(suite: Suite) -> Result<Vec<Case>> {
    let mut rows = Vec::new();
    if matches!(suite, Suite::Timeouts) {
        add(
            &mut rows,
            "header_timeout_15s",
            json!({"responses":[{"header_delay":16},{}],"minimum_seconds":15}),
        )?;
        add(
            &mut rows,
            "body_timeout_15s",
            json!({"responses":[{"body_delay":16},{}],"minimum_seconds":15}),
        )?;
        add(
            &mut rows,
            "body_progress_exceeds_15s",
            json!({"responses":[{"drip":8}],"minimum_seconds":16}),
        )?;
        return Ok(rows);
    }
    if matches!(suite, Suite::All) {
        add(&mut rows, "clock_pre_epoch", json!({"config":{"utc":-1}}))?;
        add(
            &mut rows,
            "clock_year_one",
            json!({"config":{"utc":-62135596800i64}}),
        )?;
        add(
            &mut rows,
            "clock_last_valid_expiry",
            json!({"config":{"utc":253402297199i64}}),
        )?;
        add(
            &mut rows,
            "clock_expiry_overflow",
            json!({"config":{"utc":253402297200i64,"max_sleeps":2}}),
        )?;
        add(
            &mut rows,
            "clock_year_zero",
            json!({"config":{"utc":-62135596801i64,"max_sleeps":2}}),
        )?;
        add(
            &mut rows,
            "clock_year_ten_thousand",
            json!({"config":{"utc":253402300800i64,"max_sleeps":2}}),
        )?;
        add(
            &mut rows,
            "public_open_precedes_private_decode",
            json!({"private":{"bytes_hex":"ff"},"public_unreadable":true}),
        )?;
        add(
            &mut rows,
            "closed_log_missing_key",
            json!({"keys":[],"config":{"closed_log":true}}),
        )?;
        add(
            &mut rows,
            "closed_log_invalid_param",
            json!({"param":{"bytes_hex":"ff"},"config":{"closed_log":true}}),
        )?;
        add(
            &mut rows,
            "closed_log_auth_info",
            json!({"config":{"closed_log":true,"spinner":true}}),
        )?;
        add(
            &mut rows,
            "closed_log_imei_exception",
            json!({"config":{"closed_log":true,"spinner":true,"imeis":[{}]}}),
        )?;
        add(
            &mut rows,
            "param_write_failure",
            json!({"param_directory":true,"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "fallback_stat_permission_error",
            json!({"fallback_parent_unreadable":true}),
        )?;
        add(
            &mut rows,
            "no_keys_overrides_saved",
            json!({"keys":[],"param":{"bytes_hex":"6f6c64"}}),
        )?;
        add(&mut rows, "missing_public", json!({"no_public":true}))?;
        add(
            &mut rows,
            "empty_public",
            json!({"public":{"bytes_hex":""}}),
        )?;
        add(
            &mut rows,
            "existing_reads_but_does_not_sign",
            json!({"param":{"bytes_hex":"6f6c64"},"private":{"bytes_hex":"6e6f742061206b6579"}}),
        )?;
        add(
            &mut rows,
            "existing_invalid_private_utf8",
            json!({"param":{"bytes_hex":"6f6c64"},"private":{"bytes_hex":"ff"}}),
        )?;
        add(
            &mut rows,
            "existing_invalid_public_utf8",
            json!({"param":{"bytes_hex":"6f6c64"},"public":{"bytes_hex":"ff"}}),
        )?;
        add(
            &mut rows,
            "persist_fallback",
            json!({"fallback":{"bytes_hex":"201c73796e7468657469632d66616c6c6261636b0d0a"}}),
        )?;
        add(
            &mut rows,
            "persist_empty",
            json!({"fallback":{"bytes_hex":"201f0d0a"},"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "persist_invalid_utf8",
            json!({"fallback":{"bytes_hex":"ff"}}),
        )?;
        add(
            &mut rows,
            "invalid_param_falls_back",
            json!({"param":{"bytes_hex":"ff"},"fallback":{"bytes_hex":"66616c6c6261636b"}}),
        )?;
        add(
            &mut rows,
            "empty_param_falls_back",
            json!({"param":{"bytes_hex":""},"fallback":{"bytes_hex":"66616c6c6261636b"}}),
        )?;
        add(
            &mut rows,
            "param_precedes_fallback",
            json!({"param":{"bytes_hex":"70726573656e74"},"fallback":{"bytes_hex":"66616c6c6261636b"}}),
        )?;
        add(&mut rows, "rsa_signed", json!({"config":{"spinner":true}}))?;
        add(&mut rows, "rsa_before_ec", json!({"keys":["ec","rsa"]}))?;
        add(&mut rows, "ec_sec1_signed", json!({"keys":["ec"]}))?;
        add(
            &mut rows,
            "ec_pkcs8_signed",
            json!({"keys":["ec"],"pkcs8":true}),
        )?;
        add(&mut rows, "crlf_keys", json!({"crlf":true}))?;
        add(
            &mut rows,
            "only_second_imei",
            json!({"config":{"imeis":[null,"second +/한"],"serial":"serial +/한"}}),
        )?;
        add(
            &mut rows,
            "empty_imei_is_available",
            json!({"config":{"imeis":["",null]}}),
        )?;
        add(
            &mut rows,
            "both_none_poll_without_sleep",
            json!({"config":{"imeis":[null,null,"first",null],"step":31,"spinner":true}}),
        )?;
        add(
            &mut rows,
            "second_imei_error_discards_first",
            json!({"config":{"imeis":["discard",{},null,"second"],"step":61,"spinner":true}}),
        )?;
        add(
            &mut rows,
            "first_imei_error_skips_second",
            json!({"config":{"imeis":[{},null,"second"],"spinner":true}}),
        )?;
        add(
            &mut rows,
            "serial_error_has_no_explicit_close",
            json!({"config":{"serial":{},"spinner":true}}),
        )?;
        add(
            &mut rows,
            "spinner_start_error",
            json!({"config":{"spinner":true,"spinner_fail":"spinner_start"}}),
        )?;
        add(
            &mut rows,
            "spinner_update_error",
            json!({"config":{"spinner":true,"spinner_fail":"spinner_update"}}),
        )?;
        add(
            &mut rows,
            "spinner_close_error_prevents_persist",
            json!({"config":{"spinner":true,"spinner_fail":"spinner_close"}}),
        )?;
        add(
            &mut rows,
            "invalid_key_retries_before_http",
            json!({"private":{"bytes_hex":"696e76616c6964"},"config":{"max_sleeps":2,"spinner":true}}),
        )?;
        add(
            &mut rows,
            "retry_backoff_cap",
            json!({"responses":[{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{"body":"{}"},{}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "retry_http_error",
            json!({"responses":[{"disconnect":true},{}]}),
        )?;
        add(
            &mut rows,
            "retry_invalid_json",
            json!({"responses":[{"body":"invalid"},{}]}),
        )?;
        add(
            &mut rows,
            "retry_nonobject",
            json!({"responses":[{"body":"[]"},{}]}),
        )?;
        add(
            &mut rows,
            "deflate_json",
            json!({"responses":[{"deflate":true}]}),
        )?;
        add(
            &mut rows,
            "raw_deflate_json",
            json!({"responses":[{"deflate":"raw"}]}),
        )?;
        add(
            &mut rows,
            "brotli_json",
            json!({"responses":[{"brotli":true}]}),
        )?;
        add(&mut rows, "gzip_json", json!({"responses":[{"gzip":true}]}))?;
        add(
            &mut rows,
            "utf7_valid_shift",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글😀\"}","encoding":"utf-7","content_type":"application/json; charset=utf-7"}]}),
        )?;
        add(
            &mut rows,
            "utf7_malformed_shift",
            json!({"responses":[{"body":"{\"dongle_id\":\"+A-\"}","content_type":"application/json; charset=utf-7"}]}),
        )?;
        add(
            &mut rows,
            "utf7_direct_nonascii",
            json!({"responses":[{"body_hex":"7b22646f6e676c655f6964223a22ff227d","content_type":"application/json; charset=utf-7"}]}),
        )?;
        add(
            &mut rows,
            "utf7_unpaired_surrogate",
            json!({"config":{"spinner":true},"responses":[{"body":"{\"dongle_id\":\"+2AA-\"}","content_type":"application/json; charset=utf-7"}]}),
        )?;
        add(
            &mut rows,
            "utf7_surrogate_invalid_escape",
            json!({"responses":[{"body_hex":"7b22646f6e676c655f6964223a225c2b3241412d227d","content_type":"application/json; charset=utf-7"},{}]}),
        )?;
        add(
            &mut rows,
            "utf7_surrogate_after_backslash",
            json!({"config":{"spinner":true},"responses":[{"body_hex":"7b22646f6e676c655f6964223a225c5c2b3241412d227d","content_type":"application/json; charset=utf-7"}]}),
        )?;
        add(
            &mut rows,
            "malformed_cp932",
            json!({"responses":[{"body_hex":"7b22646f6e676c655f6964223a2282a08120227d","content_type":"application/json; charset=cp932"}]}),
        )?;
        add(
            &mut rows,
            "malformed_shift_jis",
            json!({"responses":[{"body_hex":"7b22646f6e676c655f6964223a2282a08120227d","content_type":"application/json; charset=shift_jis"}]}),
        )?;
        add(
            &mut rows,
            "explicit_iso8859_1",
            json!({"responses":[{"body":"{\"dongle_id\":\"\"}","encoding":"latin1","content_type":"application/json; charset=iso8859-1"}]}),
        )?;
        add(
            &mut rows,
            "explicit_latin_1",
            json!({"responses":[{"body":"{\"dongle_id\":\"\"}","encoding":"latin1","content_type":"application/json; charset=latin_1"}]}),
        )?;
        add(
            &mut rows,
            "explicit_cp819",
            json!({"responses":[{"body":"{\"dongle_id\":\"\"}","encoding":"latin1","content_type":"application/json; charset=cp819"}]}),
        )?;
        add(
            &mut rows,
            "explicit_utf32_le",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글😀\"}","encoding":"utf-32-le","content_type":"application/json; charset=utf-32-le"}]}),
        )?;
        add(
            &mut rows,
            "explicit_utf32_be",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글😀\"}","encoding":"utf-32-be","content_type":"application/json; charset=UTF-32-BE"}]}),
        )?;
        add(
            &mut rows,
            "explicit_utf32_bom",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글😀\"}","encoding":"utf-32","content_type":"application/json; charset=utf-32"}]}),
        )?;
        add(
            &mut rows,
            "explicit_utf32_replace",
            json!({"responses":[{"body_hex":"7b00000022000000640000006f0000006e000000670000006c000000650000005f0000006900000064000000220000003a0000002200000000d8000000001100220000007d000000","content_type":"application/json; charset=utf-32-le"}]}),
        )?;
        add(
            &mut rows,
            "explicit_unknown_utf8_fallback",
            json!({"responses":[{"body":"{\"dongle_id\":\"\"}","encoding":"latin1","content_type":"application/json; charset=unknown-encoding"}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_ascii",
            json!({"responses":[{"content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_korean",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_utf8_bom",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","encoding":"utf-8-sig","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_utf16_bom",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}","encoding":"utf-16","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_cp1251",
            json!({"responses":[{"body":"{\"dongle_id\":\"идентификатор\"}","encoding":"cp1251","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_cp949",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}","encoding":"cp949","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_latin1",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","encoding":"latin1","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "no_content_type_utf8",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","content_type":null}]}),
        )?;
        add(
            &mut rows,
            "json_default_utf8",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","encoding":"latin1"}]}),
        )?;
        add(
            &mut rows,
            "html_default_latin1",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","encoding":"latin1","content_type":"text/html"}]}),
        )?;
        add(
            &mut rows,
            "latin1_json",
            json!({"responses":[{"body":"{\"dongle_id\":\"café\"}","encoding":"latin1","content_type":"text/plain"}]}),
        )?;
        add(
            &mut rows,
            "utf16_json",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}","encoding":"utf-16","content_type":"application/json; charset=utf-16"}]}),
        )?;
        add(
            &mut rows,
            "utf8_bom_sig_json",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}","encoding":"utf-8-sig","content_type":"application/json; charset=utf-8-sig"}]}),
        )?;
        add(
            &mut rows,
            "utf8_json",
            json!({"responses":[{"body":"{\"dongle_id\":\"한글\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_200",
            json!({"responses":[{"status":200,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_201",
            json!({"responses":[{"status":201,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_400",
            json!({"responses":[{"status":400,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_401",
            json!({"responses":[{"status":401,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_402",
            json!({"responses":[{"status":402,"body":"not json"}]}),
        )?;
        add(
            &mut rows,
            "status_403",
            json!({"responses":[{"status":403,"body":"not json"}]}),
        )?;
        add(
            &mut rows,
            "status_404",
            json!({"responses":[{"status":404,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "status_500",
            json!({"responses":[{"status":500,"body":"{\"dongle_id\":\"status-id\"}"}]}),
        )?;
        add(
            &mut rows,
            "redirect_301",
            json!({"responses":[{"status":301,"location":"/redirected"},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_302",
            json!({"responses":[{"status":302,"location":"/redirected"},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_303",
            json!({"responses":[{"status":303,"location":"/redirected"},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_307",
            json!({"responses":[{"status":307,"location":"/redirected"},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_308",
            json!({"responses":[{"status":308,"location":"/redirected"},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_cookie",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=synthetic; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_applicability_92",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=deleted; Max-Age=0; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_applicability_93",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=old; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_applicability_94",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=secure; Secure; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_applicability_95",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=path; Path=/elsewhere"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_applicability_96",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=domain; Domain=example.invalid; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_path_order",
            json!({"responses":[{"status":302,"location":"/cookie/next","headers":[["Set-Cookie","root=first; Path=/"],["Set-Cookie","specific=second; Path=/cookie"]]},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_delete",
            json!({"responses":[{"status":302,"location":"/cookie","headers":{"Set-Cookie":"test=value; Path=/"}},{"status":302,"location":"/cookie/next","headers":{"Set-Cookie":"test=deleted; Max-Age=0; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "cookie_reset_on_retry",
            json!({"responses":[{"body":"invalid","headers":{"Set-Cookie":"test=value; Path=/"}},{}]}),
        )?;
        add(
            &mut rows,
            "redirect_limit",
            json!({"responses":[{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{"status":302,"location":"/loop"},{}]}),
        )?;
        add(
            &mut rows,
            "json_value_101",
            json!({"responses":[{"body":"{\"dongle_id\":null}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_102",
            json!({"responses":[{"body":"{\"dongle_id\":false}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_103",
            json!({"responses":[{"body":"{\"dongle_id\":0}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_104",
            json!({"responses":[{"body":"{\"dongle_id\":-0.0}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_105",
            json!({"responses":[{"body":"{\"dongle_id\":\"\"}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_106",
            json!({"responses":[{"body":"{\"dongle_id\":[]}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_107",
            json!({"responses":[{"body":"{\"dongle_id\":{}}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_108",
            json!({"responses":[{"body":"{\"dongle_id\":1}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_109",
            json!({"responses":[{"body":"{\"dongle_id\":true}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_110",
            json!({"responses":[{"body":"{\"dongle_id\":[1]}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_111",
            json!({"responses":[{"body":"{\"dongle_id\":{\"a\":1}}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_112",
            json!({"responses":[{"body":"{\"dongle_id\":NaN}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "json_value_113",
            json!({"responses":[{"body":"{\"dongle_id\":\"\\ud800\"}"}],"config":{"spinner":true}}),
        )?;
        add(
            &mut rows,
            "is_registered_114",
            json!({"config":{"mode":"is_registered"}}),
        )?;
        add(
            &mut rows,
            "is_registered_115",
            json!({"config":{"mode":"is_registered"},"param":{"bytes_hex":""}}),
        )?;
        add(
            &mut rows,
            "is_registered_116",
            json!({"config":{"mode":"is_registered"},"param":{"bytes_hex":"ff"}}),
        )?;
        add(
            &mut rows,
            "is_registered_117",
            json!({"config":{"mode":"is_registered"},"param":{"bytes_hex":"556e72656769737465726564446576696365"}}),
        )?;
        add(
            &mut rows,
            "is_registered_118",
            json!({"config":{"mode":"is_registered"},"param":{"bytes_hex":"72656769737465726564"}}),
        )?;
    }
    add(
        &mut rows,
        "hardware_string",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a2266697874757265202b2fed959c227d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_null",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a6e756c6c7d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_empty",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a22227d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_false",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a66616c73657d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_integer",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a3132333435363738393031323334353637383930313233343536373839307d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_float",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a2d302e307d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_array",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a5b6e756c6c2c2066616c73652c2022222c205b22ed959c222c206e756c6c5d2c207b2271756f74655c22223a747275657d2c204e614e5d7d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_object",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a7b2261202b2fed959c223a372c227365636f6e64223a6e756c6c7d7d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_empty_array",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a5b5d7d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_surrogate_query",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"7b22696d6569223a225c7564383030227d"}},"config":{"spinner":false,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_modem_error",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"616e64726f6964626f6f742e73657269616c6e6f3d66697874757265"},"dev/shm/modem":{"bytes_hex":"5b5d"}},"config":{"spinner":true,"step":61,"max_sleeps":1}}),
    )?;
    add(
        &mut rows,
        "hardware_serial_error",
        json!({"hardware":{"proc/cmdline":{"bytes_hex":"6f746865723d76616c7565"}},"config":{"spinner":true}}),
    )?;
    Ok(rows)
}
