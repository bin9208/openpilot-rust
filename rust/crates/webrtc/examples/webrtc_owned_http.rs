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
    let mut server = None;
    let mut resolver = None;
    let mut debug = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--debug" => debug = true,
            "--mdns" => {
                resolver = Some(
                    arguments
                        .next()
                        .ok_or(Error::Contract("missing owned mDNS recipient"))?,
                );
            }
            _ if server.is_none() => server = Some(argument),
            _ => return Err(Error::Contract("invalid owned resolver argument")),
        }
    }
    let server = server
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
    let mut network = Network::owned(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)], server);
    if let Some(recipient) = resolver {
        network = network.owned_mdns(
            recipient
                .parse()
                .map_err(|_| Error::Contract("invalid owned mDNS recipient"))?,
        )?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tokio::task::LocalSet::new().block_on(
        &runtime,
        runtime::serve(
            "127.0.0.1",
            port,
            runtime::Profile { carrot, debug },
            network,
        ),
    )
}
