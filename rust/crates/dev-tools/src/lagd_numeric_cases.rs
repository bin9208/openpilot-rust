//! Dynamic translation of check_lagd_numeric.cases, including actual NumPy PRNG
//! draw order. Production generation never loads the captured request stream.
use crate::{
    lagd_frames, lagd_packet_reference::object, lagd_random::Generator, lagd_reference,
    seal::Result,
};
use openpilot_log_format::{JsonValue, JsonView};

pub struct Case {
    pub name: String,
    pub input: JsonValue,
    pub budget: &'static str,
}
const STAT: &str = "lag_and_block_statistics";
const CORR: &str = "correlation";
const POSE: &str = "pose_and_smoothing";
fn float(value: f64) -> Result<JsonValue> {
    Ok(JsonValue::parse(&format!("{value:?}"))?)
}
fn integer(value: impl std::fmt::Display) -> Result<JsonValue> {
    Ok(JsonValue::parse(&value.to_string())?)
}
fn floats(values: &[f64]) -> Result<JsonValue> {
    lagd_frames::array(
        &values
            .iter()
            .copied()
            .map(float)
            .collect::<Result<Vec<_>>>()?,
    )
}
fn flags(values: &[bool]) -> Result<JsonValue> {
    lagd_frames::array(
        &values
            .iter()
            .map(|v| integer(*v))
            .collect::<Result<Vec<_>>>()?,
    )
}
/// Python json.dumps default separators and ordered dictionaries; the shared
/// lossless scalar writer already preserves Python float/string formatting.
pub(crate) fn compact(value: &JsonValue) -> Result<String> {
    match value.view() {
        JsonView::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(compact)
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        JsonView::Object(values) => Ok(format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| Ok(format!(
                    "{}: {}",
                    JsonValue::codepoints(key.to_vec())
                        .ok_or("invalid JSON key")?
                        .to_json()?,
                    compact(value)?
                )))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
        _ => Ok(value.to_json()?),
    }
}
fn add(output: &mut Vec<Case>, name: impl Into<String>, input: JsonValue, budget: &'static str) {
    output.push(Case {
        name: name.into(),
        input,
        budget,
    });
}
fn comparison(
    kind: &str,
    expected: &[f64],
    actual: &[f64],
    mask: &[bool],
    n: usize,
) -> Result<JsonValue> {
    let mut fields = vec![("kind", JsonValue::text(kind))];
    if kind == "correlate" {
        fields.push(("n", integer(n)?));
    } else {
        fields.extend([
            ("dt", float(0.05)?),
            ("min", float(0.15)?),
            ("max", float(1.)?),
        ]);
    }
    fields.extend([
        ("expected", floats(expected)?),
        ("actual", floats(actual)?),
        ("mask", flags(mask)?),
    ]);
    object(&fields)
}
fn good_size(count: usize) -> Result<usize> {
    let request = object(&[
        ("kind", JsonValue::text("padding")),
        ("values", lagd_frames::array(&[integer(count)?])?),
    ])?;
    let values = lagd_reference::numeric(&request)?;
    let JsonView::Array(values) = values.view() else {
        return Err("padding source result is not a list".into());
    };
    Ok(values[0].to_json()?.parse()?)
}

