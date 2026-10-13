use openpilot_carrot_server::{
    youtube_live::{rtmp::Client, sink::Sink},
    Value,
};
use std::{io::Write, sync::Arc};

pub fn run(input: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    let Value::Array(urls) = input.get("repeat_urls") else {
        return Err("fixture URLs missing".into());
    };
    let bytes = std::fs::read(input.get("input").string()?)?;
    let mut rows = Vec::new();
    for url in urls {
        let endpoint = url.string()?;
        let parsed = url::Url::parse(&endpoint)?;
        let host: std::net::IpAddr = parsed.host_str().ok_or("fixture host missing")?.parse()?;
        if !host.is_loopback() {
            return Err("fixture recipients must be loopback".into());
        }
        let client = Arc::new(Client::new(endpoint)?);
        let mut sink = Sink::new(Arc::clone(&client));
        client.connect()?;
        sink.write_all(&bytes)?;
        sink.flush()?;
        let written = client.bytes_written();
        client.close();
        drop(sink);
        drop(client);
        let mapped = std::fs::read_to_string("/proc/self/maps")?.contains("/librtmp.so.1");
        rows.push(Value::object([
            ("bytes_written", Value::integer(written)),
            ("library_mapped_after_client_drop", Value::Bool(mapped)),
        ]));
    }
    Ok(Value::Array(rows))
}
