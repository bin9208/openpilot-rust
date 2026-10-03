//! Separate debug-port behavior from nmeaport.py; not the primary GNSS publisher.
use crate::{config::Config, daemon::antenna, Error};
use num_bigint::BigInt;
use std::sync::atomic::AtomicBool;
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum Number {
    Integer(String),
    Float(f64),
}
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Message {
    pub kind: &'static str,
    pub fields: Vec<(&'static str, Option<Number>)>,
}
const CLOCK: [(&str, bool); 9] = [
    ("flags", false),
    ("leap_seconds", false),
    ("time_ns", false),
    ("time_uncertainty_ns", false),
    ("full_bias_ns", false),
    ("bias_ns", true),
    ("bias_uncertainty_ns", true),
    ("drift_nsps", true),
    ("drift_uncertainty_nsps", true),
];
const MEAS: [(&str, bool); 13] = [
    ("messageCount", false),
    ("messageNum", false),
    ("svCount", false),
    ("constellation", false),
    ("svId", false),
    ("flags", false),
    ("time_offset_ns", false),
    ("state", false),
    ("time_of_week_ns", false),
    ("time_of_week_uncertainty_ns", false),
    ("carrier_to_noise_ratio", true),
    ("pseudorange_rate", true),
    ("pseudorange_rate_uncertainty", true),
];
pub fn checksum_delimiter(line: &str) -> bool {
    // Source computes XOR but never compares it with the two supplied checksum characters.
    line.chars().skip(1).position(|value| value == '*') == line.chars().count().checked_sub(4)
        && line.chars().skip(1).any(|value| value == '*')
}
pub fn parse(line: &str) -> Result<Option<Message>, Error> {
    let line = line.trim();
    if !line.starts_with('$') || !checksum_delimiter(line) {
        return Ok(None);
    }
    let mut fields = line.split(',');
    let (kind, spec): (&str, &[(&str, bool)]) = match fields.next() {
        Some("$GNCLK") => ("GnssClockNmeaPort", &CLOCK),
        Some("$GNMEAS") => ("GnssMeasNmeaPort", &MEAS),
        _ => return Ok(None),
    };
    let fields: Vec<_> = fields.collect();
    if fields.len() < spec.len() {
        return Err(Error::Protocol("NMEA missing positional field"));
    }
    let mut fields = fields.into_iter();
    let mut values = Vec::with_capacity(spec.len());
    for &(name, float) in spec {
        let raw = fields
            .next()
            .ok_or(Error::Protocol("NMEA missing positional field"))?;
        let value = if raw.is_empty() {
            None
        } else if float {
            Some(Number::Float(
                raw.trim()
                    .parse()
                    .map_err(|_| Error::Protocol("NMEA float"))?,
            ))
        } else {
            Some(Number::Integer(
                raw.trim()
                    .parse::<BigInt>()
                    .map_err(|_| Error::Protocol("NMEA integer"))?
                    .to_string(),
            ))
        };
        values.push((name, value));
    }
    Ok(Some(Message {
        kind,
        fields: values,
    }))
}
impl std::fmt::Display for Message {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{}(", self.kind)?;
        for (index, (name, value)) in self.fields.iter().enumerate() {
            if index != 0 {
                write!(output, ", ")?;
            }
            write!(output, "{name}=")?;
            match value {
                None => write!(output, "None")?,
                Some(Number::Integer(value)) => write!(output, "{value}")?,
                Some(Number::Float(value)) => {
                    let mut text = String::new();
                    openpilot_runtime_core::python_float::write_float(*value, &mut text)?;
                    write!(
                        output,
                        "{}",
                        text.replace("NaN", "nan").replace("Infinity", "inf")
                    )?;
                }
            }
        }
        write!(output, ")")
    }
}
pub fn setup(config: &Config, stop: &AtomicBool) -> Result<(), Error> {
    println!("power up antenna ...");
    antenna(config, true)?;
    for (query, configure) in [
        ("AT+QGPS?", "AT+QGPSEND"),
        (
            "AT+QGPSCFG=\"outport\"",
            "AT+QGPSCFG=\"outport\",\"usbnmea\"",
        ),
        (
            "AT+QGPSCFG=\"gnssrawdata\"",
            "AT+QGPSCFG=\"gnssrawdata\",3,0",
        ),
    ] {
        // Source compares bytes membership against at_cmd's str result: a nonempty reply raises TypeError.
        if !config.at.command(query, stop)?.is_empty() {
            return Err(Error::Protocol(
                "'in <string>' requires string as left operand, not bytes",
            ));
        }
        println!(
            "{}",
            match query {
                "AT+QGPS?" => "stop location tracking ...",
                "AT+QGPSCFG=\"outport\"" => "configure outport ...",
                _ => "configure gnssrawdata ...",
            }
        );
        config.at.command(configure, stop)?;
    }
    println!("rebooting ...");
    config.at.command("AT+CFUN=1,1", stop)?;
    Err(Error::NmeaReboot)
}
