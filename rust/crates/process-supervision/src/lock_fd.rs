use crate::Error;
use rustix::net::{
    recvmsg, sendmsg, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags,
    SendAncillaryBuffer, SendAncillaryMessage, SendFlags,
};
use std::{
    io::{IoSlice, IoSliceMut, Read, Write},
    mem::MaybeUninit,
    os::{
        fd::{BorrowedFd, OwnedFd},
        unix::net::UnixStream,
    },
};

pub(crate) fn send(stream: &mut UnixStream, lock: BorrowedFd<'_>) -> Result<(), Error> {
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let descriptors = [lock];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !control.push(SendAncillaryMessage::ScmRights(&descriptors)) {
        return Err(Error::LaunchProtocol("repository lock control buffer full"));
    }
    let sent = sendmsg(
        &*stream,
        &[IoSlice::new(b"L")],
        &mut control,
        SendFlags::NOSIGNAL,
    )
    .map_err(std::io::Error::from)?;
    if sent != 1 {
        return Err(Error::LaunchProtocol("repository lock marker not sent"));
    }
    let mut acknowledgement = [0];
    stream.read_exact(&mut acknowledgement)?;
    if acknowledgement != *b"L" {
        return Err(Error::LaunchProtocol(
            "repository lock acknowledgement missing",
        ));
    }
    Ok(())
}

pub(crate) fn receive(stream: &mut UnixStream) -> Result<OwnedFd, Error> {
    let mut marker = [0; 2];
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(2))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let report = recvmsg(
        &*stream,
        &mut [IoSliceMut::new(&mut marker)],
        &mut control,
        RecvFlags::CMSG_CLOEXEC,
    )
    .map_err(std::io::Error::from)?;
    if report.bytes != 1 || marker[0] != b'L' || report.flags.contains(ReturnFlags::CTRUNC) {
        return Err(Error::LaunchProtocol("invalid repository lock transfer"));
    }
    let mut lock = None;
    for message in control.drain() {
        match message {
            RecvAncillaryMessage::ScmRights(descriptors) => {
                for descriptor in descriptors {
                    if lock.replace(descriptor).is_some() {
                        return Err(Error::LaunchProtocol(
                            "multiple repository lock descriptors",
                        ));
                    }
                }
            }
            _ => {
                return Err(Error::LaunchProtocol(
                    "unexpected repository lock control message",
                ))
            }
        }
    }
    let lock = lock.ok_or(Error::LaunchProtocol("repository lock descriptor missing"))?;
    rustix::io::fcntl_setfd(&lock, rustix::io::FdFlags::empty()).map_err(std::io::Error::from)?;
    stream.write_all(b"L")?;
    Ok(lock)
}

#[cfg(test)]
#[path = "lock_fd_tests.rs"]
mod tests;
