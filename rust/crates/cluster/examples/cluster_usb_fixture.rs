use openpilot_cluster::{
    usb::{Display, H264Chunk, Options, Runtime},
    Error,
};
use std::{
    cell::RefCell,
    io::{self, Read},
    rc::Rc,
    time::Duration,
};

#[derive(serde::Deserialize)]
struct Case {
    #[serde(default)]
    options: Options,
    #[serde(default)]
    gpu_active: bool,
    #[serde(default)]
    reset_protected: bool,
    actions: Vec<Action>,
    lock: std::path::PathBuf,
}

#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Action {
    Open,
    Close,
    Jpeg {
        bytes: Vec<u8>,
    },
    Png {
        bytes: Vec<u8>,
    },
    Brightness {
        value: i32,
        force: bool,
    },
    Orientation {
        value: i32,
        force: bool,
    },
    Fps {
        value: i32,
        force: bool,
    },
    StartH264 {
        requested: i64,
    },
    Chunk {
        bytes: Vec<u8>,
        last: bool,
        wait: bool,
        require: bool,
    },
    StopH264,
}

#[derive(Default, serde::Serialize)]
struct Trace {
    gpu_active: bool,
    reset_protected: bool,
    sleeps_ms: Vec<u128>,
    connected: Vec<bool>,
}
struct TraceRuntime(Rc<RefCell<Trace>>);
impl Runtime for TraceRuntime {
    fn milliseconds(&self) -> Result<u32, Error> {
        Ok(43_210)
    }
    fn sleep(&mut self, duration: Duration) {
        self.0.borrow_mut().sleeps_ms.push(duration.as_millis());
    }
    fn gpu_active(&self) -> bool {
        self.0.borrow().gpu_active
    }
    fn reset_protected(&self) -> bool {
        self.0.borrow().reset_protected
    }
    fn connected(&mut self, value: bool) {
        self.0.borrow_mut().connected.push(value);
    }
    fn warning(&mut self, message: &str) {
        eprintln!("{message}");
    }
}

#[derive(serde::Serialize)]
struct Observation<'a> {
    results: Vec<Result<serde_json::Value, String>>,
    dimensions: [u32; 2],
    runtime: &'a Trace,
}

fn action(display: &mut Display<TraceRuntime>, action: Action) -> Result<serde_json::Value, Error> {
    use serde_json::{json, Value};
    match action {
        Action::Open => {
            display.open()?;
            Ok(Value::Null)
        }
        Action::Close => {
            display.close();
            Ok(Value::Null)
        }
        Action::Jpeg { bytes } => {
            display.send_jpeg(&bytes)?;
            Ok(Value::Null)
        }
        Action::Png { bytes } => {
            display.send_png(&bytes)?;
            Ok(Value::Null)
        }
        Action::Brightness { value, force } => Ok(json!(display.set_brightness(value, force))),
        Action::Orientation { value, force } => Ok(json!(display.set_orientation(value, force))),
        Action::Fps { value, force } => Ok(json!(display.set_display_fps(value, force))),
        Action::StartH264 { requested } => Ok(json!(display.start_h264_stream(requested)?)),
        Action::Chunk {
            bytes,
            last,
            wait,
            require,
        } => {
            display.send_h264_chunk(H264Chunk {
                bytes: &bytes,
                last,
                wait_for_ack: wait,
                require_ack: require,
            })?;
            Ok(Value::Null)
        }
        Action::StopH264 => {
            display.stop_h264_stream();
            Ok(Value::Null)
        }
    }
}
fn main() -> Result<(), Error> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let case: Case = serde_json::from_str(&input)?;
    let trace = Rc::new(RefCell::new(Trace {
        gpu_active: case.gpu_active,
        reset_protected: case.reset_protected,
        ..Trace::default()
    }));
    let runtime = TraceRuntime(Rc::clone(&trace));
    let mut display = Display::new(case.options, runtime, &case.lock)?;
    let results = case
        .actions
        .into_iter()
        .map(|a| action(&mut display, a).map_err(|e| e.to_string()))
        .collect();
    let dimensions = display.dimensions();
    display.close();
    let observation = Observation {
        results,
        dimensions,
        runtime: &trace.borrow(),
    };
    println!("{}", serde_json::to_string(&observation)?);
    Ok(())
}
