use openpilot_log_format::{JsonValue, JsonView};
use openpilot_pandad::safety::{Effects, Error, Safety};
use openpilot_params::{Error as ParamError, Params};
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, BufRead, Write},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
};

#[derive(Deserialize)]
struct Operation {
    onroad: bool,
    #[serde(default)]
    params: BTreeMap<String, Option<Vec<u8>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RequestOperation {
    Safety,
}

#[derive(Deserialize)]
struct Request {
    #[serde(rename = "op")]
    _operation: RequestOperation,
    params_root: Box<RawValue>,
    pandas: usize,
    operations: Vec<Operation>,
}

fn filesystem_root(raw: &RawValue) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let value = JsonValue::parse(raw.get())?;
    let JsonView::Text(points) = value.view() else {
        return Err("params_root must be a JSON string".into());
    };
    let mut bytes = Vec::new();
    for &point in points {
        if (0xdc80..=0xdcff).contains(&point) {
            bytes.push((point - 0xdc00) as u8);
        } else {
            let character = char::from_u32(point).ok_or("invalid params_root codepoint")?;
            let mut utf8 = [0; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut utf8).as_bytes());
        }
    }
    Ok(PathBuf::from(OsString::from_vec(bytes)))
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
    let Request {
        _operation: _,
        params_root,
        pandas,
        operations,
    } = request;
    let params_root = filesystem_root(&params_root)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    #[test]
    fn raw_params_root_is_used_without_unicode_replacement() {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().join("params-");
        let mut encoded = serde_json::to_string(base.to_str().unwrap()).unwrap();
        encoded.pop();
        encoded.push_str("\\udcff\"");
        let request = format!(
            "{{\"op\":\"safety\",\"params_root\":{encoded},\"pandas\":0,\"operations\":[]}}"
        );
        let mut path = base.into_os_string();
        path.push(OsStr::from_bytes(&[0xff]));
        let path = PathBuf::from(path);
        let result = run(serde_json::from_str(&request).unwrap()).unwrap();
        assert_eq!(result, json!({"results": []}));
        assert!(path.is_dir(), "Params must use the original raw root bytes");
    }

    #[test]
    fn params_root_rejects_non_text_and_unencodable_surrogates() {
        for source in ["1", r#""\ud800""#, r#""\udc7f""#] {
            let raw: Box<RawValue> = serde_json::from_str(source).unwrap();
            assert!(filesystem_root(&raw).is_err());
        }
        let unicode: Box<RawValue> = serde_json::from_str(r#""한글-🦀""#).unwrap();
        assert_eq!(filesystem_root(&unicode).unwrap(), PathBuf::from("한글-🦀"));
        let normal: Request = serde_json::from_str(
            r#"{"op":"safety","params_root":"/tmp","pandas":0,"operations":[]}"#,
        )
        .unwrap();
        assert_eq!(
            filesystem_root(&normal.params_root).unwrap(),
            PathBuf::from("/tmp")
        );
        assert!(serde_json::from_str::<Request>(
            r#"{"op":"other","params_root":"/tmp","pandas":0,"operations":[]}"#
        )
        .is_err());
    }
}