pub fn cases() -> Result<Vec<Case>> {
    let mut output = Vec::with_capacity(95);
    let values = (1..150)
        .chain([499, 501, 999, 1200, 1220, 1225, 2049, 10001])
        .map(integer)
        .collect::<Result<Vec<_>>>()?;
    add(
        &mut output,
        "padding",
        object(&[
            ("kind", JsonValue::text("padding")),
            ("values", lagd_frames::array(&values)?),
        ])?,
        STAT,
    );
    for (values, index) in [
        ([3., 2., 1.], 0),
        ([1., 2., 3.], 2),
        ([0., 2., 0.], 1),
        ([0.1, 0.8, 0.5], 1),
        ([1., 1., 1.], 1),
    ] {
        let values = floats(&values)?;
        add(
            &mut output,
            format!("peak-{index}-{}", compact(&values)?),
            object(&[
                ("kind", JsonValue::text("peak")),
                ("values", values),
                ("index", integer(index)?),
            ])?,
            STAT,
        );
    }
    let mut random = Generator::new(139);
    for count in [40usize, 200, 500, 1200] {
        let times = (0..count).map(|i| i as f64 * 0.05).collect::<Vec<_>>();
        let signal = |shift: f64| {
            times
                .iter()
                .map(|time| {
                    let time = time - shift;
                    (0.8 * (time * 0.73).sin() + 0.2 * (time * 2.31).sin())
                        + 0.1 * (time * 4.3).cos()
                })
                .collect::<Vec<_>>()
        };
        let base = signal(0.);
        let actual = signal(0.31);
        // The original constructs the complete mask tuple before yielding any
        // case, so random(count) is consumed even before the all-mask rows.
        let masks = [
            ("all", vec![true; count]),
            ("holes", (0..count).map(|_| random.random() > 0.2).collect()),
            ("empty", vec![false; count]),
        ];
        for (name, mask) in masks {
            add(
                &mut output,
                format!("smooth-{count}-{name}"),
                object(&[
                    ("kind", JsonValue::text("smooth")),
                    ("values", floats(&base)?),
                    ("mask", flags(&mask)?),
                    ("k", integer(5)?),
                    ("sigma", float(1.)?),
                ])?,
                POSE,
            );
            let size = good_size(count + 20)?;
            add(
                &mut output,
                format!("ncc-{count}-{name}"),
                comparison("correlate", &base, &actual, &mask, size)?,
                CORR,
            );
            add(
                &mut output,
                format!("delay-{count}-{name}"),
                comparison("delay", &base, &actual, &mask, 0)?,
                STAT,
            );
        }
    }
    for kind in ["zero", "constant", "impulse", "alternating"] {
        let values = (0..1200)
            .map(|i| match kind {
                "zero" => 0.,
                "constant" => 0.1,
                "impulse" => {
                    if i == 500 {
                        1.
                    } else {
                        0.
                    }
                }
                _ => {
                    if i % 2 == 0 {
                        0.
                    } else {
                        1.
                    }
                }
            })
            .collect::<Vec<_>>();
        let actual = (0..values.len())
            .map(|i| values[(i + values.len() - 6) % values.len()])
            .collect::<Vec<_>>();
        let mask = vec![true; 1200];
        add(
            &mut output,
            format!("degenerate-ncc-{kind}"),
            comparison("correlate", &values, &actual, &mask, 1225)?,
            CORR,
        );
        add(
            &mut output,
            format!("degenerate-delay-{kind}"),
            comparison("delay", &values, &actual, &mask, 0)?,
            STAT,
        );
    }
    let mut constant_random = Generator::new(13901);
    for count in [200usize, 1200] {
        for amplitude in [0.1, 1.01] {
            let masks = [
                (
                    "holes",
                    (0..count)
                        .map(|_| constant_random.random() > 0.2)
                        .collect::<Vec<_>>(),
                ),
                ("periodic", (0..count).map(|i| i % 5 != 0).collect()),
                ("sparse", (0..count).map(|i| i % 20 == 0).collect()),
            ];
            for (name, mask) in masks {
                let values = vec![amplitude; count];
                let name = format!("masked-constant-{count}-{amplitude}-{name}");
                add(
                    &mut output,
                    format!("{name}-ncc"),
                    comparison("correlate", &values, &values, &mask, good_size(count + 20)?)?,
                    CORR,
                );
                add(
                    &mut output,
                    format!("{name}-delay"),
                    comparison("delay", &values, &values, &mask, 0)?,
                    STAT,
                );
            }
        }
    }
    for valid in [-1, 0, 1, 4, 5] {
        let updates = (0..40)
            .map(|_| random.uniform(0.15, 1.))
            .collect::<Vec<_>>();
        add(
            &mut output,
            format!("blocks-{valid}"),
            object(&[
                ("kind", JsonValue::text("blocks")),
                ("count", integer(5)?),
                ("size", integer(3)?),
                ("initial", float(0.4)?),
                ("valid", integer(valid)?),
                ("updates", floats(&updates)?),
            ])?,
            STAT,
        );
    }
    for index in 0..16 {
        let mut measurements = Vec::new();
        for name in [
            "orientation",
            "velocity",
            "acceleration",
            "angular_velocity",
        ] {
            let xyz = (0..3).map(|_| random.normal()).collect::<Vec<_>>();
            let std = (0..3).map(|_| random.uniform(0.01, 1.)).collect::<Vec<_>>();
            measurements.push((
                name,
                object(&[("xyz", floats(&xyz)?), ("std", floats(&std)?)])?,
            ));
        }
        let rpy = (0..3).map(|_| random.uniform(-2., 2.)).collect::<Vec<_>>();
        add(
            &mut output,
            format!("pose-{index}"),
            object(&[
                ("kind", JsonValue::text("pose")),
                ("pose", object(&measurements)?),
                ("rpy", floats(&rpy)?),
                ("valid", integer(index % 2 == 0)?),
            ])?,
            POSE,
        );
    }
    Ok(output)
}
pub fn request() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for case in cases()? {
        bytes.extend_from_slice(compact(&case.input)?.as_bytes());
        bytes.push(b'\n');
    }
    Ok(bytes)
}
