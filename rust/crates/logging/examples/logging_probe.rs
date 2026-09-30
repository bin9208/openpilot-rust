use openpilot_logging::{
    context::ContextGuard,
    log_site,
    producer::{Delivery, Factory, Logger},
    record::{self, Level, Metadata, Record},
    Error, Fields, Value,
};
use serde::Deserialize;
use serde_json::json;
use std::io;

type Pairs = Vec<(String, Value)>;
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Command {
    Format {
        level: u8,
        message: Value,
        context: Pairs,
        exception: Option<String>,
    },
    Event {
        name: String,
        arguments: Vec<Value>,
        fields: Pairs,
        context: Pairs,
        special: bool,
    },
    Bind {
        fields: Pairs,
    },
    Global {
        fields: Pairs,
    },
    Push {
        fields: Pairs,
    },
    Pop,
    Snapshot,
    Emit {
        level: u8,
        text: String,
        exception: Option<String>,
    },
    EmitEvent {
        name: String,
        arguments: Vec<Value>,
        fields: Pairs,
        special: bool,
    },
    Thread {
        name: String,
        fields: Pairs,
    },
    PanicScope,
    Close,
    Flood {
        count: u32,
    },
}
fn level(value: u8) -> Result<Level, Error> {
    Ok(match value {
        0 => Level::NotSet,
        10 => Level::Debug,
        20 => Level::Info,
        30 => Level::Warning,
        40 => Level::Error,
        50 => Level::Critical,
        _ => return Err(Error::Contract("unknown level")),
    })
}
fn fields(values: Pairs, special: bool) -> Fields {
    let mut fields: Fields = values.into_iter().collect();
    if special {
        fields.insert("nan".into(), Value::Float(f64::NAN));
        fields.insert("positive".into(), Value::Float(f64::INFINITY));
        fields.insert("negative".into(), Value::Float(f64::NEG_INFINITY));
        fields.insert("wide_integer".into(), Value::Integer(i128::MAX));
    }
    fields
}
fn metadata() -> Metadata {
    Metadata {
        pathname: "/source/module.rs".into(),
        lineno: 10,
        module: "module".into(),
        function: "run".into(),
        host: "test-host".into(),
        process: 123,
        thread: 456,
        thread_name: "worker".into(),
        created: 1234.5,
    }
}
fn result(delivery: Delivery) -> serde_json::Value {
    json!({"delivery":match delivery{Delivery::Sent=>"sent",Delivery::Dropped=>"dropped",Delivery::Filtered=>"filtered"}})
}
fn worker(factory: Factory, name: String, values: Pairs) -> Result<serde_json::Value, Error> {
    std::thread::Builder::new()
        .name(name)
        .spawn(move || {
            let mut logger = factory.logger();
            logger.bind(values.into_iter().collect());
            let delivery = logger.emit(
                log_site!(),
                Record::text(Level::Info, "worker-record".into()),
            )?;
            let mut acknowledgement = String::new();
            io::stdin().read_line(&mut acknowledgement)?;
            Ok::<_, Error>(delivery)
        })?
        .join()
        .map_err(|_| Error::Contract("worker panicked"))?
        .map(result)
}
fn execute(
    command: Command,
    logger: &mut Logger,
    factory: &Factory,
    scopes: &mut Vec<ContextGuard>,
) -> Result<serde_json::Value, Error> {
    match command {
        Command::Format {
            level: value,
            message,
            context,
            exception,
        } => Ok(
            json!({"packet":record::format_record(level(value)?,message,context.into_iter().collect(),exception.as_deref(),&metadata())?}),
        ),
        Command::Event {
            name,
            arguments,
            fields: values,
            context,
            special,
        } => {
            let (severity, message) = record::event(&name, arguments, fields(values, special))?;
            Ok(
                json!({"packet":record::format_record(severity,message,context.into_iter().collect(),None,&metadata())?}),
            )
        }
        Command::Bind { fields: values } => {
            logger.bind(values.into_iter().collect());
            Ok(json!({"ok":true}))
        }
        Command::Global { fields: values } => {
            factory.bind_global(values.into_iter().collect())?;
            Ok(json!({"ok":true}))
        }
        Command::Push { fields: values } => {
            scopes.push(logger.context(values.into_iter().collect()));
            Ok(json!({"ok":true}))
        }
        Command::Pop => {
            drop(scopes.pop().ok_or(Error::Contract("scope stack empty"))?);
            Ok(json!({"ok":true}))
        }
        Command::Snapshot => Ok(json!({"context":logger.context_snapshot()?.to_json()?})),
        Command::Emit {
            level: value,
            text,
            exception,
        } => {
            let mut record = Record::text(level(value)?, text);
            record.exception = exception;
            Ok(result(logger.emit(log_site!(), record)?))
        }
        Command::EmitEvent {
            name,
            arguments,
            fields: values,
            special,
        } => Ok(result(logger.emit(
            log_site!(),
            Record::event(&name, arguments, fields(values, special))?,
        )?)),
        Command::Thread { name, fields } => worker(factory.clone(), name, fields),
        Command::Close => {
            logger.close();
            Ok(json!({"ok":true}))
        }
        Command::PanicScope => {
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _scope =
                    logger.context([("unwind".into(), Value::Bool(true))].into_iter().collect());
                panic!("intentional context unwind");
            }))
            .is_err();
            Ok(json!({"panicked":panicked,"context":logger.context_snapshot()?.to_json()?}))
        }
        Command::Flood { count } => {
            let (mut sent, mut dropped) = (0, 0);
            let started = std::time::Instant::now();
            for _ in 0..count {
                match logger.emit(
                    log_site!(),
                    Record::text(Level::Debug, "backpressure".into()),
                )? {
                    Delivery::Sent => sent += 1,
                    Delivery::Dropped => dropped += 1,
                    Delivery::Filtered => {
                        return Err(Error::Contract("debug unexpectedly filtered"))
                    }
                }
            }
            Ok(json!({"sent":sent,"dropped":dropped,"seconds":started.elapsed().as_secs_f64()}))
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let factory = match std::env::args().nth(1) {
        Some(endpoint) => Factory::new(endpoint)?,
        None => Factory::for_runtime()?,
    };
    let mut logger = factory.logger();
    let mut scopes = Vec::new();
    loop {
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let command: Command = serde_json::from_str(&line)?;
        let response = match execute(command, &mut logger, &factory, &mut scopes) {
            Ok(value) => value,
            Err(error) => json!({"error":error.to_string()}),
        };
        println!("{response}");
    }
    Ok(())
}
