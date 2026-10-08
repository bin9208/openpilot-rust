use super::Error;
use std::{
    ffi::OsString,
    net::{Ipv4Addr, SocketAddrV4},
    num::NonZeroU64,
    path::PathBuf,
};

pub struct Options {
    pub root: PathBuf,
    pub assets: PathBuf,
    pub config: PathBuf,
    pub tcp: SocketAddrV4,
    pub http: SocketAddrV4,
    pub frames: Option<NonZeroU64>,
    pub device_ip: Option<String>,
}

pub fn parse(arguments: impl Iterator<Item = OsString>) -> Result<Option<Options>, Error> {
    let mut root = None;
    let mut assets = None;
    let mut config = None;
    let mut tcp = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 7711);
    let mut http = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8082);
    let mut frames = None;
    let mut device_ip = None;
    let mut arguments = arguments.peekable();
    while let Some(flag) = arguments.next() {
        if flag == "--help" && arguments.peek().is_none() {
            return Ok(None);
        }
        let value = arguments
            .next()
            .ok_or(Error::Contract("missing option value"))?;
        match flag.to_str() {
            Some("--root") => root = Some(PathBuf::from(value)),
            Some("--assets") => assets = Some(PathBuf::from(value)),
            Some("--config") => config = Some(PathBuf::from(value)),
            Some("--tcp-port" | "--http-port") => {
                let port = value
                    .to_str()
                    .and_then(|value| value.parse::<std::num::NonZeroU16>().ok())
                    .ok_or(Error::Contract("port must be 1 through 65535"))?;
                if flag == "--tcp-port" {
                    tcp.set_port(port.get());
                } else {
                    http.set_port(port.get());
                }
            }
            Some("--frames") => {
                frames = Some(
                    value
                        .to_str()
                        .and_then(|value| value.parse::<NonZeroU64>().ok())
                        .ok_or(Error::Contract("frames must be positive"))?,
                )
            }
            Some("--device-ip") => {
                device_ip = Some(
                    value
                        .into_string()
                        .map_err(|_| Error::Contract("device IP is not UTF-8"))?,
                )
            }
            _ => return Err(Error::Contract("unknown option; see --help")),
        }
    }
    let root = root
        .ok_or(Error::Contract("--root is required"))?
        .canonicalize()?;
    let service = root.join("openpilot/selfdrive/carrot/xiaoge");
    Ok(Some(Options {
        root,
        assets: assets.unwrap_or_else(|| service.join("assets")),
        config: config.unwrap_or_else(|| service.join("v_asm_config.json")),
        tcp,
        http,
        frames,
        device_ip,
    }))
}
