use openpilot_usbgpu::model_delivery::{self, Error};
use std::io::Read;

fn main() -> Result<(), Error> {
    let url = std::env::args()
        .nth(1)
        .ok_or_else(|| Error::Invalid("missing owned HTTP URL".into()))?;
    let response = model_delivery::agent(3).get(url).call()?;
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0; 4 << 20];
    let mut events = Vec::new();
    loop {
        match reader.read(&mut buffer) {
            Ok(count) => {
                events.push(serde_json::json!({"read":count,"bytes":&buffer[..count]}));
                if count == 0 {
                    break;
                }
            }
            Err(error) => {
                events.push(serde_json::json!({"error":error.to_string(),"kind":format!("{:?}",error.kind())}));
                break;
            }
        }
    }
    serde_json::to_writer(std::io::stdout().lock(), &events)?;
    Ok(())
}
