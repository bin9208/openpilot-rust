use openpilot_panda_usb::Api;
use openpilot_pandad::{
    firmware::{
        client::Handle,
        usb::{self, Log},
    },
    supervisor::Fault,
};
use serde_json::{json, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::var("PANDA_FIRMWARE_USB_CASE")?;
    let fixture_path = std::env::var("PANDA_FIRMWARE_USB_LIBRARY")?;
    let case: Value = serde_json::from_str(&input)?;
    let api = Api::system()?;
    let maps = std::fs::read_to_string("/proc/self/maps")?;
    if !maps.contains(&fixture_path) {
        return Err("owned USB fixture is not mapped; refusing device access".into());
    }
    let mut logs = Vec::new();
    let logger = |entry| {
        logs.push(match entry {
            Log::Opening { serial, product } => {
                json!(["debug", format!("opening device {serial} {product:#x}")])
            }
            Log::InvalidSerial(serial) => json!([
                "warning",
                format!("found device with panda descriptors but invalid serial: {serial}")
            ]),
            Log::Exception(text, _) => json!(["exception", text]),
        });
        Ok(())
    };
    let serial = case["serial"].as_str();
    let result: Result<Value, Fault> = match case["mode"].as_str() {
        Some("list") => usb::list(api, logger).map(|value|json!(value)),
        Some("dfu_list") => Ok(json!(usb::dfu_list(api))),
        Some("dfu_connect") => usb::dfu_connect(api, serial).and_then(|selected| {
            selected.map_or(Ok(Value::Null), |(mut handle,mcu)| { handle.close()?; Ok(json!(mcu)) })
        }),
        Some("connect") => usb::connect(api, serial.ok_or("runtime serial required")?, case["claim"].as_bool().unwrap_or(true),
            case["no_error"].as_bool().unwrap_or(false), logger).and_then(|selected| {
                selected.map_or(Ok(Value::Null), |mut connection| {
                    connection.handle.close()?;
                    Ok(json!({"serial":connection.serial,"bootstub":connection.bootstub,"bcd":connection.bcd,"spi":connection.spi}))
                })
            }),
        _ => return Err("unknown USB policy fixture mode".into()),
    };
    println!(
        "{}",
        match result {
            Ok(value) => json!({"ok":true,"value":value,"logs":logs}),
            Err(error) => json!({"ok":false,"error":error.to_string(),"logs":logs}),
        }
    );
    Ok(())
}
