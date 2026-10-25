//! Original camera stress-policy scenarios, independently checked against captures.
use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::{json, Value};
#[derive(Serialize)]
pub struct Scenario {
    pub name: String,
    #[serde(serialize_with = "environment")]
    pub environment: Vec<(String, String)>,
    pub actions: String,
    pub expected: Vec<Value>,
    pub error: Option<String>,
}
fn environment<S: Serializer>(
    values: &[(String, String)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(values.len()))?;
    for (key, value) in values {
        map.serialize_entry(key, value)?;
    }
    map.end()
}
fn add(
    name: &str,
    probability: Option<&str>,
    interval: Option<&str>,
    expected: Vec<Value>,
) -> Scenario {
    let mut env = vec![("STRESS_CLOCK".into(), "1:0,1:1000,1:2000,1:3000".into())];
    for (key, value) in [
        ("SPECTRA_ERROR_PROB", probability),
        ("SPECTRA_ERROR_DT", interval),
        ("STRESS_RAND", Some("0")),
    ] {
        if let Some(value) = value {
            env.push((key.into(), value.into()));
        }
    }
    Scenario {
        name: name.into(),
        environment: env,
        actions: "stress 0 0\n".into(),
        expected,
        error: None,
    }
}
fn set(case: &mut Scenario, key: &str, value: String) {
    if let Some((_, v)) = case.environment.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        case.environment.push((key.into(), value));
    }
}
pub fn scenarios() -> Vec<Scenario> {
    let mut cases = vec![];
    let mut c = add("constructor-lazy", Some("bad"), Some("bad"), vec![]);
    c.actions.clear();
    cases.push(c);
    let mut c = add(
        "now-does-not-parse",
        Some("bad"),
        Some("bad"),
        vec![json!({"now":1000000000})],
    );
    c.actions = "now\n".into();
    cases.push(c);
    let mut c = add(
        "parse-after-now",
        Some("bad"),
        None,
        vec![json!({"now":1000000000})],
    );
    c.actions = "now\nstress 2 3\n".into();
    c.error = Some("invalid_argument".into());
    cases.push(c);
    cases.push(add("defaults", None, None, vec![json!(false)]));
    for (i, value) in ["0", "-0", "-1", "-inf", "nan", "NaN(payload)"]
        .iter()
        .enumerate()
    {
        cases.push(add(
            &format!("disabled-{i}"),
            Some(value),
            None,
            vec![json!(false)],
        ));
    }
    for (name, value, expected) in [
        ("one-excludes-rand-max", "1", false),
        ("unclamped-probability", "1.01", true),
        ("infinite-probability", "inf", true),
    ] {
        let mut c = add(name, Some(value), None, vec![json!(expected)]);
        set(&mut c, "STRESS_RAND", "2147483647".into());
        cases.push(c);
    }
    let mut c = add(
        "half-random-boundary",
        Some(".5"),
        Some("0"),
        vec![json!(true), json!(false)],
    );
    c.actions = "stress 0 0\nstress 1 1\n".into();
    set(&mut c, "STRESS_RAND", "1073741823,1073741824".into());
    cases.push(c);
    for (name, interval, expected, clock) in [
        (
            "strict-millisecond-interval",
            "1",
            vec![json!(false), json!(false), json!(true), json!(false)],
            "0:999999,0:1000000,0:1000001,0:2000000,0:3000000",
        ),
        (
            "shared-three-camera-last-trigger",
            "5",
            vec![json!(true), json!(false), json!(true), json!(true)],
            "0:6000000,0:7000000,0:12000000,0:12000001,0:13000000,0:19000000,0:20000000",
        ),
    ] {
        let mut c = add(name, Some("1"), Some(interval), expected);
        c.actions = "stress 0 0\nstress 1 1\nstress 2 2\nstress 0 3\n".into();
        set(&mut c, "STRESS_CLOCK", clock.into());
        set(&mut c, "STRESS_RAND", "0,0,0,0".into());
        cases.push(c);
    }
    for (i, (value, expected)) in [
        ("inf", false),
        ("nan", false),
        ("-inf", true),
        ("-1", true),
        ("-0", true),
    ]
    .iter()
    .enumerate()
    {
        cases.push(add(
            &format!("interval-{i}"),
            Some(value),
            Some(value),
            vec![json!(expected)],
        ));
        let c = cases.last_mut().unwrap();
        set(c, "SPECTRA_ERROR_PROB", "1".into());
    }
    cases.push(add(
        "prefixes",
        Some(" \t+0x1.8p-1tail"),
        Some(" +0.5suffix"),
        vec![json!(true)],
    ));
    let mut c = add(
        "cached-env",
        Some("1"),
        Some("0"),
        vec![json!(true), json!(true)],
    );
    c.actions = "stress 1 2\nstress 2 3\n".into();
    set(&mut c, "STRESS_RAND", "0,0".into());
    c.environment
        .insert(1, ("STRESS_MUTATE_PROB".into(), "invalid".into()));
    c.environment
        .insert(2, ("STRESS_MUTATE_DT".into(), "invalid".into()));
    cases.push(c);
    let mut c = add(
        "real-libc-rand-sequence",
        Some("0.5"),
        Some("0"),
        [
            false, true, false, false, false, true, true, false, true, false,
        ]
        .into_iter()
        .map(Value::Bool)
        .collect(),
    );
    c.actions = "stress 0 0\n".repeat(10);
    set(
        &mut c,
        "STRESS_CLOCK",
        (1..=20)
            .map(|i| format!("{i}:0"))
            .collect::<Vec<_>>()
            .join(","),
    );
    c.environment.retain(|(k, _)| k != "STRESS_RAND");
    cases.push(c);
    for variable in ["probability", "interval"] {
        for (i, (value, error)) in [
            ("", "invalid_argument"),
            ("x", "invalid_argument"),
            ("+", "invalid_argument"),
            ("1e9999", "out_of_range"),
            ("1e-9999", "out_of_range"),
        ]
        .iter()
        .enumerate()
        {
            let mut c = add(
                &format!("{variable}-error-{i}"),
                (variable == "probability").then_some(*value),
                (variable == "interval").then_some(*value),
                vec![],
            );
            c.error = Some((*error).into());
            cases.push(c);
        }
    }
    cases
}
pub const PARSER_VALUES: [&str; 20] = [
    "0",
    "-0",
    "  +1.5tail",
    "0x1.8p-2suffix",
    "1e+2x",
    "inf",
    "-Infinity",
    "nan",
    "-nan",
    "NaN(0x123)",
    "NaN(payload)",
    "",
    " ",
    "+",
    "x",
    "1e9999",
    "-1e9999",
    "1e-9999",
    "2.2250738585072014e-308",
    "4.9406564584124654e-324",
];
