use crate::{discovery::Udp, mac, Datagram, Error, Registration, PORT};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    io,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Codec {
    #[default]
    Auto,
    Jpeg,
    H264,
}
pub fn capability_request(config: &Registration, nonce: [u8; 16]) -> Vec<u8> {
    let mut request = b"S3C1".to_vec();
    request.extend(config.device);
    request.extend(nonce);
    request.extend(mac(&config.key, &request));
    request
}
pub fn capability_reply(config: &Registration, nonce: [u8; 16], reply: &[u8]) -> Option<u32> {
    let mut prefix = b"S3C2".to_vec();
    prefix.extend(config.device);
    prefix.extend(nonce);
    if reply.len() != 62 || reply[..26] != prefix {
        return None;
    }
    let mut check =
        Hmac::<Sha256>::new_from_slice(&config.key).expect("HMAC accepts a 32-byte key");
    check.update(&reply[..30]);
    check.verify_slice(&reply[30..]).ok()?;
    Some(u32::from_be_bytes(reply[26..30].try_into().ok()?))
}
pub fn capabilities_with<D: Datagram>(
    config: &Registration,
    timeout: f64,
    targets: &[&str],
    nonce: [u8; 16],
    mut clock: impl FnMut() -> f64,
    create: impl FnOnce() -> io::Result<D>,
) -> Result<u32, Error> {
    let request = capability_request(config, nonce);
    let deadline = clock() + timeout;
    let mut socket = create()?;
    socket.broadcast()?;
    for target in targets {
        socket.send_to(&request, target, PORT)?;
    }
    while clock() < deadline {
        socket.timeout(0.01_f64.max(deadline - clock()))?;
        let (reply, _) = match socket.receive(128) {
            Ok(reply) => reply,
            Err(e) if e.kind() == io::ErrorKind::TimedOut => break,
            Err(e) => return Err(e.into()),
        };
        if let Some(flags) = capability_reply(config, nonce, &reply) {
            return Ok(flags);
        }
    }
    Ok(0)
}
pub fn capabilities(
    config: &Registration,
    timeout: Duration,
    targets: &[&str],
) -> Result<u32, Error> {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).map_err(|e| io::Error::other(e.to_string()))?;
    let start = Instant::now();
    capabilities_with(
        config,
        timeout.as_secs_f64(),
        targets,
        nonce,
        || start.elapsed().as_secs_f64(),
        Udp::new,
    )
}
pub fn select_codec_with(
    config: &Registration,
    jpeg_fallback: bool,
    query: impl FnOnce() -> Result<u32, Error>,
) -> Result<Codec, Error> {
    if config.codec == Codec::Jpeg || (config.codec == Codec::Auto && jpeg_fallback) {
        return Ok(Codec::Jpeg);
    }
    if query()? & 1 != 0 {
        return Ok(Codec::H264);
    }
    if config.codec == Codec::H264 {
        return Err(io::Error::other(
            "S3 H.264 support not confirmed; update firmware to 1.2.0 or check Wi-Fi",
        )
        .into());
    }
    Ok(Codec::Jpeg)
}
pub fn select_codec(config: &Registration, jpeg_fallback: bool) -> Result<Codec, Error> {
    select_codec_with(config, jpeg_fallback, || {
        capabilities(config, Duration::from_secs(1), &["255.255.255.255"])
    })
}
