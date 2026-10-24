//! Source-derived timed QA orchestration. Expected corpora are test inputs only.
use crate::{
    argparse_scan::{scan_os_with_positionals, ArgumentError, OptionSpec},
    control_tools_policy_check::json as pyjson,
    qa_process::{CommandSpec, NativeRunner, Runner},
    seal::Result,
    timed_fixtures::{self, Fixture},
    timed_reference,
};
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
#[path = "timed_http_check.rs"]
mod http;

#[derive(Clone, Copy)]
pub enum Kind {
    Reference,
    Dates,
    Http,
    Logging,
}
#[derive(Debug)]
struct ComparisonFailure(String);
impl std::fmt::Display for ComparisonFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("timed local date comparison failed")
    }
}
impl std::error::Error for ComparisonFailure {}
#[derive(Clone)]
pub struct Options {
    pub root: PathBuf,
    pub binary: PathBuf,
    pub output: PathBuf,
    pub reference: PathBuf,
    pub command: PathBuf,
    pub worker: Vec<String>,
}
fn require(value: bool, reason: &str) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn node(value: &Value) -> Result<JsonValue> {
    Ok(JsonValue::parse(&serde_json::to_string(value)?)?)
}
fn array(values: &[JsonValue]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "[{}]",
        values
            .iter()
            .map(JsonValue::to_json)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join(",")
    ))?)
}
fn object(values: &[(&str, JsonValue)]) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| Ok(format!(
                "{}:{}",
                serde_json::to_string(key)?,
                value.to_json()?
            )))
            .collect::<Result<Vec<_>>>()?
            .join(",")
    ))?)
}
fn field(value: &JsonValue, key: &str) -> Result<JsonValue> {
    value
        .get(key)
        .ok_or_else(|| format!("missing {key}").into())
}
fn nullable(value: &JsonValue, key: &str) -> Result<JsonValue> {
    require(value.is_object(), "expected object for .get")?;
    Ok(value
        .get(key)
        .unwrap_or_else(|| JsonValue::parse("null").unwrap()))
}
fn is_null(value: &JsonValue) -> bool {
    matches!(value.view(), JsonView::Null)
}
fn contains(value: &JsonValue, key: &str) -> bool {
    match value.view() {
        JsonView::Object(_) => value.get(key).is_some(),
        JsonView::Array(rows) => rows.iter().any(|row| row.text_eq(key)),
        JsonView::Text(points) => {
            let needle = key.chars().map(u32::from).collect::<Vec<_>>();
            points.windows(needle.len()).any(|part| part == needle)
        }
        _ => false,
    }
}
fn propagated(value: &JsonValue) -> Result<JsonValue> {
    let JsonView::Object(entries) = value.view() else {
        return Err("error result is not a mapping".into());
    };
    let text = entries
        .into_iter()
        .map(|(key, value)| {
            let key = JsonValue::codepoints(key.to_vec()).ok_or("invalid result key")?;
            let value = if key.text_eq("error") {
                JsonValue::text("<propagated>")
            } else {
                value
            };
            Ok(format!("{}:{}", key.to_json()?, value.to_json()?))
        })
        .collect::<Result<Vec<_>>>()?
        .join(",");
    Ok(JsonValue::parse(&format!("{{{text}}}"))?)
}
fn write_json(path: &Path, value: &JsonValue) -> Result<()> {
    fs::write(path, format!("{}\n", pyjson::pretty(value, 0)?))?;
    Ok(())
}
fn read_text(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn normalized(fixture: &Fixture, records: &[JsonValue]) -> Result<JsonValue> {
    let mut params = vec![];
    for name in ["TimezoneName", "TimezoneSource"] {
        let path = fixture.params.join(name);
        params.push((
            name,
            if path.exists() {
                JsonValue::text(&read_text(&path)?)
            } else {
                JsonValue::parse("null")?
            },
        ));
    }
    let root = fixture.directory.to_str().ok_or("invalid fixture root")?;
    let link = Path::new(
        fixture.config["paths"]["localtime"]
            .as_str()
            .ok_or("missing localtime path")?,
    );
    let target = if link.is_symlink() {
        // Preserve filesystem surrogateescape codepoints in arbitrary targets.
        let points = crate::python_fs::codepoints(fs::read_link(link)?.as_os_str());
        let needle = root.chars().map(u32::from).collect::<Vec<_>>();
        let replacement = "<root>".chars().map(u32::from).collect::<Vec<_>>();
        let mut out = vec![];
        let mut index = 0;
        while index < points.len() {
            if points[index..].starts_with(&needle) {
                out.extend_from_slice(&replacement);
                index += needle.len();
            } else {
                out.push(points[index]);
                index += 1;
            }
        }
        JsonValue::codepoints(out).ok_or("invalid target codepoints")?
    } else {
        JsonValue::parse("null")?
    };
    let mut commands = vec![];
    if fixture.directory.join("commands.jsonl").exists() {
        for line in read_text(&fixture.directory.join("commands.jsonl"))?.lines() {
            let row = pyjson::parse_text(line)?;
            let mut args = vec![];
            for value in pyjson::sequence(&row)? {
                let text = value.to_utf8().ok_or("non-text command argument")?;
                args.push(JsonValue::text(&text.replace(root, "<root>")));
            }
            commands.push(array(&args)?);
        }
    }
    object(&[
        ("params", object(&params)?),
        ("target", target),
        ("commands", array(&commands)?),
        ("records", array(records)?),
    ])
}
fn record_view(records: &[JsonValue]) -> Result<Vec<JsonValue>> {
    records
        .iter()
        .map(|row| {
            object(&[
                ("level", field(row, "levelnum")?),
                ("msg", field(row, "msg")?),
                ("exception", node(&json!(contains(row, "exc_info")))?),
            ])
        })
        .collect()
}
fn with_values(values: JsonValue, state: &JsonValue) -> Result<JsonValue> {
    object(&[
        ("values", values),
        ("params", field(state, "params")?),
        ("target", field(state, "target")?),
        ("commands", field(state, "commands")?),
        ("records", field(state, "records")?),
    ])
}
// Source children use serde for their internal protocol. Rebuild the original
// checker dictionaries in insertion order before emitting Python-style reports.
fn source_state(value: &JsonValue) -> Result<JsonValue> {
    let records = pyjson::sequence(&field(value, "records")?)?
        .iter()
        .map(|row| {
            object(&[
                ("level", field(row, "level")?),
                ("msg", field(row, "msg")?),
                ("exception", field(row, "exception")?),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    object(&[
        ("params", field(value, "params")?),
        ("target", field(value, "target")?),
        ("commands", field(value, "commands")?),
        ("records", array(&records)?),
    ])
}
fn initial() -> Value {
    json!({"params_hex":{},"command_fail":null,"sudo_exists":true,"localtime":null,"systemd_mtime_ns":null})
}
fn request(name: String, actions: Value, zone: &str, loop_mode: bool, initial: Value) -> Value {
    let mut config = timed_fixtures::default_config(Path::new("<root>"));
    config["actions"] = actions;
    json!({"name":name,"timezone":zone,"config":config,"initial":initial,"loop":loop_mode})
}
fn step(now: i64, changes: Value) -> Value {
    let mut gps = json!({"updated":true,"has_fix":true,"log_mono_time":now,"longitude":127.0,"unix_timestamp_millis":1790000000000i64});
    for (key, value) in changes.as_object().unwrap() {
        gps[key] = value.clone();
    }
    json!({"kind":"step","monotonic":now,"gps":gps})
}
fn apply(zone: &str, source: &str) -> Value {
    json!({"kind":"apply","zone":zone,"source":source})
}
pub fn reference_cases() -> Vec<Value> {
    let mut cases = vec![];
    let mut add = |name: &str, actions: Value, setup: Value| {
        cases.push(request(name.into(), actions, "UTC", false, setup))
    };
    let longitudes: [f64; 17] = [
        -1000., -187.5, -180., -172.5, -37.5, -22.5, -7.500001, -7.5, 0., 7.5, 7.500001, 22.5,
        37.5, 127., 202.5, 210., 1000.,
    ];
    // Integral literals in the original fixed requests remain integers.
    let actions=longitudes.into_iter().map(|value|json!({"kind":"gps","longitude":if value.fract()==0. {json!(value as i64)}else{json!(value)}})).collect::<Vec<_>>();
    add("gps-banker-rounding", json!(actions), initial());
    add(
        "precedence",
        json!([
            apply("bad", "app"),
            apply("Etc/GMT-9", "gps"),
            apply("Asia/Seoul", "wifi"),
            apply("Etc/GMT", "gps"),
            apply("America/New_York", "app"),
            apply("Asia/Seoul", "wifi"),
            apply("America/New_York", "app"),
            apply("bad", "unknown")
        ]),
        initial(),
    );
    add(
        "same-zone-promote",
        json!([
            apply("Asia/Seoul", "gps"),
            apply("Asia/Seoul", "wifi"),
            apply("Asia/Seoul", "app")
        ]),
        initial(),
    );
    let mut setup = initial();
    setup["localtime"] = json!("<root>/etc/absent");
    add("dangling-link", json!([apply("Asia/Seoul", "wifi")]), setup);
    for command in ["rm", "ln", "date"] {
        let mut setup = initial();
        setup["command_fail"] = json!(command);
        add(
            &format!("command-failure-{command}"),
            if command == "date" {
                json!([{"kind":"set_time","epoch":1790000030.9}])
            } else {
                json!([apply("Asia/Seoul", "wifi")])
            },
            setup,
        );
    }
    let mut setup = initial();
    setup["sudo_exists"] = json!(false);
    add(
        "missing-sudo",
        json!([{"kind":"set_time","epoch":1790000030.9}]),
        setup,
    );
    let differences: [f64; 7] = [-10.0001, -10., -9.999, 0., 9.999, 10., 10.999];
    add("clock-diff-boundaries",json!(differences.into_iter().map(|diff|json!({"kind":"set_time","epoch":if diff.fract()==0.{json!(1790000000i64+diff as i64)}else{json!(1790000000.+diff)}})).collect::<Vec<_>>()),initial());
    for zone in ["UTC", "Asia/Seoul", "America/New_York"] {
        for epoch in [
            1740095999i64,
            1740096000,
            1740096001,
            2051222399,
            2051222400,
            2051222401,
        ] {
            let mut row = request(
                format!("wall-{}-{epoch}", zone.replace('/', "-")),
                json!([{"kind":"valid"},{"kind":"bounds"}]),
                zone,
                false,
                initial(),
            );
            row["config"]["wall"] = json!(epoch * 1_000_000_000);
            cases.push(row);
        }
        for epoch in [0i64, 1740096000, 1772951400, 1793500200, 2051222400] {
            let mut setup = initial();
            setup["systemd_mtime_ns"] = json!(epoch * 1_000_000_000);
            cases.push(request(
                format!("systemd-{}-{epoch}", zone.replace('/', "-")),
                json!([{"kind":"bounds"},{"kind":"valid"}]),
                zone,
                false,
                setup,
            ));
        }
    }
    cases.push(request(
        "loop-retries-freshness".into(),
        json!([
            step(30_000_000_000, json!({"updated":false})),
            step(30_000_000_001, json!({"has_fix":false})),
            step(60_000_000_001, json!({"log_mono_time":58_000_000_000i64})),
            step(60_000_000_002, json!({"log_mono_time":58_000_000_002i64})),
            step(360_000_000_002, json!({"unix_timestamp_millis":0})),
            step(360_000_000_003, json!({"log_mono_time":400_000_000_000i64})),
            step(400_000_000_000, json!({"updated":false})),
            step(401_000_000_000, json!({"has_fix":false}))
        ]),
        "UTC",
        true,
        initial(),
    ));
    for source in ["app", "wifi", "gps", "unknown"] {
        let mut setup = initial();
        setup["params_hex"]["TimezoneSource"] = json!(source
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>());
        cases.push(request(
            format!("loop-source-{source}"),
            json!([
                step(301_000_000_000, json!({})),
                step(302_000_000_000, json!({}))
            ]),
            "UTC",
            true,
            setup,
        ));
    }
    for epoch in [
        1740095999999i64,
        1740096000000,
        1740096000001,
        2051222399999,
        2051222400000,
        2051222400001,
    ] {
        cases.push(request(
            format!("gps-date-{epoch}"),
            json!([step(10_000_000_000, json!({"unix_timestamp_millis":epoch}))]),
            "UTC",
            true,
            initial(),
        ));
    }
    let mut setup = initial();
    setup["params_hex"]["UbloxAvailable"] = json!("31");
    cases.push(request(
        "loop-ublox-selection".into(),
        json!([step(10_000_000_000, json!({"updated":false}))]),
        "UTC",
        true,
        setup,
    ));
    cases
}
pub fn date_cases() -> Vec<Value> {
    let mut offsets = vec![
        -86400001i64,
        -86400000,
        -50400001,
        -50400000,
        -43200001,
        -43200000,
        -1,
        0,
        1,
        43200000,
        43200001,
        50400000,
        50400001,
        86400000,
    ];
    for boundary in [
        36000000, 86400000, 129600000, 55928000, 104162000, -32400000, 18000000,
    ] {
        offsets.extend([boundary - 1, boundary, boundary + 1]);
    }
    offsets.sort();
    offsets.dedup();
    let mut epochs = vec![];
    for center in [-62135596800000i64, 253402300800000] {
        epochs.extend(offsets.iter().map(|offset| center + offset));
    }
    epochs.extend([-62167219200000, -62167219200001, -62198755200000, 0]);
    ["UTC","Etc/GMT-14","Etc/GMT+12","Asia/Seoul","America/New_York"].into_iter().flat_map(|zone|epochs.iter().map(move|millis|json!({"zone":zone,"milliseconds":millis,"action":step(10_000_000_000,json!({"unix_timestamp_millis":millis}))}))).collect()
}
fn reference(options: &Options, runner: &mut dyn Runner) -> Result<String> {
    let cases = reference_cases();
    for request in &cases {
        let name = request["name"].as_str().ok_or("missing scenario name")?;
        let source_directory = tempfile::tempdir()?;
        let source = timed_reference::run_case(
            &options.root,
            request,
            source_directory.path(),
            &options.reference,
            &options.command,
        )?;
        let native_directory = tempfile::tempdir()?;
        let mut fixture = Fixture::new(
            native_directory.path(),
            request["timezone"].as_str().ok_or("missing timezone")?,
            &options.command,
        )?;
        fixture.prepare(request)?;
        let (rows, records) = timed_fixtures::native(
            &fixture,
            &options.binary,
            &options.output.join(format!("{name}.jsonl")),
            runner,
        )?;
        let mut values = rows
            .iter()
            .map(|row| field(row, "result"))
            .collect::<Result<Vec<_>>>()?;
        let source_values = if request["loop"] == true {
            let values = node(&source["values"])?;
            object(&[
                ("service", field(&values, "service")?),
                ("publications", field(&values, "publications")?),
                ("sleeps", field(&values, "sleeps")?),
                ("last_attempt", field(&values, "last_attempt")?),
            ])?
        } else {
            node(&source["values"])?
        };
        let mut source_values_out = source_values.clone();
        if request["loop"] == true {
            let last = rows.last().ok_or("empty timed loop output")?;
            require(
                pyjson::equal(
                    &field(last, "last_attempt")?,
                    &field(&source_values, "last_attempt")?,
                ),
                "last_attempt differs",
            )?;
            require(
                pyjson::equal(&field(last, "sleeps")?, &field(&source_values, "sleeps")?),
                "sleeps differ",
            )?;
            require(
                values.iter().all(|value| contains(value, "ok")),
                "loop result lacks ok",
            )?;
        } else {
            let mut expected = pyjson::sequence(&source_values)?;
            require(
                expected.len() == values.len(),
                "timed result cardinality differs",
            )?;
            for (left, right) in expected.iter_mut().zip(&mut values) {
                require(
                    contains(left, "error") == contains(right, "error"),
                    "error occurrence differs",
                )?;
                if contains(left, "error") {
                    *left = propagated(left)?;
                    *right = propagated(right)?;
                }
            }
            source_values_out = array(&expected)?;
        }
        let source_result = with_values(
            source_values_out.clone(),
            &source_state(&node(&source["state"])?)?,
        )?;
        let state = normalized(&fixture, &record_view(&records)?)?;
        let native_result = with_values(
            if request["loop"] == true {
                source_values_out
            } else {
                array(&values)?
            },
            &state,
        )?;
        require(
            pyjson::equal(&source_result, &native_result),
            "timed source/native policy differs",
        )?;
        write_json(
            &options.output.join(format!("{name}.comparison.json")),
            &array(&[source_result, native_result])?,
        )?;
    }
    write_json(
        &options.output.join("summary.json"),
        &object(&[
            ("passed", node(&json!(true))?),
            ("source_native_scenarios", node(&json!(cases.len()))?),
        ])?,
    )?;
    Ok(format!(
        "PASS: {} original-source/native scenarios\n",
        cases.len()
    ))
}
fn dates(options: &Options, runner: &mut dyn Runner) -> Result<String> {
    let mut reports = vec![];
    let mut failures = vec![];
    for case in date_cases() {
        let zone = case["zone"].as_str().ok_or("missing timezone")?;
        let millis = case["milliseconds"].as_i64().ok_or("missing date")?;
        let output = options
            .output
            .join(format!("{}-{millis}", zone.replace('/', "-")));
        fs::create_dir_all(&output)?;
        let mut setup = initial();
        setup["params_hex"]["TimezoneSource"] = json!("617070");
        let request = request(String::new(), json!([case["action"]]), zone, true, setup);
        let directory = tempfile::tempdir()?;
        let source = timed_reference::run_case(
            &options.root,
            &request,
            directory.path(),
            &options.reference,
            &options.command,
        )?;
        let source_result = object(&[
            ("error", node(&source["errors"][0]["message"])?),
            ("sleeps", node(&source["values"]["sleeps"])?),
            ("state", source_state(&node(&source["state"])?)?),
        ])?;
        let directory = tempfile::tempdir()?;
        let mut fixture = Fixture::new(directory.path(), zone, &options.command)?;
        fixture.prepare(&request)?;
        let (rows, records) = timed_fixtures::native(
            &fixture,
            &options.binary,
            &output.join("native.jsonl"),
            runner,
        )?;
        let first = rows.first().ok_or("missing first timed result")?;
        let error = nullable(&field(first, "result")?, "error")?;
        let sleeps = field(first, "sleeps")?;
        let state = normalized(&fixture, &record_view(&records)?)?;
        let source_state = field(&source_result, "state")?;
        require(
            pyjson::sequence(&field(&source_state, "commands")?)?.is_empty()
                && pyjson::sequence(&field(&state, "commands")?)?.is_empty(),
            "unexpected date command",
        )?;
        let passed = is_null(&field(&source_result, "error")?) == is_null(&error)
            && pyjson::equal(&field(&source_result, "sleeps")?, &sleeps)
            && pyjson::equal(&source_state, &state);
        let native_result = object(&[("error", error), ("sleeps", sleeps), ("state", state)])?;
        let report = object(&[
            ("zone", JsonValue::text(zone)),
            ("unix_timestamp_millis", node(&json!(millis))?),
            ("passed", node(&json!(passed))?),
            ("results", array(&[source_result, native_result])?),
        ])?;
        write_json(&output.join("comparison.json"), &report)?;
        if !passed {
            failures.push(report.clone());
        }
        reports.push(report);
    }
    let report = object(&[
        ("passed", node(&json!(failures.is_empty()))?),
        ("scenarios", node(&json!(reports.len()))?),
        ("failures", array(&failures)?),
        ("results", array(&reports)?),
    ])?;
    write_json(&options.output.join("summary.json"), &report)?;
    let text = format!(
        "{}\n",
        pyjson::pretty(
            &object(&[
                ("passed", node(&json!(failures.is_empty()))?),
                ("scenarios", node(&json!(reports.len()))?),
                ("failed_cases", node(&json!(failures.len()))?)
            ])?,
            0
        )?
    );
    if !failures.is_empty() {
        return Err(Box::new(ComparisonFailure(text)));
    }
    Ok(text)
}
fn logging(options: &Options, runner: &mut dyn Runner) -> Result<String> {
    let mut reports = vec![];
    for malformed in [false, true] {
        let label = if malformed { "True" } else { "False" };
        let actions = json!([{"kind":"set_time","epoch":1790000000.0},{"kind":"close_logger"},{"kind":"set_time","epoch":1790000030.0},apply("Asia/Seoul","wifi")]);
        let mut setup = initial();
        if malformed {
            setup["params_hex"]["TimezoneSource"] = json!("ff");
        }
        let request = request(String::new(), actions, "UTC", false, setup);
        let directory = tempfile::tempdir()?;
        let mut fixture = Fixture::new(directory.path(), "UTC", &options.command)?;
        fixture.prepare(&request)?;
        let source = source_logging(options, &fixture, malformed)?;
        write_json(
            &options.output.join(format!("source-{label}.logs.json")),
            &field(&source, "records")?,
        )?;
        let directory = tempfile::tempdir()?;
        let mut fixture = Fixture::new(directory.path(), "UTC", &options.command)?;
        fixture.prepare(&request)?;
        let (rows, records) = timed_fixtures::native(
            &fixture,
            &options.binary,
            &options.output.join(format!("native-{label}.jsonl")),
            runner,
        )?;
        let results = rows
            .iter()
            .map(|row| {
                Ok(JsonValue::text(
                    if contains(&field(row, "result")?, "error") {
                        "error"
                    } else {
                        "ok"
                    },
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        require(
            pyjson::equal(
                &array(&results)?,
                &node(&json!(["ok", "ok", "error", "error"]))?,
            ),
            "closed logger error sequence differs",
        )?;
        require(
            records.len() == 1 && field(&records[0], "msg")?.text_eq("Time diff too small: 0.0s"),
            "closed logger records differ",
        )?;
        let mut params = vec![];
        for name in ["TimezoneName", "TimezoneSource"] {
            let path = fixture.params.join(name);
            params.push((
                name,
                if path.exists() {
                    JsonValue::text(
                        &fs::read(path)?
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>(),
                    )
                } else {
                    JsonValue::parse("null")?
                },
            ));
        }
        let raw = object(&params)?;
        if malformed {
            require(
                pyjson::equal(
                    &raw,
                    &node(&json!({"TimezoneName":null,"TimezoneSource":"ff"}))?,
                ),
                "malformed Params persistence differs",
            )?;
            fs::remove_file(fixture.params.join("TimezoneSource"))?;
        }
        let native_result = object(&[
            ("results", array(&results)?),
            ("state", normalized(&fixture, &[])?),
            ("raw_params", raw),
        ])?;
        let result = field(&source, "result")?;
        let source_result = object(&[
            ("results", field(&result, "results")?),
            ("state", source_state(&field(&result, "state")?)?),
            ("raw_params", field(&result, "raw_params")?),
        ])?;
        require(
            pyjson::equal(&source_result, &native_result),
            "closed logger side effects differ",
        )?;
        reports.push(array(&[source_result, native_result])?);
    }
    write_json(
        &options.output.join("summary.json"),
        &object(&[
            ("passed", node(&json!(true))?),
            ("scenarios", node(&json!(2))?),
            ("results", array(&reports)?),
        ])?,
    )?;
    Ok(
        "PASS: original/native closed-log propagation and command/Params side-effect boundaries\n"
            .into(),
    )
}
fn source_logging(options: &Options, fixture: &Fixture, malformed: bool) -> Result<JsonValue> {
    let mut spec = CommandSpec::new_os(
        vec![options.reference.as_os_str().to_owned()],
        Duration::from_secs(30),
    );
    fixture.configure(&mut spec);
    spec.input = Some(
        format!(
            "{}\n",
            json!({"config":fixture.config,"logging":true,"malformed":malformed})
        )
        .into_bytes(),
    );
    spec.stdout_limit = usize::MAX;
    spec.stderr_limit = usize::MAX;
    let result = NativeRunner.run(&spec)?;
    require(
        result.code == 0,
        &format!(
            "source logging failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ),
    )?;
    pyjson::parse(&result.stdout)
}
pub fn run(kind: Kind, options: &Options, runner: &mut dyn Runner) -> Result<String> {
    timed_reference::sources(&options.root)?;
    fs::create_dir_all(&options.output)?;
    match kind {
        Kind::Reference => reference(options, runner),
        Kind::Dates => dates(options, runner),
        Kind::Logging => logging(options, runner),
        Kind::Http => http::run(options, runner),
    }
}
pub fn cli(kind: Kind) -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let name = match kind {
        Kind::Reference => "reference",
        Kind::Dates => "dates",
        Kind::Http => "http",
        Kind::Logging => "logging",
    };
    let program = format!("check_timed_{name}.py");
    let usage = format!(
        "usage: {program} [-h] --binary BINARY --output OUTPUT{}\n",
        if matches!(kind, Kind::Http) {
            "\n                           [--worker WORKER WORKER]"
        } else {
            ""
        }
    );
    let mut specs = vec![
        OptionSpec::value("--binary", true),
        OptionSpec::value("--output", true),
    ];
    if matches!(kind, Kind::Http) {
        specs.push(OptionSpec::value("--worker", false));
    }
    let has_worker = matches!(kind, Kind::Http)
        && args.iter().any(|arg| {
            arg.to_str().is_some_and(|arg| {
                let name = arg.split('=').next().unwrap_or(arg);
                name.starts_with("--w") && "--worker".starts_with(name)
            })
        });
    let parsed =
        scan_os_with_positionals(&args, &specs, usize::from(has_worker)).map_err(|error| {
            let reason = if error.0.ends_with(" required") {
                let optional = specs
                    .iter()
                    .map(|spec| OptionSpec::value(spec.name, false))
                    .collect::<Vec<_>>();
                let values =
                    scan_os_with_positionals(&args, &optional, usize::from(has_worker)).ok();
                let missing = ["--binary", "--output"]
                    .into_iter()
                    .filter(|key| {
                        values
                            .as_ref()
                            .is_none_or(|parsed| !parsed.values.contains_key(*key))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("the following arguments are required: {missing}")
            } else if error.0 == "unrecognized arguments" {
                let extras = args
                    .iter()
                    .filter_map(|arg| arg.to_str())
                    .filter(|arg| {
                        arg.starts_with("--")
                            && !["--binary", "--output", "--worker"]
                                .iter()
                                .any(|key| key.starts_with(arg.split('=').next().unwrap_or(arg)))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("unrecognized arguments: {extras}")
            } else {
                error.0
            };
            ArgumentError(format!("{usage}{program}: error: {reason}"))
        })?;
    if parsed.help {
        print!("{usage}\noptions:\n  -h, --help{}show this help message and exit\n  --binary BINARY\n  --output OUTPUT\n{}",if matches!(kind,Kind::Http){"            "}else{"       "},if matches!(kind,Kind::Http){"  --worker WORKER WORKER\n"}else{""});
        return Ok(());
    }
    let helper = std::env::current_exe()?
        .parent()
        .ok_or("missing helper directory")?
        .to_owned();
    let value = |key: &str| -> Result<OsString> {
        parsed
            .values
            .get(key)
            .cloned()
            .ok_or_else(|| ArgumentError(format!("missing {key}")).into())
    };
    let options = Options {
        root: std::env::var_os("OPENPILOT_RUST_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")),
        binary: value("--binary")?.into(),
        output: value("--output")?.into(),
        reference: helper.join("timed-source-reference"),
        command: helper.join("timed-command-fixture"),
        worker: if has_worker {
            vec![
                value("--worker")?
                    .into_string()
                    .map_err(|_| "non-Unicode worker mode")?,
                parsed.positionals[0]
                    .clone()
                    .into_string()
                    .map_err(|_| "non-Unicode worker implementation")?,
            ]
        } else {
            vec![]
        },
    };
    match run(kind, &options, &mut NativeRunner) {
        Ok(text) => print!("{text}"),
        Err(error) => {
            if let Some(failure) = error.downcast_ref::<ComparisonFailure>() {
                print!("{}", failure.0);
            }
            return Err(error);
        }
    }
    Ok(())
}
