use crate::{health::Health, supervisor::Log};
use openpilot_logging::{
    record::{Level, Record},
    Fields, Value,
};
use unicode_general_category::{get_general_category, GeneralCategory};

fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut output = String::from(quote);
    for character in text.chars() {
        let point = u32::from(character);
        match character {
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\\' => output.push_str("\\\\"),
            value if value == quote => {
                output.push('\\');
                output.push(value);
            }
            value
                if value == ' '
                    || !matches!(
                        get_general_category(value),
                        GeneralCategory::Control
                            | GeneralCategory::Format
                            | GeneralCategory::Surrogate
                            | GeneralCategory::PrivateUse
                            | GeneralCategory::Unassigned
                            | GeneralCategory::LineSeparator
                            | GeneralCategory::ParagraphSeparator
                            | GeneralCategory::SpaceSeparator
                    ) =>
            {
                output.push(value)
            }
            _ if point <= 255 => output.push_str(&format!("\\x{point:02x}")),
            _ if point <= 65535 => output.push_str(&format!("\\u{point:04x}")),
            _ => output.push_str(&format!("\\U{point:08x}")),
        }
    }
    output.push(quote);
    output
}

fn health(value: Health) -> Value {
    let mut fields = Fields::new();
    for (name, number) in [
        ("uptime", u64::from(value.uptime)),
        ("voltage", u64::from(value.voltage)),
        ("current", u64::from(value.current)),
        ("safety_tx_blocked", u64::from(value.safety_tx_blocked)),
        ("safety_rx_invalid", u64::from(value.safety_rx_invalid)),
        ("tx_buffer_overflow", u64::from(value.tx_overflow)),
        ("rx_buffer_overflow", u64::from(value.rx_overflow)),
        ("faults", u64::from(value.faults)),
        ("ignition_line", u64::from(value.ignition_line)),
        ("ignition_can", u64::from(value.ignition_can)),
        ("controls_allowed", u64::from(value.controls_allowed)),
        ("car_harness_status", u64::from(value.harness_status)),
        ("safety_mode", u64::from(value.safety_model)),
        ("safety_param", u64::from(value.safety_param)),
        ("fault_status", u64::from(value.fault_status)),
        ("power_save_enabled", u64::from(value.power_save)),
        ("heartbeat_lost", u64::from(value.heartbeat_lost)),
        (
            "alternative_experience",
            u64::from(value.alternative_experience),
        ),
    ] {
        fields.insert(name.into(), Value::Integer(number.into()));
    }
    fields.insert(
        "interrupt_load".into(),
        Value::Float(f64::from(value.interrupt_load)),
    );
    for (name, number) in [
        ("fan_power", u64::from(value.fan_power)),
        (
            "safety_rx_checks_invalid",
            u64::from(value.safety_rx_checks_invalid),
        ),
        (
            "spi_checksum_error_count",
            u64::from(value.spi_checksum_errors),
        ),
        ("fan_stall_count", u64::from(value.fan_stall_count)),
        ("sbu1_voltage_mV", u64::from(value.sbu1_mv)),
        ("sbu2_voltage_mV", u64::from(value.sbu2_mv)),
        ("som_reset_triggered", u64::from(value.som_reset_triggered)),
    ] {
        fields.insert(name.into(), Value::Integer(number.into()));
    }
    Value::Object(fields)
}

pub fn record(entry: Log) -> Result<Record, openpilot_logging::Error> {
    Ok(match entry {
        Log::Info(text) => Record::text(Level::Info, text),
        Log::Warning(text) => Record::text(Level::Warning, text),
        Log::Error(text) => Record::text(Level::Error, text.into()),
        Log::Exception(text, error) => Record::text(Level::Error, text.into()).with_exception(error.to_string()),
        Log::Connect { count } => Record::event("pandad.flash_and_connect", Vec::new(),
            [("count".into(), Value::Integer(count.into()))].into_iter().collect())?,
        Log::HeartbeatLost { health: value, serial } => Record::event("heartbeat lost", Vec::new(),
            [("deviceState".into(), health(value)), ("serial".into(), Value::Text(serial))].into_iter().collect())?,
        Log::SomReset { health: value, serial } => Record::event("panda.som_reset_triggered", Vec::new(),
            [("health".into(), health(value)), ("serial".into(), Value::Text(serial))].into_iter().collect())?,
        Log::DevelopmentBootloader { version, internal } => Record::text(Level::Info, format!(
            "Flashed firmware not booting, flashing development bootloader. bootstub_version={}, internal_panda={}",
            quoted(&version), if internal { "True" } else { "False" })),
        Log::Found(serials) => Record::text(Level::Info, format!("{} panda(s) found, connecting - [{}]",
            serials.len(), serials.iter().map(|value| quoted(value)).collect::<Vec<_>>().join(", "))),
    })
}
