//! Identity of the exact descriptor consumed by a successful transfer.
use crate::TransferError;
use std::{
    fs::{self, File, Metadata},
    io,
    os::unix::fs::MetadataExt,
    path::Path,
};

pub struct UploadedSource {
    pub(crate) file: File,
    metadata: Metadata,
}
impl UploadedSource {
    /// Retain the source before reading it; a cloned descriptor can supply the
    /// transfer body while this owner is returned with its response.
    pub fn new(file: File) -> io::Result<Self> {
        let metadata = file.metadata()?;
        Ok(Self { file, metadata })
    }
    pub(crate) fn check(&self) -> Result<(), TransferError> {
        if !same(&self.metadata, &self.file.metadata()?) {
            return Err(TransferError::Contract(
                "upload source changed before marking",
            ));
        }
        Ok(())
    }
}
fn same(left: &Metadata, right: &Metadata) -> bool {
    let identity = |m: &Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    identity(left) == identity(right)
}

// An ordinary marking IO error retains the existing logging/fatal policy. A
// different source is a failed upload, not a successful transfer with a failed
// xattr. In either case no pathname-based mark can affect a replacement inode.
pub(crate) struct Marker(pub(crate) io::Result<UploadedSource>);
pub(crate) fn marker(
    path: &Path,
    initial: &Metadata,
    uploaded: Option<UploadedSource>,
) -> Result<Marker, TransferError> {
    let changed = || TransferError::Contract("upload source changed before completion");
    if !same(initial, &fs::metadata(path).map_err(|_| changed())?) {
        return Err(changed());
    }
    if let Some(source) = uploaded {
        if !same(initial, &source.metadata) || !same(&source.metadata, &source.file.metadata()?) {
            return Err(changed());
        }
        return Ok(Marker(Ok(source)));
    }
    // Empty/oversized files and HTTP 412 have no body descriptor. They need
    // only the write capability already required by user.upload, not read
    // permission. NONBLOCK prevents a swapped FIFO from stalling this open.
    let file = rustix::fs::open(
        path,
        rustix::fs::OFlags::WRONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(io::Error::from);
    if let Ok(file) = &file {
        if !same(initial, &file.metadata()?) {
            return Err(changed());
        }
    }
    Ok(Marker(file.map(|file| UploadedSource {
        file,
        metadata: initial.clone(),
    })))
}
