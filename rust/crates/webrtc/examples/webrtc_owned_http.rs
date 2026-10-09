use openpilot_webrtc::{network::Network, runtime, Error};
use std::net::{IpAddr, Ipv4Addr};

fn main() -> Result<(), Error> {
    let mut arguments = std::env::args().skip(1);
    let carrot = arguments.next().is_some_and(|mode| mode == "carrot");
    let port = arguments
        .next()
        .ok_or(Error::Contract("missing owned port"))?
        .parse::<u16>()
        .map_err(|_| Error::Contract("invalid owned port"))?;
    let server = arguments
        .next()
        .map(|server| {
            let (host, port) = server
                .rsplit_once(':')
                .ok_or(Error::Contract("invalid owned STUN server"))?;
            Ok::<_, Error>((
                host.to_owned(),
                port.parse::<u16>()
                    .map_err(|_| Error::Contract("invalid owned STUN port"))?,
            ))
        })
        .transpose()?;
    let network = Network::owned(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)], server);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tokio::task::LocalSet::new()
        .block_on(&runtime, runtime::serve("127.0.0.1", port, carrot, network))
}
