use super::{dns, mdns::Resolver};
use crate::Error;
use serde::Serialize;
use std::{
    fmt::Write,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};
use tokio::task::JoinSet;

#[derive(Serialize)]
struct Observation {
    scenario: &'static str,
    queries: Vec<String>,
    replies: Vec<String>,
    results: Vec<Option<String>>,
    elapsed: f64,
    pending: usize,
    socket_rebound: bool,
}

fn hex(bytes: &[u8]) -> Result<String, Error> {
    let mut result = String::new();
    for value in bytes {
        write!(result, "{value:02x}")?;
    }
    Ok(result)
}

fn response(query: &[u8], address: IpAddr, class: u16) -> Vec<u8> {
    let mut result = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
    result.extend_from_slice(&query[12..query.len() - 4]);
    let (kind, length, bytes) = match address {
        IpAddr::V4(value) => (1_u16, 4_u16, value.octets().to_vec()),
        IpAddr::V6(value) => (28_u16, 16_u16, value.octets().to_vec()),
    };
    result.extend_from_slice(&kind.to_be_bytes());
    result.extend_from_slice(&class.to_be_bytes());
    result.extend_from_slice(&120_u32.to_be_bytes());
    result.extend_from_slice(&length.to_be_bytes());
    result.extend_from_slice(&bytes);
    result
}

fn reply(raw: &[u8], kind: &str) -> Vec<u8> {
    let ip = if matches!(kind, "aaaa" | "first") {
        IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    };
    let mut packet = response(raw, ip, 0x8001);
    let tail = raw.len() - 4;
    match kind {
        "compressed" => {
            let mut compressed = vec![0, 0, 0x84, 0, 0, 1, 0, 1, 0, 0, 0, 0];
            compressed.extend_from_slice(&raw[12..]);
            compressed.extend_from_slice(&[0xc0, 12]);
            compressed.extend_from_slice(&packet[tail..]);
            packet = compressed;
        }
        "first" => {
            packet[7] = 2;
            packet.extend_from_slice(&[0xc0, 12]);
            packet
                .extend_from_slice(&response(raw, IpAddr::V4(Ipv4Addr::LOCALHOST), 0x8001)[tail..]);
        }
        "flags" => packet[..4].copy_from_slice(&[0x12, 0x34, 0, 0]),
        _ => {}
    }
    packet
}

async fn collect(
    tasks: &mut JoinSet<(usize, Result<Option<IpAddr>, Error>)>,
) -> Result<Vec<Option<String>>, Error> {
    let mut results = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((index, value)) => results.push((index, value?.map(|address| address.to_string()))),
            Err(error) if error.is_cancelled() => {}
            Err(_) => return Err(Error::Contract("owned DNS task failed")),
        }
    }
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, value)| value).collect())
}

async fn query(socket: &tokio::net::UdpSocket) -> Result<(Vec<u8>, SocketAddr), Error> {
    let mut raw = vec![0_u8; 65_536];
    let (length, address) =
        tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut raw))
            .await
            .map_err(|_| Error::Contract("owned DNS query missing"))??;
    raw.truncate(length);
    assert_eq!(raw, dns::query("OwNeD.local")?);
    Ok((raw, address))
}

async fn scenario(kind: &'static str) -> Result<Observation, Error> {
    let socket = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let resolver = Resolver::new(Some(socket.local_addr()?));
    let owner = resolver.acquire()?;
    let mut tasks = JoinSet::new();
    let lease = resolver.acquire()?;
    let first = tasks.spawn(async move { (0, lease.resolve("OwNeD.local").await) });
    if matches!(kind, "coalesced" | "cancel") {
        let lease = resolver.acquire()?;
        tasks.spawn(async move { (1, lease.resolve("owned.local").await) });
    }
    let started = Instant::now();
    let (raw, address) = query(&socket).await?;
    let mut queries = vec![hex(&raw)?];
    let mut replies = Vec::new();
    if kind == "cancel" {
        first.abort();
    }
    match kind {
        "closed" => resolver.close()?,
        "unresolved" => {}
        _ => {
            if kind == "malformed" {
                let mut truncated = response(&raw, IpAddr::V4(Ipv4Addr::LOCALHOST), 0x8001);
                truncated.pop();
                let mut notify = response(&raw, IpAddr::V4(Ipv4Addr::LOCALHOST), 0x8001);
                notify[2] |= 0x20;
                for packet in [
                    vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0xc0, 12],
                    truncated,
                    response(&raw, IpAddr::V4(Ipv4Addr::LOCALHOST), 1),
                    notify,
                ] {
                    replies.push(hex(&packet)?);
                    socket.send_to(&packet, address).await?;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
                assert_eq!(resolver.pending()?, 1);
            }
            let packet = reply(&raw, kind);
            replies.push(hex(&packet)?);
            socket.send_to(&packet, address).await?;
        }
    }
    let mut results = collect(&mut tasks).await?;
    if kind == "uncached" {
        let lease = resolver.acquire()?;
        tasks.spawn(async move { (0, lease.resolve("OwNeD.local").await) });
        let (raw, second_address) = query(&socket).await?;
        assert_eq!(second_address, address);
        queries.push(hex(&raw)?);
        let packet = reply(&raw, kind);
        replies.push(hex(&packet)?);
        socket.send_to(&packet, address).await?;
        results.extend(collect(&mut tasks).await?);
    }
    let pending = resolver.pending()?;
    assert_eq!(pending, 0);
    let count = if matches!(kind, "coalesced" | "uncached") {
        2
    } else {
        1
    };
    let expected = match kind {
        "unresolved" | "closed" => None,
        "aaaa" | "first" => Some("::1".to_owned()),
        _ => Some("127.0.0.1".to_owned()),
    };
    assert_eq!(results, vec![expected; count]);
    let mut buffer = [0_u8; 256];
    assert!(
        tokio::time::timeout(Duration::from_millis(20), socket.recv_from(&mut buffer))
            .await
            .is_err()
    );
    drop(owner);
    let rebound = UdpSocket::bind(address)?;
    Ok(Observation {
        scenario: kind,
        queries,
        replies,
        results,
        elapsed: started.elapsed().as_secs_f64(),
        pending,
        socket_rebound: rebound.local_addr()? == address,
    })
}

#[tokio::test]
async fn owned_udp_source_boundaries() -> Result<(), Error> {
    for kind in [
        "a",
        "aaaa",
        "coalesced",
        "uncached",
        "cancel",
        "closed",
        "malformed",
        "unresolved",
        "first",
        "compressed",
        "flags",
    ] {
        println!(
            "MDNS_OBSERVATION {}",
            serde_json::to_string(&scenario(kind).await?)?
        );
    }
    Ok(())
}
