use openpilot_pandad::safety::{Effects, Error, Safety};
use openpilot_params::{Error as ParamError, Params};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Operation {
    onroad: bool,
    #[serde(default)]
    params: BTreeMap<String, Option<Vec<u8>>>,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Safety {
        params_root: PathBuf,
        pandas: usize,
        operations: Vec<Operation>,
    },
}

struct NativeEffects {
    params: Params,
    pandas: usize,
    commands: Vec<Value>,
    logs: Vec<Value>,
}

fn write_result(result: Result<(), ParamError>) -> Result<(), ParamError> {
    match result {
        Ok(()) | Err(ParamError::Io(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

impl Effects for NativeEffects {
    type Error = ParamError;
    fn panda_count(&self) -> usize {
        self.pandas
    }
    fn boolean(&mut self, key: &str) -> Result<bool, Self::Error> {
        Ok(self.bytes(key)? == b"1")
    }
    fn bytes(&mut self, key: &str) -> Result<Vec<u8>, Self::Error> {
        match self.params.get(key) {
            Ok(value) => Ok(value.unwrap_or_default()),
            Err(ParamError::Io(_)) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }
    fn put_bool(&mut self, key: &str, value: bool) -> Result<(), Self::Error> {
        write_result(self.params.put_bool(key, value))
    }
    fn set_safety(&mut self, panda: usize, model: u16, parameter: u16) -> Result<(), Self::Error> {
        self.commands
            .push(json!({"panda": panda, "request": 0xdc, "value": model, "index": parameter}));
        Ok(())
    }
    fn set_alternative(&mut self, panda: usize, experience: u16) -> Result<(), Self::Error> {
        self.commands
            .push(json!({"panda": panda, "request": 0xdf, "value": experience, "index": 0}));
        Ok(())
    }
    fn warning(&mut self, message: &str) -> Result<(), Self::Error> {
        self.logs.push(json!({"level": 30, "message": message}));
        Ok(())
    }
}

fn run(request: Request) -> Result<Value, Box<dyn std::error::Error>> {
    let Request::Safety {
        params_root,
        pandas,
        operations,
    } = request;
    let params = Params::open(&params_root, "panda-safety-fixture")?;
    for key in [
        "ObdMultiplexingEnabled",
        "ObdMultiplexingChanged",
        "FirmwareQueryDone",
        "ControlsReady",
        "CarParams",
    ] {
        write_result(params.remove(key))?;
    }
    let mut effects = NativeEffects {
        params,
        pandas,
        commands: Vec::new(),
        logs: Vec::new(),
    };
    let mut safety = Safety::default();
    let mut results = Vec::new();
    for operation in operations {
        for (key, value) in operation.params {
            write_result(match value {
                Some(value) => effects.params.put(&key, &value),
                None => effects.params.remove(&key),
            })?;
        }
        effects.commands.clear();
        effects.logs.clear();
        let failed = match safety.configure(operation.onroad, &mut effects) {
            Ok(()) => false,
            Err(Error::Cereal(_)) => true,
            Err(Error::Effect(error)) => return Err(error.into()),
        };
        let changed = effects.bytes("ObdMultiplexingChanged")?;
        results.push(json!({"failed": failed, "commands": effects.commands, "logs": effects.logs, "changed": changed, "state": safety}));
    }
    Ok(json!({"results": results}))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        serde_json::to_writer(&mut output, &run(serde_json::from_str(&line?)?)?)?;
        writeln!(output)?;
    }
    Ok(())
}
