//! Exact Linux AF_UNIX listener -> VFS identity. No pathname inference.
use super::{require, Result};
use rustix::{fs, net};
use std::{os::fd::AsFd, time::Duration};

fn word(bytes: &[u8]) -> u32 {
    u32::from_ne_bytes(bytes.try_into().unwrap())
}
fn half(bytes: &[u8]) -> u16 {
    u16::from_ne_bytes(bytes.try_into().unwrap())
}
fn device(dev: fs::Dev) -> Result<u32> {
    let major = fs::major(dev);
    let minor = fs::minor(dev);
    require(
        major < (1 << 12) && minor < (1 << 20),
        "UNIX_DIAG VFS device is not representable",
    )?;
    // unix_diag.c reports kernel s_dev (MKDEV), not userspace dev_t encoding.
    Ok((major << 20) | minor)
}
fn reply(data: &[u8], port: u32, inode: u32, cookie: u64) -> Result<(u32, u32)> {
    require(
        data.len() >= 32 && data.len() <= 4096,
        "invalid exact UNIX_DIAG reply size",
    )?;
    require(
        word(&data[0..4]) as usize == data.len(),
        "truncated or multiple UNIX_DIAG messages",
    )?;
    require(
        half(&data[4..6]) == 20 && half(&data[6..8]) == 0,
        "unexpected UNIX_DIAG type/flags",
    )?;
    require(
        word(&data[8..12]) == 1 && word(&data[12..16]) == port,
        "UNIX_DIAG sequence/recipient mismatch",
    )?;
    require(
        data[16..20] == [1, 1, 10, 0],
        "UNIX_DIAG is not an AF_UNIX stream listener",
    )?;
    require(
        word(&data[20..24]) == inode,
        "UNIX_DIAG socket inode mismatch",
    )?;
    require(
        word(&data[24..28]) == cookie as u32 && word(&data[28..32]) == (cookie >> 32) as u32,
        "UNIX_DIAG socket cookie mismatch",
    )?;
    let mut at = 32;
    let mut identity = None;
    while at < data.len() {
        require(data.len() - at >= 4, "short UNIX_DIAG attribute")?;
        let length = usize::from(half(&data[at..at + 2]));
        let kind = half(&data[at + 2..at + 4]);
        require(
            length >= 4 && length <= data.len() - at,
            "invalid UNIX_DIAG attribute length",
        )?;
        let aligned = (length + 3) & !3;
        require(
            aligned <= data.len() - at,
            "truncated UNIX_DIAG attribute alignment",
        )?;
        match kind {
            1 => {
                require(
                    length == 12 && identity.is_none(),
                    "invalid or duplicate UNIX_DIAG_VFS",
                )?;
                identity = Some((word(&data[at + 4..at + 8]), word(&data[at + 8..at + 12])));
            }
            6 => require(length == 5, "invalid UNIX_DIAG_SHUTDOWN")?,
            _ => return Err("unexpected UNIX_DIAG attribute".into()),
        }
        at += aligned;
    }
    identity.ok_or_else(|| "UNIX_DIAG_VFS absent; ownership unproved".into())
}

