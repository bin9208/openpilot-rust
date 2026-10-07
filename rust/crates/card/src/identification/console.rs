use super::Event;

impl Event<'_> {
    pub fn console(&self) -> Result<String, std::fmt::Error> {
        Ok(match self {
            Self::MalformedVin { vin } => {
                format!("{{'event': 'Malformed VIN', 'vin': {}}}", string_repr(vin))
            }
            Self::Unmatched { fingerprints } => format!(
                "{{'event': \"car doesn't match any fingerprints\", 'fingerprints': {}}}",
                string_repr(fingerprints)
            ),
            Self::Fingerprinted {
                car_fingerprint,
                source,
                fuzzy,
                cached,
                fw_count,
                ecu_responses,
                vin_rx_addr,
                vin_rx_bus,
                fingerprints,
                fw_query_time,
            } => {
                let responses = ecu_responses
                    .iter()
                    .map(|target| {
                        format!(
                            "({}, {}, {})",
                            target.0,
                            target
                                .1
                                .map_or_else(|| "None".into(), |address| address.to_string()),
                            target.2
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{'event': 'fingerprinted', 'car_fingerprint': {}, 'source': {source}, 'fuzzy': {}, 'cached': {}, 'fw_count': {fw_count}, 'ecu_responses': [{responses}], 'vin_rx_addr': {vin_rx_addr}, 'vin_rx_bus': {vin_rx_bus}, 'fingerprints': {}, 'fw_query_time': {}}}",
                    string_repr(car_fingerprint),
                    if *fuzzy { "True" } else { "False" },
                    if *cached { "True" } else { "False" },
                    string_repr(fingerprints),
                    super::float_text::repr(*fw_query_time)?
                )
            }
        })
    }
}

pub(crate) fn string_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut output = String::new();
    output.push(quote);
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            character if character == quote => {
                output.push('\\');
                output.push(character);
            }
            character if printable(character) => output.push(character),
            character => {
                let point = u32::from(character);
                output.push_str(&match point {
                    0..=255 => format!("\\x{point:02x}"),
                    256..=65535 => format!("\\u{point:04x}"),
                    _ => format!("\\U{point:08x}"),
                });
            }
        }
    }
    output.push(quote);
    output
}

fn printable(character: char) -> bool {
    let point = u32::from(character);
    let ranges = super::printable_ranges::NON_PRINTABLE;
    let index = ranges.partition_point(|&(start, _)| start <= point);
    index == 0 || point > ranges[index - 1].1
}
