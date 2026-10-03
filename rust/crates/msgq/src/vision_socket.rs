use crate::{queue, vision_wire::MAX_FDS, Error};
use std::{
    mem::{size_of, zeroed},
    os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) fn path(name: &str) -> Result<PathBuf, Error> {
    let prefix = queue::prefix()?;
    if !queue::component(name, 40, false)
        || prefix
            .as_ref()
            .is_some_and(|prefix| !queue::component(prefix, 40, true))
    {
        return Err(Error::Invalid("invalid VisionIPC server name or namespace"));
    }
    Ok(PathBuf::from(match prefix {
        Some(prefix) => format!("/tmp/{prefix}_visionipc_{name}"),
        None => format!("/tmp/visionipc_{name}"),
    }))
}

fn address(path: &std::path::Path) -> Result<libc::sockaddr_un, Error> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: all-zero bytes are valid for sockaddr_un, including its pathname.
    let mut address: libc::sockaddr_un = unsafe { zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(Error::Invalid("invalid VisionIPC socket path"));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as libc::c_char;
    }
    Ok(address)
}

fn socket() -> Result<OwnedFd, Error> {
    // SAFETY: socket takes only scalar arguments and returns a fresh owned FD.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(Error::last("create VisionIPC socket"));
    }
    // SAFETY: successful socket transferred this fresh FD to this sole owner.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub(crate) fn connect(path: &std::path::Path) -> Result<Option<OwnedFd>, Error> {
    let address = address(path)?;
    let socket = socket()?;
    // SAFETY: address is an initialized sockaddr_un, with the matching byte
    // length; socket remains owned and live for the call.
    let result = unsafe {
        libc::connect(
            socket.as_raw_fd(),
            std::ptr::from_ref(&address).cast(),
            size_of::<libc::sockaddr_un>() as libc::socklen_t,
        )
    };
    if result != 0 {
        return Ok(None);
    }
    Ok(Some(socket))
}

pub(crate) fn bind(path: &std::path::Path) -> Result<OwnedFd, Error> {
    let address = address(path)?;
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::Io("unlink VisionIPC socket", error)),
    }
    let socket = socket()?;
    // SAFETY: address has the matching initialized UNIX-socket layout and size.
    if unsafe {
        libc::bind(
            socket.as_raw_fd(),
            std::ptr::from_ref(&address).cast(),
            size_of::<libc::sockaddr_un>() as libc::socklen_t,
        )
    } != 0
    {
        return Err(Error::last("bind VisionIPC socket"));
    }
    // SAFETY: this live socket was just bound; the backlog is the original value.
    if unsafe { libc::listen(socket.as_raw_fd(), 3) } != 0 {
        let error = Error::last("listen on VisionIPC socket");
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(socket)
}

