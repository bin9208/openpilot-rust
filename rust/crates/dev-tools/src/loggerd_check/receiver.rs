//! R4 contract: bind atomically, authenticate the listener, transfer one FD.
//! The supervisor decides when all producers are gone; dropping a Receiver
//! alone deliberately does not authorize unlinking its shared diagnostic leaf.
use super::{require, save, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    os::{
        fd::{AsRawFd, IntoRawFd},
        unix::{fs::MetadataExt, net::UnixListener},
    },
    path::{Path, PathBuf},
};
#[path = "../route_upload_check/lifecycle/socket_identity.rs"]
mod socket_identity;

pub(super) struct Receiver {
    // Drop order closes libzmq's duplicate and context before our listener.
    socket: zmq::Socket,
    context: zmq::Context,
    listener: UnixListener,
    held: File,
    path: PathBuf,
    root: PathBuf,
}
impl Receiver {
    pub fn bind(prefix: &str, root: &Path) -> Result<Self> {
        let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        // Unlike libzmq's ordinary IPC bind this never unlinks an occupied path.
        let listener = UnixListener::bind(&path)?;
        let held = File::from(rustix::fs::open(
            &path,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?);
        require(
            socket_identity::matches(&listener, &held)?,
            "logger diagnostic listener/VFS identity unproved; preserved",
        )?;
        let result = (|| {
            listener.set_nonblocking(true)?;
            let context = zmq::Context::new();
            let mut socket = context.socket(zmq::PULL)?;
            socket.set_linger(0)?;
            socket.set_rcvhwm(10000)?;
            let fd = listener.try_clone()?;
            let number = fd.as_raw_fd();
            // SAFETY: live socket, exact int-sized option. Same libzmq 4.3.4
            // ownership contract as native_logging_check::transport: a failed
            // bind retains the clone, a successful bind transfers it once.
            // This socket is never reused after failure.
            let status = unsafe {
                zmq_sys::zmq_setsockopt(
                    socket.as_mut_ptr(),
                    zmq_sys::ZMQ_USE_FD as i32,
                    (&number as *const i32).cast(),
                    std::mem::size_of_val(&number),
                )
            };
            require(status == 0, "logger prebound FD option rejected")?;
            socket.bind(&format!("ipc://{}", path.display()))?;
            let _transferred = fd.into_raw_fd();
            Ok::<_, Box<dyn std::error::Error>>((socket, context))
        })();
        let (socket, context) = match result {
            Ok(value) => value,
            Err(error) => {
                // No worker has started. Only the authenticated original leaf
                // may be cleaned; a replacement remains untouched.
                let _ = remove_leaf(&path, &held);
                return Err(error);
            }
        };
        Ok(Self {
            socket,
            context,
            listener,
            held,
            path,
            root: root.into(),
        })
    }
    pub fn receipt(&self) -> Result<Value> {
        let metadata = self.held.metadata()?;
        Ok(
            json!({"path":self.path,"dev":metadata.dev(),"ino":metadata.ino(),
            "socket_cookie":rustix::net::sockopt::socket_cookie(&self.listener)?,
            "ownership":"prebound listener/SO_COOKIE/UNIX_DIAG_VFS; ZMQ_USE_FD"}),
        )
    }
    pub fn drain(&self) -> Result<()> {
        // Preserve each raw packet before decoding, including a malformed one.
        let path = self.root.join("diagnostic-packets");
        fs::create_dir_all(&path)?;
        let mut rows = Vec::new();
        let mut index = Vec::new();
        while self.socket.poll(zmq::POLLIN, 20)? > 0 {
            let packet = self.socket.recv_bytes(0)?;
            let name = format!("{:04}.packet", rows.len());
            fs::write(path.join(&name), &packet)?;
            index.push(json!({"path":name,"sha256":super::hash(&packet)}));
            save(&path.join("index.json"), &json!(index))?;
            require(
                packet.len() > 1 && rows.len() < 10000,
                "diagnostic packet shape/count invalid",
            )?;
            let row: Value = serde_json::from_slice(&packet[1..])?;
            require(
                row["levelnum"].as_u64() == Some(u64::from(packet[0])),
                "diagnostic wire level prefix differs",
            )?;
            rows.push(row);
        }
        save(&path.join("index.json"), &json!(index))?;
        save(&self.root.join("diagnostics.json"), &json!(rows))
    }
    pub fn cleanup(self) -> Result<Value> {
        let Self {
            socket,
            context,
            listener,
            held,
            path,
            ..
        } = self;
        drop(socket);
        drop(context);
        drop(listener);
        remove_leaf(&path, &held)
    }
    pub fn preserve(self) {
        // No unlinking Drop: even closing libzmq leaves the shared leaf intact.
        drop(self);
    }
}
fn remove_leaf(path: &Path, held: &File) -> Result<Value> {
    let original = held.metadata()?;
    let status = match fs::symlink_metadata(path) {
        Ok(current) if (current.dev(), current.ino()) == (original.dev(), original.ino()) => {
            fs::remove_file(path)?;
            "removed-owned"
        }
        Ok(_) => "preserved-replacement",
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "already-absent",
        Err(e) => return Err(e.into()),
    };
    Ok(
        json!({"kind":"diagnostic","path":path,"dev":original.dev(),"ino":original.ino(),"status":status}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn occupied_and_replaced_diagnostic_leaves_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let prefix = format!(
            "loggerd-test-{}",
            root.path().file_name().unwrap().to_string_lossy()
        );
        let path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
        fs::write(&path, b"foreign").unwrap();
        assert!(Receiver::bind(&prefix, root.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        fs::remove_file(&path).unwrap();
        symlink(root.path().join("missing"), &path).unwrap();
        assert!(Receiver::bind(&prefix, root.path()).is_err());
        assert!(fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_file(&path).unwrap();
        let receiver = Receiver::bind(&prefix, root.path()).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement").unwrap();
        assert_eq!(
            receiver.cleanup().unwrap()["status"],
            "preserved-replacement"
        );
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        fs::remove_file(&path).unwrap();
        let receiver = Receiver::bind(&prefix, root.path()).unwrap();
        assert_eq!(receiver.cleanup().unwrap()["status"], "removed-owned");
        assert!(!path.exists());
    }
}
