use openpilot_xiaoge::{
    config::Config,
    settings::Settings,
    vision::{gate, Blindspot, GateInput, Side},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Config {
        value: Value,
    },
    Settings {
        value: Value,
    },
    Params {
        values: Vec<Option<Vec<u8>>>,
    },
    Gate {
        input: GateInput,
    },
    Smoothing {
        steps: Vec<Step>,
    },
    Nv12 {
        bytes: Vec<u8>,
        width: usize,
        height: usize,
        stride: usize,
        uv_offset: usize,
    },
}
#[derive(Deserialize)]
struct Step {
    side: Side,
    confidence: f64,
    threshold: f64,
    smoothing: f64,
    dt: f64,
}

fn evaluate(request: Request) -> Value {
    let invalid = |error: &openpilot_xiaoge::Error| match error {
        openpilot_xiaoge::Error::FloatOverflow | openpilot_xiaoge::Error::IntegerFloatOverflow => {
            json!({"exception": "OverflowError", "error": error.to_string()})
        }
        openpilot_xiaoge::Error::Invalid(_) => json!({"error": error.to_string()}),
    };
    match request {
        Request::Config { value } => match Config::normalize(&value) {
            Ok(value) => json!({"value": value}),
            Err(error) => invalid(&error),
        },
        Request::Settings { value } => match Settings::default().patch(&value) {
            Ok(settings) => match settings.parameters() {
                Ok(parameters) => json!({"value": settings, "parameters": parameters}),
                Err(error) => json!({"error": error.to_string()}),
            },
            Err(error) => invalid(&error),
        },
        Request::Params { values } => {
            let mut iter = values.into_iter();
            json!({"value": Settings::from_parameters(|_| iter.next().flatten())})
        }
        Request::Gate { input } => json!({"value": gate(input)}),
        Request::Smoothing { steps } => {
            let mut result = Blindspot::default();
            let rows: Vec<_> = steps
                .into_iter()
                .map(|step| {
                    result.update(
                        step.side,
                        step.confidence,
                        step.threshold,
                        step.smoothing,
                        step.dt,
                    );
                    json!([result.side(Side::Left), result.side(Side::Right)])
                })
                .collect();
            json!({"value": rows})
        }
        Request::Nv12 {
            bytes,
            width,
            height,
            stride,
            uv_offset,
        } => {
            let frame = openpilot_xiaoge::nv12::Frame::new(
                &bytes,
                openpilot_xiaoge::nv12::Layout {
                    width,
                    height,
                    stride,
                    uv_offset,
                },
            );
            match frame.and_then(|frame| Ok((frame.pack()?, frame.center_square()?))) {
                Ok((packed, square)) => json!({"packed": packed, "square": square}),
                Err(error) => json!({"error": error.to_string()}),
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let requests: Vec<Request> = serde_json::from_str(&input)?;
    serde_json::to_writer(
        std::io::stdout().lock(),
        &requests.into_iter().map(evaluate).collect::<Vec<_>>(),
    )?;
    std::io::stdout().flush()?;
    Ok(())
}
