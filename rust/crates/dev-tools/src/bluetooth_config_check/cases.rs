use crate::seal::Result;
use openpilot_log_format::JsonValue;

const MAC: &str = "66:C0:0C:7B:6E:71";
const SURROGATE: &str = "#LONE_SURROGATE#";
pub(super) const ACTIONS: &[&str] = &[
    "none",
    "accelCruise",
    "decelCruise",
    "gapAdjustCruise",
    "lfaButton",
    "cancel",
    "accelCruiseLong",
    "decelCruiseLong",
    "gapAdjustCruiseLong",
    "lfaButtonLong",
    "cancelLong",
    "laneLeft",
    "laneRight",
    "paddleDecel",
    "carrotCruise",
];
fn parse(text: &str) -> Result<JsonValue> {
    Ok(JsonValue::parse(text)?)
}
fn text(value: &str) -> Result<String> {
    Ok(JsonValue::text(value).to_json()?)
}
fn device(value: &str) -> Result<JsonValue> {
    parse(&format!("{{\"devices\": {{\"{MAC}\": {value}}}}}"))
}
fn profile(value: &str) -> Result<JsonValue> {
    device(&format!("{{\"profile\": {value}}}"))
}
fn mapping(items: &[(String, String)]) -> Result<JsonValue> {
    let pairs = items
        .iter()
        .map(|(key, action)| Ok(format!("{}: {action}", text(key)?)))
        .collect::<Result<Vec<_>>>()?;
    device(&format!(
        "{{\"profile\": \"generic\", \"mapping\": {{{}}}}}",
        pairs.join(", ")
    ))
}
pub fn cases() -> Result<Vec<JsonValue>> {
    let mut out = [
        "null",
        "false",
        "0",
        "\"\"",
        "[]",
        "{}",
        "{\"devices\": null}",
        "{\"devices\": []}",
        "{\"devices\": {}}",
        "{\"version\": -55, \"ignored\": {\"invalid\": true}}",
    ]
    .into_iter()
    .map(parse)
    .collect::<Result<Vec<_>>>()?;
    out.extend([
        device("null")?,
        device("false")?,
        device("{}")?,
        profile("\"generic\"")?,
        profile("\"yiser-j6\"")?,
    ]);
    for value in [
        "\"generic\"",
        "\"yiser-j6\"",
        "\"unknown\"",
        "null",
        "1",
        "[]",
        "{}",
    ] {
        out.push(profile(value)?);
    }
    for value in [
        "true", "false", "1", "0", "null", "\"\"", "[]", "{}", "\"true\"",
    ] {
        out.push(device(&format!(
            "{{\"profile\": \"generic\", \"enabled\": {value}}}"
        ))?);
    }
    let names = [
        "null".into(),
        "true".into(),
        "false".into(),
        "100".into(),
        "-0.0".into(),
        "Infinity".into(),
        "-Infinity".into(),
        "NaN".into(),
        format!("1{}", "0".repeat(200)),
        "1e-07".into(),
        "1e+20".into(),
        text(&"한😀".repeat(100))?,
        "\"\\ud800x\\udfff\"".into(),
        "[\"a\", null, true, 1e-07]".into(),
        "{\"z\": \"'한\", \"a\": false}".into(),
    ];
    for value in names {
        out.push(device(&format!(
            "{{\"profile\": \"generic\", \"name\": {value}}}"
        ))?);
    }
    for address in [
        "66:c0:0c:7b:6e:71",
        "ﬀ:aa:bb:cc:dd:ee",
        "AA:BB:CC:DD:EE:FF",
        "A:B:C:D:E:F",
        "66:C0:0C:7B:6E:71\n",
        " 66:C0:0C:7B:6E:71",
        SURROGATE,
        "01:02:03:04:05:0G",
    ] {
        let address = if address == SURROGATE {
            "\"\\ud800\"".into()
        } else {
            text(address)?
        };
        out.push(parse(&format!(
            "{{\"devices\": {{{address}: {{\"profile\": \"generic\"}}}}}"
        ))?);
    }
    out.push(parse(&format!("{{\"devices\": {{\"66:c0:0c:7b:6e:71\": {{\"profile\": \"generic\", \"name\": \"first\"}}, \"00:00:00:00:00:00\": {{\"profile\": \"generic\"}}, \"{MAC}\": {{\"profile\": \"yiser-j6\", \"name\": \"last\"}}}}}}"))?);
    for count in [16, 17] {
        let items = (0..count)
            .map(|i| format!("\"00:00:00:00:00:{i:02x}\": {{\"profile\": \"generic\"}}"))
            .collect::<Vec<_>>();
        out.push(parse(&format!(
            "{{\"devices\": {{{}}}}}",
            items.join(", ")
        ))?);
    }
    for value in [
        "null",
        "[]",
        "\"up\"",
        "true",
        "{}",
        "{\"up\": \"accelCruise\"}",
    ] {
        out.push(device(&format!(
            "{{\"profile\": \"generic\", \"mapping\": {value}}}"
        ))?);
    }
    let tokens = [
        "up",
        "down",
        "left",
        "right",
        "center",
        "1",
        "2",
        "key:0",
        "key:0001",
        "key:767",
        "key:768",
        "key:9999",
        "key:00000",
        "key:-1",
        "key:",
        "key:１",
        "swipe:x+",
        "swipe:x-",
        "swipe:y+",
        "swipe:y-",
        "swipe:z+",
        "tap:0:0",
        "tap:99999:00000",
        "tap:100000:0",
        "tap:-1:0",
        "tap:1:2:3",
        "up\n",
    ];
    for token in tokens
        .into_iter()
        .map(str::to_owned)
        .chain([SURROGATE.into()])
    {
        for suffix in ["", "@double", "@long", "@other", "@long@double"] {
            let key = if token == SURROGATE {
                format!("\"\\ud800{suffix}\"")
            } else {
                text(&format!("{token}{suffix}"))?
            };
            out.push(device(&format!(
                "{{\"profile\": \"generic\", \"mapping\": {{{key}: \"accelCruise\"}}}}"
            ))?);
        }
    }
    for action in ACTIONS
        .iter()
        .map(|s| text(s))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .chain([
            "null".into(),
            "false".into(),
            "1".into(),
            "[]".into(),
            "{}".into(),
            "\"shell\"".into(),
            "\"AccelCruise\"".into(),
        ])
    {
        for token in ["up", "key:999"] {
            out.push(device(&format!(
                "{{\"profile\": \"generic\", \"mapping\": {{\"{token}\": {action}}}}}"
            ))?);
        }
    }
    for count in [63, 64, 65] {
        out.push(mapping(
            &(0..count)
                .map(|key| (format!("key:{key}"), "\"none\"".into()))
                .collect::<Vec<_>>(),
        )?);
    }
    let full = (0..64)
        .flat_map(|key| {
            ["", "@long", "@double"]
                .map(move |suffix| (format!("key:{key}{suffix}"), "\"none\"".into()))
        })
        .collect::<Vec<_>>();
    out.push(mapping(&full)?);
    let mut overflow = full;
    overflow.push(("up".into(), "\"none\"".into()));
    out.push(mapping(&overflow)?);
    let mut bad: Vec<(String, String)> = (0..65)
        .map(|key| (format!("key:{key}"), "\"none\"".into()))
        .collect::<Vec<_>>();
    bad.push((SURROGATE.into(), "\"none\"".into()));
    // Preserve the lone surrogate key as an escape, not a replacement character.
    let pairs = bad
        .iter()
        .map(|(key, action)| {
            Ok(format!(
                "{}: {action}",
                if key == SURROGATE {
                    "\"\\ud800\"".into()
                } else {
                    text(key)?
                }
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    out.push(device(&format!(
        "{{\"profile\": \"generic\", \"mapping\": {{{}}}}}",
        pairs.join(", ")
    ))?);
    Ok(out)
}