pub(super) fn matches(listener: impl AsFd, node: impl AsFd) -> Result<bool> {
    let metadata = fs::fstat(&node)?;
    if fs::FileType::from_raw_mode(metadata.st_mode) != fs::FileType::Socket {
        return Ok(false);
    }
    // UNIX_DIAG_VFS truncates the *old* bound inode too. The ext filesystem
    // format bounds all inode numbers to u32 (s_inodes_count). Do not infer
    // that bound from a replacement node's small inode or from inode counts.
    require(
        fs::fstatfs(&node)?.f_type == 0xef53,
        "UNIX_DIAG VFS requires an ext-family 32-bit inode filesystem",
    )?;
    if net::sockopt::socket_domain(&listener)? != net::AddressFamily::UNIX
        || net::sockopt::socket_type(&listener)? != net::SocketType::STREAM
        || !net::sockopt::socket_acceptconn(&listener)?
    {
        return Ok(false);
    }
    // The UAPI truncates both inodes to u32. Refuse to compare truncated values.
    let expected = (u32::try_from(metadata.st_ino)?, device(metadata.st_dev)?);
    let inode = u32::try_from(fs::fstat(&listener)?.st_ino)?;
    require(inode != 0, "UNIX_DIAG socket inode is zero")?;
    let cookie = net::sockopt::socket_cookie(&listener)?;
    require(
        cookie != 0 && cookie != u64::MAX,
        "UNIX_DIAG cookie is not an exact identity",
    )?;
    let socket = net::socket_with(
        net::AddressFamily::NETLINK,
        net::SocketType::DGRAM,
        net::SocketFlags::CLOEXEC,
        Some(net::netlink::SOCK_DIAG),
    )?;
    net::bind(&socket, &net::netlink::SocketAddrNetlink::new(0, 0))?;
    let local = net::netlink::SocketAddrNetlink::try_from(net::getsockname(&socket)?)?;
    require(
        local.pid() != 0 && local.groups() == 0,
        "invalid local UNIX_DIAG address",
    )?;
    for timeout in [net::sockopt::Timeout::Recv, net::sockopt::Timeout::Send] {
        net::sockopt::set_socket_timeout(&socket, timeout, Some(Duration::from_millis(100)))?;
    }
    let mut request = Vec::with_capacity(40);
    request.extend_from_slice(&40u32.to_ne_bytes());
    request.extend_from_slice(&20u16.to_ne_bytes()); // SOCK_DIAG_BY_FAMILY
    request.extend_from_slice(&1u16.to_ne_bytes()); // NLM_F_REQUEST, exact query
    request.extend_from_slice(&1u32.to_ne_bytes());
    request.extend_from_slice(&local.pid().to_ne_bytes());
    request.extend_from_slice(&[1, 0, 0, 0]); // AF_UNIX, protocol, pad
    request.extend_from_slice(&(1u32 << 10).to_ne_bytes()); // TCP_LISTEN
    request.extend_from_slice(&inode.to_ne_bytes());
    request.extend_from_slice(&2u32.to_ne_bytes()); // UDIAG_SHOW_VFS
    request.extend_from_slice(&(cookie as u32).to_ne_bytes());
    request.extend_from_slice(&((cookie >> 32) as u32).to_ne_bytes());
    require(
        net::sendto(
            &socket,
            &request,
            net::SendFlags::empty(),
            &net::netlink::SocketAddrNetlink::new(0, 0),
        )? == request.len(),
        "short UNIX_DIAG request",
    )?;
    let mut buffer = [0u8; 4096];
    let (_, count, sender) = net::recvfrom(&socket, &mut buffer[..], net::RecvFlags::TRUNC)?;
    let sender =
        net::netlink::SocketAddrNetlink::try_from(sender.ok_or("UNIX_DIAG sender absent")?)?;
    require(
        sender.pid() == 0 && sender.groups() == 0 && count <= buffer.len(),
        "UNIX_DIAG non-kernel sender or oversized reply",
    )?;
    Ok(reply(&buffer[..count], local.pid(), inode, cookie)? == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{
        fs::MetadataExt,
        net::{UnixDatagram, UnixListener},
    };
    fn node(path: &std::path::Path) -> rustix::fd::OwnedFd {
        fs::open(
            path,
            fs::OFlags::PATH | fs::OFlags::NOFOLLOW | fs::OFlags::CLOEXEC,
            fs::Mode::empty(),
        )
        .unwrap()
    }
    #[test]
    fn first_observation_rejects_foreign_inode_even_with_a_stale_owned_listener() {
        for kind in ["datagram", "closed-stream"] {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("socket");
            let listener = UnixListener::bind(&path).unwrap();
            let original = std::fs::symlink_metadata(&path).unwrap();
            let original_fd = node(&path);
            std::fs::rename(&path, root.path().join("original")).unwrap();
            let datagram = if kind == "datagram" {
                Some(UnixDatagram::bind(&path).unwrap())
            } else {
                drop(UnixListener::bind(&path).unwrap());
                None
            };
            let foreign = std::fs::symlink_metadata(&path).unwrap();
            assert_ne!(original.ino(), foreign.ino());
            // This is deliberately the FIRST identity query, without any cache.
            assert!(
                !matches(&listener, node(&path)).unwrap(),
                "first observation must reject a foreign VFS inode: {kind}"
            );
            assert!(matches(&listener, &original_fd).unwrap());
            drop(datagram);
        }
    }
    #[test]
    fn exact_reply_rejects_bad_framing_and_identity() {
        let cookie = 0x1122334455667788u64;
        let mut good = Vec::new();
        good.extend_from_slice(&44u32.to_ne_bytes());
        good.extend_from_slice(&20u16.to_ne_bytes());
        good.extend_from_slice(&0u16.to_ne_bytes());
        good.extend_from_slice(&1u32.to_ne_bytes());
        good.extend_from_slice(&37u32.to_ne_bytes());
        good.extend_from_slice(&[1, 1, 10, 0]);
        good.extend_from_slice(&41u32.to_ne_bytes());
        good.extend_from_slice(&(cookie as u32).to_ne_bytes());
        good.extend_from_slice(&((cookie >> 32) as u32).to_ne_bytes());
        good.extend_from_slice(&12u16.to_ne_bytes());
        good.extend_from_slice(&1u16.to_ne_bytes());
        good.extend_from_slice(&43u32.to_ne_bytes());
        good.extend_from_slice(&47u32.to_ne_bytes());
        assert_eq!(reply(&good, 37, 41, cookie).unwrap(), (43, 47));
        for index in [0, 4, 6, 8, 12, 16, 17, 18, 20, 24, 28, 32, 34] {
            let mut bad = good.clone();
            bad[index] ^= 0x40;
            assert!(
                reply(&bad, 37, 41, cookie).is_err(),
                "accepted modified framing/identity byte {index}"
            );
        }
        for length in 0..good.len() {
            assert!(reply(&good[..length], 37, 41, cookie).is_err());
        }
        let mut duplicate = good.clone();
        duplicate.extend_from_slice(&good[32..]);
        let length = duplicate.len() as u32;
        duplicate[..4].copy_from_slice(&length.to_ne_bytes());
        assert!(reply(&duplicate, 37, 41, cookie).is_err());
    }
    #[test]
    fn vfs_identity_rejects_unsupported_filesystems_and_device_truncation() {
        assert!(device(fs::makedev(4096, 1)).is_err());
        assert!(device(fs::makedev(1, 1 << 20)).is_err());
        let root = tempfile::tempdir_in("/dev/shm").unwrap();
        let path = root.path().join("socket");
        let listener = UnixListener::bind(&path).unwrap();
        assert!(matches(&listener, node(&path))
            .unwrap_err()
            .to_string()
            .contains("32-bit inode filesystem"));
    }
}
