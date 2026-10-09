use openpilot_cluster::{autorun, rate, uevent, usb::packet, Error};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Run {
        request: autorun::RunRequest,
        board: bool,
        orientation: i64,
        debug: i64,
        onroad: bool,
    },
    Uevent {
        payload: Vec<u8>,
        product: u16,
    },
    Rate {
        requested: Option<i64>,
        h264: bool,
        target: f64,
        fallback: i64,
        bitrate: String,
    },
    Camera {
        mode: i64,
        speed: f64,
        wide: bool,
    },
    Command {
        id: u8,
        wall: f64,
        midnight: f64,
        fields: Vec<(usize, i64)>,
    },
    Frame {
        id: u8,
        wall: f64,
        midnight: f64,
        bytes: Vec<u8>,
        last: bool,
    },
}
fn observe(request: Request) -> Result<Value, Error> {
    Ok(match request {
        Request::Run {
            request,
            board,
            orientation,
            debug,
            onroad,
        } => json!({
            "args": request.args(),
            "sequence": request.configured_encoder.sequence(board).iter().map(|value| value.setting()).collect::<Vec<_>>(),
            "product": autorun::product_id(request.hud_mode),
            "orientation": autorun::orientation(orientation),
            "allowed": autorun::output_allowed(debug, onroad),
            "fixed_fps": autorun::fixed_fps(request.usbgpu_active),
        }),
        Request::Uevent { payload, product } => {
            json!({"decoded":uevent::decode(&payload),"matched":uevent::matches(&payload,product)})
        }
        Request::Rate {
            requested,
            h264,
            target,
            fallback,
            bitrate,
        } => json!({
            "display_fps":rate::display_fps(requested,h264,target,fallback)?,
            "encoder_fps":rate::encoder_fps(target,fallback)?,
            "bitrate":rate::bitrate(&bitrate,target,fallback)?,
        }),
        Request::Camera { mode, speed, wide } => {
            json!({"wide":rate::prefers_wide(mode,speed,wide),"zoom":rate::wide_zoom(speed)})
        }
        Request::Command {
            id,
            wall,
            midnight,
            fields,
        } => json!(packet::command(id, packet::milliseconds(wall, midnight)?, &fields)?.as_slice()),
        Request::Frame {
            id,
            wall,
            midnight,
            bytes,
            last,
        } => json!(packet::frame(
            id,
            packet::milliseconds(wall, midnight)?,
            &bytes,
            last
        )?),
    })
}
fn main() -> Result<(), Error> {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes)?;
    let requests: Vec<Request> = serde_json::from_slice(&bytes)?;
    let results = requests
        .into_iter()
        .map(observe)
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_writer(io::stdout().lock(), &results)?;
    Ok(())
}
