use openpilot_athena::{http_socket, policy::UploadItem, upload_http, Error};

fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or(Error::Contract("path required"))?;
    if path == "--connect" {
        let port = args
            .next()
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or(Error::Contract("port required"))?;
        let millis = args
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(Error::Contract("timeout milliseconds required"))?;
        let _socket = openpilot_athena::net::tcp(
            "127.0.0.1",
            port,
            std::time::Duration::from_millis(millis),
        )?;
        println!("connected");
        return Ok(());
    }
    let url = args.next().ok_or(Error::Contract("url required"))?;
    let item = UploadItem {
        path,
        url,
        headers: serde_json::Map::new(),
        created_at: 0,
        id: Some("syscall-fixture".into()),
        retry_count: 0,
        current: false,
        progress: 0.,
        allow_cellular: true,
        priority: 0,
    };
    println!(
        "{}",
        upload_http::upload(&http_socket::agent(), &item, |_, _| Ok(()))?
    );
    Ok(())
}
