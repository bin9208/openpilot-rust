use super::{
    decoder_settings,
    runtime_input::{self, Input},
    runtime_io::Capture,
};
use openpilot_can::Packet;
use openpilot_radarcan::{
    batch::Ego, databases::Databases, decoder::Config, numerics::Numerics, runtime::Engine, wire,
    Error,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Request {
    #[serde(flatten)]
    config: Config,
    #[serde(default)]
    params: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    replay: bool,
    #[serde(default)]
    flip: bool,
    ticks: Vec<Tick>,
}

#[derive(Deserialize)]
struct Tick {
    now_ns: u64,
    can: Vec<Input<Packet>>,
    #[serde(rename = "carState")]
    states: Vec<Input<Ego>>,
    #[serde(default)]
    processing_ns: u64,
}

pub fn trace(
    request: Value,
    databases: &mut Databases,
    numerics: &mut Numerics,
    stdout: &mut String,
) -> Result<Value, Error> {
    let request: Request = serde_json::from_value(request)?;
    let mut io = Capture {
        now: 1_000_000_000,
        processing: 0,
        flip: request.flip,
        stdout,
        databases,
        numerics,
        settings: decoder_settings::Observed {
            values: request.params,
            ..Default::default()
        },
        params: vec![json!({"key":"CarParams","block":true})],
        states: Vec::new(),
        warnings: Vec::new(),
        publications: Vec::new(),
        errors: Vec::new(),
    };
    let mut state = Engine::new(request.config, &mut io, request.replay)?;
    let mut diagnostics = Vec::new();
    let mut failure = Value::Null;
    for (index, tick) in request.ticks.into_iter().enumerate() {
        io.now = tick.now_ns;
        io.processing = tick.processing_ns;
        let operation = (|| -> Result<_, Error> {
            let packets = tick
                .can
                .into_iter()
                .map(runtime_input::packet)
                .collect::<Result<Vec<_>, _>>()?;
            state.batches.add_can(packets);
            for input in tick.states {
                state.add_state(runtime_input::ego(input)?);
            }
            state.process(&mut io)
        })();
        match operation {
            Ok(metrics) => {
                let work = (io.now as f64 * 1e-9 - tick.now_ns as f64 * 1e-9) * 1000.;
                diagnostics.push(json!({"work_ms":work,"thread_cpu_ms":0.,"decode_ms":0.,"radar_ms":work,
                    "input_age_ms":metrics.input_age_ms,"processed_batches":metrics.processed_batches,
                    "invalid":u8::from(metrics.invalid),"pending_states":metrics.pending_states,"pending_can":metrics.pending_can}));
            }
            Err(error) => {
                let kind = match error {
                    Error::Wire(wire::Error::ByteLength) => "ValueError",
                    _ => "NativeError",
                };
                failure = json!({"kind":kind,"message":error.to_string(),"tick":index});
                break;
            }
        }
    }
    let parser = io
        .states
        .last()
        .ok_or(Error::Contract("fixture constructor absent"))?["parser"]
        .clone();
    let pending_can = state
        .batches
        .can
        .iter()
        .map(|packet| {
            json!([
                packet.mono_time,
                packet
                    .frames
                    .iter()
                    .map(|frame| json!([frame.address, frame.data, frame.bus]))
                    .collect::<Vec<_>>()
            ])
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"publications":io.publications,"errors":io.errors,"warnings":io.warnings,"diagnostics":diagnostics,
        "constructor_count":io.states.len(),"constructor_states":io.states,"params_reads":io.params,
        "integer_reads":io.settings.reads,"scheduling":[[4,51]],"pending_can":pending_can,
        "pending_states":state.batches.states,"overflowed":state.batches.overflowed,"failure":failure,"parser":parser}),
    )
}
