use openpilot_logging::{
    log_site, native::Logger, producer::Delivery, rate::RateLimit, record::Level,
};
use serde::Deserialize;
use std::io::{self, BufRead};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Emit { level: u8, text: String },
    Rate { timestamp: u64, text: String },
    Flood { count: usize },
    Threads { count: usize },
    Close,
}
fn level(value: u8) -> Result<Level, &'static str> {
    match value {
        0 => Ok(Level::NotSet),
        10 => Ok(Level::Debug),
        20 => Ok(Level::Info),
        30 => Ok(Level::Warning),
        40 => Ok(Level::Error),
        50 => Ok(Level::Critical),
        _ => Err("unsupported level"),
    }
}
fn emit(
    logger: &Logger,
    severity: Level,
    text: String,
) -> Result<Delivery, openpilot_logging::Error> {
    logger.emit(log_site!(), severity, text)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args().nth(1).ok_or("missing endpoint")?;
    let logger = if endpoint == "runtime" {
        Logger::for_runtime("test-version", "pc")?
    } else {
        Logger::new(endpoint, "test-version", "pc")?
    };
    let mut rate = RateLimit::default();
    for line in io::stdin().lock().lines() {
        let command: Command = serde_json::from_str(&line?)?;
        let mut sent = 0;
        let mut dropped = 0;
        let mut filtered = 0;
        let mut deliveries = Vec::new();
        match command {
            Command::Emit {
                level: severity,
                text,
            } => deliveries.push(emit(&logger, level(severity)?, text)?),
            Command::Rate { timestamp, text } => {
                let decision = rate.admit(timestamp)?;
                if decision.suppressed > 0 {
                    deliveries.push(emit(
                        &logger,
                        Level::Warning,
                        format!("cloudlog: {} messages suppressed", decision.suppressed),
                    )?);
                }
                if decision.emit {
                    deliveries.push(emit(&logger, Level::Info, text)?);
                }
            }
            Command::Flood { count } => {
                for _ in 0..count {
                    deliveries.push(emit(&logger, Level::Debug, "flood".into())?);
                }
            }
            Command::Threads { count } => {
                let handles: Vec<_> = (0..count)
                    .map(|i| {
                        let logger = logger.clone();
                        std::thread::spawn(move || {
                            emit(&logger, Level::Info, format!("thread-{i}"))
                        })
                    })
                    .collect();
                for handle in handles {
                    deliveries.push(handle.join().map_err(|_| "worker panicked")??);
                }
            }
            Command::Close => logger.close()?,
        }
        for delivery in deliveries {
            match delivery {
                Delivery::Sent => sent += 1,
                Delivery::Dropped => dropped += 1,
                Delivery::Filtered => filtered += 1,
            }
        }
        eprintln!(
            "{}",
            serde_json::json!({"sent": sent, "dropped": dropped, "filtered": filtered})
        );
    }
    logger.close()?;
    Ok(())
}