pub(crate) fn accept(socket: BorrowedFd<'_>) -> Result<OwnedFd, Error> {
    // SAFETY: no peer address is requested, and a successful accept4 creates a
    // new descriptor independent of the borrowed listener.
    let fd = unsafe {
        libc::accept4(
            socket.as_raw_fd(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            libc::SOCK_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(Error::last("accept VisionIPC client"));
    }
    // SAFETY: accept4 returned this freshly owned FD exactly once.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub(crate) fn wait(socket: BorrowedFd<'_>, stop: &AtomicBool) -> Result<bool, Error> {
    while !stop.load(Ordering::Acquire) {
        let mut descriptor = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor is a live writable one-element pollfd array.
        let result = unsafe { libc::poll(&mut descriptor, 1, 100) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(Error::Io("poll VisionIPC socket", error));
        }
        if descriptor.revents != 0 {
            return Ok(!stop.load(Ordering::Acquire));
        }
    }
    Ok(false)
}

#[repr(C)]
struct Control {
    header: libc::cmsghdr,
    descriptors: [libc::c_int; MAX_FDS],
}

fn control_length<T, U: TryFrom<T>>(length: T) -> Result<U, Error> {
    U::try_from(length).map_err(|_| Error::Invalid("VisionIPC control length exceeds socket ABI"))
}

pub(crate) fn send(
    socket: BorrowedFd<'_>,
    payload: &[u8],
    descriptors: &[OwnedFd],
) -> Result<(), Error> {
    if descriptors.len() > MAX_FDS {
        return Err(Error::Invalid("too many VisionIPC descriptors"));
    }
    // SAFETY: zero is a valid initialized representation of these POSIX structs.
    let mut control: Control = unsafe { zeroed() };
    // SAFETY: zero initializes unused msghdr pointers and lengths to empty.
    let mut message: libc::msghdr = unsafe { zeroed() };
    let mut vector = libc::iovec {
        iov_base: payload.as_ptr().cast_mut().cast(),
        iov_len: payload.len(),
    };
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    if !descriptors.is_empty() {
        control.header.cmsg_level = libc::SOL_SOCKET;
        control.header.cmsg_type = libc::SCM_RIGHTS;
        let length = size_of::<libc::cmsghdr>() + descriptors.len() * size_of::<libc::c_int>();
        control.header.cmsg_len = control_length(length)?;
        for (target, fd) in control.descriptors.iter_mut().zip(descriptors) {
            *target = fd.as_raw_fd();
        }
        message.msg_control = std::ptr::from_mut(&mut control).cast();
        message.msg_controllen =
            control_length((length + size_of::<usize>() - 1) & !(size_of::<usize>() - 1))?;
    }
    // SAFETY: every msghdr/iovec/control pointer covers initialized storage alive
    // throughout sendmsg; the kernel reads but never mutates the borrowed payload.
    let result = unsafe { libc::sendmsg(socket.as_raw_fd(), &message, libc::MSG_NOSIGNAL) };
    if result < 0 {
        return Err(Error::last("send VisionIPC packet"));
    }
    if usize::try_from(result).ok() != Some(payload.len()) {
        return Err(Error::Corrupt("short VisionIPC packet send"));
    }
    Ok(())
}

pub(crate) fn receive(
    socket: BorrowedFd<'_>,
    maximum: usize,
) -> Result<(Vec<u8>, Vec<OwnedFd>), Error> {
    let mut payload = vec![0; maximum];
    // SAFETY: all fields of Control accept the all-zero representation.
    let mut control: Control = unsafe { zeroed() };
    // SAFETY: zero initializes unused msghdr pointers and lengths to empty.
    let mut message: libc::msghdr = unsafe { zeroed() };
    let mut vector = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    message.msg_control = std::ptr::from_mut(&mut control).cast();
    message.msg_controllen = control_length(size_of::<Control>())?;
    // SAFETY: all output pointers cover writable allocations of the advertised
    // sizes; the kernel owns creation of descriptors returned in SCM_RIGHTS.
    let result = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
    if result < 0 {
        return Err(Error::last("receive VisionIPC packet"));
    }
    let mut descriptors = Vec::new();
    let mut invalid = false;
    // SAFETY: recvmsg initialized the control region and bounded msg_controllen
    // to its supplied size; CMSG traversal remains inside that returned region.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            let value = &*header;
            let length: usize = control_length(value.cmsg_len)?;
            let length = length.saturating_sub(size_of::<libc::cmsghdr>());
            if value.cmsg_level == libc::SOL_SOCKET && value.cmsg_type == libc::SCM_RIGHTS {
                invalid |= length == 0 || !length.is_multiple_of(size_of::<libc::c_int>());
                for index in 0..length / size_of::<libc::c_int>() {
                    let fd = libc::CMSG_DATA(header)
                        .cast::<libc::c_int>()
                        .add(index)
                        .read_unaligned();
                    // Each returned SCM_RIGHTS entry is a distinct newly installed FD.
                    descriptors.push(OwnedFd::from_raw_fd(fd));
                }
            } else {
                invalid = true;
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if invalid || message.msg_flags & !libc::MSG_CMSG_CLOEXEC != 0 {
        return Err(Error::Corrupt(
            "truncated or invalid VisionIPC ancillary data",
        ));
    }
    let length =
        usize::try_from(result).map_err(|_| Error::Corrupt("invalid received packet length"))?;
    payload.truncate(length);
    Ok((payload, descriptors))
}

#[cfg(test)]
#[path = "vision_socket_tests.rs"]
mod tests;
