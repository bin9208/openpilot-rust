//! Source-shaped scenario generation. No captured request/result data is read.
use super::Scenario;
use crate::{
    manager_catalog_reference::ops as v,
    manager_catalog_reference::{catalog_names, PREDICATES},
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};
use std::{collections::BTreeMap, ffi::OsString, os::unix::ffi::OsStringExt};
const BOOL_KEYS: [&str; 9] = [
    "IsDriverViewEnabled",
    "DisableLogging",
    "UbloxAvailable",
    "JoystickDebugMode",
    "LongitudinalManeuverMode",
    "LateralManeuverMode",
    "SoftwareMenu",
    "ShareData",
    "HardwareC3xLite",
];
const INT_KEYS: [&str; 4] = [
    "DisableDM",
    "ClusterHud",
    "CarrotYouTubeLive",
    "CarrotYouTubeQuality",
];
fn obj(fields: &[(&str, JsonValue)]) -> Result<JsonValue> {
    v::object_dynamic(
        &fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<Vec<_>>(),
    )
}
fn boolean(b: bool) -> JsonValue {
    v::number(if b { "true" } else { "false" })
}
fn bytes(raw: &[u8]) -> Result<JsonValue> {
    v::array(&raw.iter().map(v::number).collect::<Vec<_>>())
}
fn encoded(fields: &[(&str, &[u8])]) -> Result<JsonValue> {
    obj(&fields
        .iter()
        .map(|(k, v)| Ok((*k, bytes(v)?)))
        .collect::<Result<Vec<_>>>()?)
}
fn number_values(fields: &[(&str, i32)]) -> Result<JsonValue> {
    encoded(
        &fields
            .iter()
            .map(|(k, v)| (*k, v.to_string().into_bytes()))
            .collect::<Vec<_>>()
            .iter()
            .map(|(k, v)| (*k, v.as_slice()))
            .collect::<Vec<_>>(),
    )
}
fn case(
    name: &str,
    values: JsonValue,
    started: bool,
    not_car: bool,
    extra: &[(&str, JsonValue)],
) -> Result<JsonValue> {
    let mut fields = vec![
        ("predicate", JsonValue::text(name)),
        ("started", boolean(started)),
        ("not_car", boolean(not_car)),
        ("values", values),
    ];
    fields.extend_from_slice(extra);
    obj(&fields)
}
fn affected(key: &str) -> &str {
    match key {
        "DisableDM" => "enable_dm",
        "ClusterHud" => "enable_cluster_hud",
        _ => "enable_youtube_encoder",
    }
}
fn add(
    rows: &mut Vec<Scenario>,
    name: String,
    request: JsonValue,
    environment: BTreeMap<String, Option<OsString>>,
    fatal: bool,
) -> Result<()> {
    rows.push(Scenario {
        name,
        request: format!(
            "{}\n",
            crate::control_tools_policy_check::json::pretty(&request, 0)?
        )
        .into_bytes(),
        environment,
        fatal,
    });
    Ok(())
}
fn regular(rows: &mut Vec<Scenario>, name: &str, cases: &[JsonValue]) -> Result<()> {
    add(
        rows,
        name.into(),
        obj(&[("cases", v::array(cases)?)])?,
        BTreeMap::new(),
        false,
    )
}
pub fn generate() -> Result<Vec<Scenario>> {
    let mut rows = Vec::new();
    for index in 0..64 {
        let keys = [
            "pc",
            "tici",
            "webcam",
            "carrot_web_external",
            "darwin",
            "bodyteleop_available",
        ];
        let config = obj(&keys
            .iter()
            .enumerate()
            .map(|(i, k)| (*k, boolean(index & (1 << (5 - i)) != 0)))
            .collect::<Vec<_>>())?;
        add(
            &mut rows,
            format!("catalog-{index:02}"),
            obj(&[("config", config)])?,
            BTreeMap::new(),
            false,
        )?;
    }
    let env_values = [
        None,
        Some(Vec::new()),
        Some(b"0".to_vec()),
        Some(b"1".to_vec()),
        Some(vec![255]),
    ];
    for (i, webcam) in env_values.iter().enumerate() {
        for (j, external) in env_values.iter().enumerate() {
            let env = [
                ("USE_WEBCAM".into(), webcam.clone().map(OsString::from_vec)),
                (
                    "CARROT_WEB_EXTERNAL".into(),
                    external.clone().map(OsString::from_vec),
                ),
            ]
            .into_iter()
            .collect();
            add(
                &mut rows,
                format!("environment-{:02}", i * 5 + j),
                obj(&[("bodyteleop_available", boolean(true))])?,
                env,
                false,
            )?;
        }
    }
    add(
        &mut rows,
        "body-module-missing".into(),
        obj(&[
            ("body_discovery_missing_parent", boolean(true)),
            ("bodyteleop_available", boolean(false)),
        ])?,
        BTreeMap::new(),
        false,
    )?;
    let empty = obj(&[])?;
    let mut profiles = vec![
        empty.clone(),
        number_values(&BOOL_KEYS.map(|k| (k, 1)))?,
        number_values(&BOOL_KEYS.map(|k| (k, 0)))?,
    ];
    for dm in 0..=2 {
        for hud in 0..=1 {
            for quality in 0..=4 {
                profiles.push(number_values(&[
                    ("DisableDM", dm),
                    ("ClusterHud", hud),
                    ("CarrotYouTubeLive", 1),
                    ("CarrotYouTubeQuality", quality),
                ])?);
            }
        }
    }
    let mut cases = Vec::new();
    for profile in profiles {
        for started in [false, true] {
            for not_car in [false, true] {
                for name in catalog_names() {
                    cases.push(case(
                        &format!("process:{name}"),
                        profile.clone(),
                        started,
                        not_car,
                        &[],
                    )?);
                }
            }
        }
    }
    regular(&mut rows, "full-catalog-policy", &cases)?;
    cases.clear();
    for name in PREDICATES {
        for started in [false, true] {
            for not_car in [false, true] {
                cases.push(case(name, empty.clone(), started, not_car, &[])?);
            }
        }
    }
    for key in BOOL_KEYS {
        for value in [b"".as_slice(), b"0", b"1", b"true", b"1\0", b"\xff"] {
            for name in PREDICATES {
                cases.push(case(name, encoded(&[(key, value)])?, true, false, &[])?);
            }
        }
    }
    for dm in -1..=3 {
        for driver in [0, 1] {
            for started in [false, true] {
                cases.push(case(
                    "enable_dm",
                    number_values(&[("DisableDM", dm), ("IsDriverViewEnabled", driver)])?,
                    started,
                    false,
                    &[],
                )?);
            }
        }
    }
    for name in &PREDICATES[19..23] {
        for live in -1..=2 {
            for quality in -1..=4 {
                cases.push(case(
                    name,
                    number_values(&[
                        ("CarrotYouTubeLive", live),
                        ("CarrotYouTubeQuality", quality),
                    ])?,
                    true,
                    false,
                    &[],
                )?);
            }
        }
    }
    for wide in [b"".as_slice(), b"0", b"1", b"true", b"\xff", b"1\0"] {
        cases.push(case(
            "enable_youtube_wide_encoder",
            encoded(&[
                ("UseWideCamera", wide),
                ("CarrotYouTubeLive", b"1"),
                ("CarrotYouTubeQuality", b"3"),
            ])?,
            true,
            false,
            &[],
        )?);
    }
    for name in ["ublox", "qcomgps"] {
        for started in [false, true] {
            for stored in [0, 1] {
                for tty in [false, true] {
                    for quectel in [false, true] {
                        let paths = [
                            ("/dev/ttyHS0", tty),
                            ("/persist/comma/use-quectel-gps", quectel),
                        ]
                        .into_iter()
                        .filter(|(_, v)| *v)
                        .map(|(k, _)| JsonValue::text(k))
                        .collect::<Vec<_>>();
                        cases.push(case(
                            name,
                            number_values(&[("UbloxAvailable", stored)])?,
                            started,
                            false,
                            &[("gps_paths", v::array(&paths)?)],
                        )?);
                    }
                }
            }
        }
    }
    for key in INT_KEYS {
        for value in [
            b"".as_slice(),
            b"  +2tail",
            b"-1suffix",
            b"2147483647",
            b"-2147483648",
            b"0\0garbage",
            b"1.9",
            b"\x0b2",
        ] {
            let fields = if key == "CarrotYouTubeLive" {
                vec![("CarrotYouTubeLive", value)]
            } else {
                vec![("CarrotYouTubeLive", b"1".as_slice()), (key, value)]
            };
            cases.push(case(affected(key), encoded(&fields)?, true, false, &[])?);
        }
    }
    for key in BOOL_KEYS
        .into_iter()
        .chain(INT_KEYS)
        .chain(["UseWideCamera"])
    {
        for name in PREDICATES {
            for extra in [
                obj(&[("directories", v::array(&[JsonValue::text(key)])?)])?,
                obj(&[("exception_key", JsonValue::text(key))])?,
            ] {
                let JsonView::Object(extras) = extra.view() else {
                    unreachable!()
                };
                let extras = extras
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.iter()
                                .map(|&p| char::from_u32(p).unwrap())
                                .collect::<String>(),
                            value.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                cases.push(case(
                    name,
                    number_values(&[("CarrotYouTubeLive", 1), ("CarrotYouTubeQuality", 3)])?,
                    true,
                    false,
                    &extras
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.clone()))
                        .collect::<Vec<_>>(),
                )?);
            }
        }
    }
    for name in [
        "enable_dm",
        "process:carrot_vision_encoderd",
        "process:youtube_encoderd",
        "process:youtube_wide_encoderd",
    ] {
        cases.push(case(
            name,
            encoded(&INT_KEYS.map(|k| (k, b"invalid".as_slice())))?,
            false,
            false,
            &[],
        )?);
    }
    cases.push(case(
        "enable_youtube_wide_encoder",
        encoded(&[("UseWideCamera", b"0"), ("CarrotYouTubeLive", b"invalid")])?,
        true,
        false,
        &[],
    )?);
    cases.push(case(
        "enable_youtube_encoder",
        encoded(&[
            ("CarrotYouTubeLive", b"0"),
            ("CarrotYouTubeQuality", b"invalid"),
        ])?,
        true,
        false,
        &[],
    )?);
    cases.push(case(
        "enable_webrtc",
        encoded(&[("DisableDM", b"0"), ("ClusterHud", b"invalid")])?,
        true,
        false,
        &[],
    )?);
    regular(&mut rows, "parameter-and-order-policy", &cases)?;
    for (ki, key) in INT_KEYS.iter().enumerate() {
        for (vi, value) in [
            b"invalid".as_slice(),
            b"2147483648",
            b"-2147483649",
            b" ",
            b"\xff",
            b"\0",
        ]
        .iter()
        .enumerate()
        {
            let fields = if *key == "CarrotYouTubeLive" {
                vec![("CarrotYouTubeLive", *value)]
            } else {
                vec![("CarrotYouTubeLive", b"1".as_slice()), (*key, *value)]
            };
            let request = obj(&[(
                "cases",
                v::array(&[case(affected(key), encoded(&fields)?, true, false, &[])?])?,
            )])?;
            add(
                &mut rows,
                format!("fatal-{:02}", ki * 6 + vi),
                request,
                BTreeMap::new(),
                true,
            )?;
        }
    }
    Ok(rows)
}
