//! Ordered SwagFormatter records; callers supply resolved Rust text or structured payloads.
use crate::{Error, Fields, PythonText, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    NotSet = 0,
    Debug = 10,
    Info = 20,
    Warning = 30,
    Error = 40,
    Critical = 50,
}
impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Self::NotSet => "NOTSET",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Metadata {
    pub pathname: String,
    pub lineno: u32,
    pub module: String,
    pub function: String,
    pub host: String,
    pub process: u32,
    pub thread: u32,
    pub thread_name: String,
    pub created: f64,
}

pub fn event(name: &str, arguments: Vec<Value>, fields: Fields) -> Result<(Level, Value), Error> {
    if fields.contains_key("event") {
        return Err(Error::Contract("event supplied twice"));
    }
    let level = if fields.contains_key("error") {
        Level::Error
    } else if fields.contains_key("debug") {
        Level::Debug
    } else {
        Level::Info
    };
    let mut message = Fields::new();
    message.insert("event".into(), Value::Text(name.into()));
    if !arguments.is_empty() {
        message.insert("args".into(), Value::Array(arguments));
    }
    message.extend(
        fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    Ok((level, Value::Object(message)))
}

pub fn format_record(
    level: Level,
    message: Value,
    context: Fields,
    exception: Option<&str>,
    metadata: &Metadata,
) -> Result<Vec<u8>, Error> {
    let mut fields = Fields::new();
    fields.insert("msg".into(), message);
    fields.insert("ctx".into(), Value::Object(context));
    if let Some(exception) = exception {
        fields.insert("exc_info".into(), Value::Text(exception.into()));
    }
    fields.insert("level".into(), Value::Text(level.name().into()));
    fields.insert("levelnum".into(), Value::Integer(i128::from(level as u8)));
    fields.insert("name".into(), Value::Text("swaglog".into()));
    let filename = metadata.pathname.rsplit('/').next().unwrap_or("");
    fields.insert("filename".into(), Value::Text(filename.into()));
    fields.insert("lineno".into(), Value::Integer(i128::from(metadata.lineno)));
    fields.insert("pathname".into(), Value::Text(metadata.pathname.clone()));
    fields.insert("module".into(), Value::Text(metadata.module.clone()));
    fields.insert("funcName".into(), Value::Text(metadata.function.clone()));
    fields.insert("host".into(), Value::Text(metadata.host.clone()));
    fields.insert(
        "process".into(),
        Value::Integer(i128::from(metadata.process)),
    );
    fields.insert("thread".into(), Value::Integer(i128::from(metadata.thread)));
    fields.insert(
        "threadName".into(),
        Value::Text(metadata.thread_name.clone()),
    );
    fields.insert("created".into(), Value::Float(metadata.created));
    let json = fields.to_json()?;
    let mut packet = Vec::with_capacity(json.len() + 1);
    packet.push(level as u8);
    packet.extend_from_slice(json.as_bytes());
    Ok(packet)
}

pub struct Record {
    pub level: Level,
    pub message: Value,
    pub exception: Option<String>,
    pub(crate) console: String,
}
impl Record {
    pub fn text(level: Level, text: String) -> Self {
        Self {
            level,
            message: Value::Text(text.clone()),
            exception: None,
            console: text,
        }
    }
    pub fn python_text(level: Level, text: PythonText) -> Self {
        let console = text.console();
        Self {
            level,
            message: Value::PythonText(text),
            exception: None,
            console,
        }
    }
    pub fn event(name: &str, arguments: Vec<Value>, fields: Fields) -> Result<Self, Error> {
        let (level, message) = event(name, arguments, fields)?;
        let Value::Object(object) = &message else {
            return Err(Error::Contract("event must be an object"));
        };
        let console = object.to_json()?;
        Ok(Self {
            level,
            message,
            exception: None,
            console,
        })
    }
    pub fn with_exception(mut self, details: String) -> Self {
        self.exception = Some(details);
        self
    }
}
