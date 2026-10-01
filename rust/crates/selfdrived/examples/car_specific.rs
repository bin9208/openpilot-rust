use openpilot_logging::producer::Factory;
use openpilot_params::Params;
use openpilot_selfdrived::{
    callbacks::{Error as ParamsError, NativeParams},
    car_specific::{
        CarControl, CarInputs, CarParams, CarSpecificEvents, CarSpecificParams, CarState,
        CommonOptions,
    },
    events::Catalog,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    io::{self, BufRead, Cursor, Write},
    path::Path,
};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Init {
        cp: Vec<u8>,
    },
    Update {
        current: Vec<u8>,
        previous: Vec<u8>,
        control: Vec<u8>,
    },
    Common {
        current: Vec<u8>,
        previous: Vec<u8>,
        control: Vec<u8>,
        pcm_enable: bool,
        allow_enable: bool,
        allow_button_cancel: bool,
    },
    UpdateParams,
    Parameter {
        key: String,
        bytes: Option<Vec<u8>>,
        directory: bool,
    },
}

#[derive(Serialize)]
struct Effect {
    operation: &'static str,
    key: String,
    value: bool,
}

struct TraceParams<'a> {
    inner: NativeParams<'a>,
    effects: Vec<Effect>,
}

impl CarSpecificParams for TraceParams<'_> {
    type Error = ParamsError;

    fn get_bool(&mut self, key: &str) -> Result<bool, Self::Error> {
        let value = self.inner.get_bool(key)?;
        self.effects.push(Effect {
            operation: "get_bool",
            key: key.into(),
            value,
        });
        Ok(value)
    }

    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error> {
        self.inner.put_bool(key, value)?;
        self.effects.push(Effect {
            operation: "put_bool",
            key: key.into(),
            value,
        });
        Ok(())
    }
}

fn state(bytes: &[u8]) -> Result<CarState, Box<dyn std::error::Error>> {
    let message =
        capnp::serialize::read_message(Cursor::new(bytes), capnp::message::ReaderOptions::new())?;
    Ok(CarState::read(message.get_root()?)?)
}

fn inputs(wires: (&[u8], &[u8], &[u8])) -> Result<CarInputs, Box<dyn std::error::Error>> {
    let message =
        capnp::serialize::read_message(Cursor::new(wires.2), capnp::message::ReaderOptions::new())?;
    Ok(CarInputs {
        current: state(wires.0)?,
        previous: state(wires.1)?,
        control: CarControl::read(message.get_root()?)?,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [root, prefix, endpoint] = arguments.as_slice() else {
        return Err("expected Params root, prefix and logging endpoint".into());
    };
    let params = Params::open(Path::new(root), prefix)?;
    let mut logger = Factory::new(endpoint.clone())?.logger();
    let catalog = Catalog::load(false)?;
    let mut machine = None;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let mut trace = TraceParams {
            inner: NativeParams {
                params: &params,
                logger: &mut logger,
            },
            effects: Vec::new(),
        };
        let mut events = Vec::new();
        match request {
            Request::Init { cp } => {
                let message = capnp::serialize::read_message(
                    Cursor::new(cp),
                    capnp::message::ReaderOptions::new(),
                )?;
                machine = Some(CarSpecificEvents::new(CarParams::read(
                    message.get_root()?,
                )?));
            }
            Request::Update {
                current,
                previous,
                control,
            } => {
                let input = inputs((&current, &previous, &control))?;
                events = machine
                    .as_mut()
                    .ok_or("policy not initialized")?
                    .update(&input, &mut trace, &catalog)?;
            }
            Request::Common {
                current,
                previous,
                control,
                pcm_enable,
                allow_enable,
                allow_button_cancel,
            } => {
                let input = inputs((&current, &previous, &control))?;
                let options = CommonOptions {
                    extra_gears: &[],
                    pcm_enable,
                    allow_enable,
                    allow_button_cancel,
                };
                events = machine
                    .as_mut()
                    .ok_or("policy not initialized")?
                    .create_common_events(&input, &mut trace, (&catalog, options))?;
            }
            Request::UpdateParams => machine
                .as_mut()
                .ok_or("policy not initialized")?
                .update_params(&mut trace)?,
            Request::Parameter {
                key,
                bytes,
                directory,
            } => {
                openpilot_params::metadata(&key).ok_or("unknown fixture parameter")?;
                let path = Path::new(root).join(prefix).join(key);
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_dir() => fs::remove_dir(&path)?,
                    Ok(_) => fs::remove_file(&path)?,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                if directory {
                    fs::create_dir(path)?;
                } else if let Some(bytes) = bytes {
                    fs::write(path, bytes)?;
                }
            }
        }
        let result = json!({"events": events.into_iter().map(u16::from).collect::<Vec<_>>(), "state": machine, "effects": trace.effects});
        println!("{result}");
        io::stdout().flush()?;
    }
    Ok(())
}
