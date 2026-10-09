use crate::{
    queue::{Kind, Namespace, PublisherMode, Queue},
    vision_memory::Buffer,
    vision_socket, vision_wire, Error, VisionLayout, VisionMetadata, VisionStream,
};
use std::{
    cell::RefCell,
    fs::File,
    io::Read,
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd},
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

#[cfg(feature = "webcam-inactive-stream")]
mod inactive;
#[cfg(feature = "webcam-inactive-stream")]
mod unaligned;
#[cfg(feature = "webcam-inactive-stream")]
pub use unaligned::UnalignedWebcamImage;

struct Stream {
    publisher: Queue,
    buffers: Vec<Rc<Buffer>>,
    #[cfg(feature = "webcam-inactive-stream")]
    webcam_files: Option<inactive::FileStorage>,
}

struct Listener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("VisionIPC listener panicked");
            }
        }
    }
}

struct State {
    listener: Option<Listener>,
    streams: [Option<Stream>; 4],
    name: String,
    path: PathBuf,
    server_id: u64,
}

pub struct VisionServer {
    owner: Rc<RefCell<State>>,
}

pub struct VisionImage {
    owner: Rc<RefCell<State>>,
    buffer: Rc<Buffer>,
}

struct Transfer {
    payload: Vec<u8>,
    descriptors: Vec<OwnedFd>,
}

fn listen(socket: OwnedFd, transfers: [Option<Transfer>; 4], stop: Arc<AtomicBool>, path: PathBuf) {
    let run = || -> Result<(), Error> {
        while vision_socket::wait(socket.as_fd(), &stop)? {
            let client = match vision_socket::accept(socket.as_fd()) {
                Err(Error::Io(_, error)) if error.kind() == std::io::ErrorKind::Interrupted => {
                    continue
                }
                value => value?,
            };
            if !vision_socket::wait(client.as_fd(), &stop)? {
                break;
            }
            let request = match vision_socket::receive(client.as_fd(), 4) {
                Ok((request, descriptors)) if request.len() == 4 && descriptors.is_empty() => {
                    request
                }
                _ => continue,
            };
            let request = i32::from_le_bytes(
                request
                    .try_into()
                    .map_err(|_| Error::Corrupt("invalid VisionIPC stream request"))?,
            );
            let result = if request == 4 {
                let payload: Vec<_> = transfers
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| value.is_some())
                    .flat_map(|(index, _)| (index as i32).to_le_bytes())
                    .collect();
                vision_socket::send(client.as_fd(), &payload, &[])
            } else {
                let Some(transfer) = usize::try_from(request)
                    .ok()
                    .and_then(|index| transfers.get(index))
                    .and_then(Option::as_ref)
                else {
                    continue;
                };
                vision_socket::send(client.as_fd(), &transfer.payload, &transfer.descriptors)
            };
            if let Err(error) = result {
                eprintln!("{error}");
            }
        }
        Ok(())
    };
    if let Err(error) = run() {
        eprintln!("{error}");
    }
    drop(socket);
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("unlink VisionIPC socket: {error}");
        }
    }
}

impl VisionServer {
    pub fn new(name: &str) -> Result<Self, Error> {
        let path = vision_socket::path(name)?;
        let mut id = [0; 8];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut id))
            .map_err(|error| Error::Io("read VisionIPC server entropy", error))?;
        Ok(Self {
            owner: Rc::new(RefCell::new(State {
                listener: None,
                streams: std::array::from_fn(|_| None),
                name: name.to_owned(),
                path,
                server_id: u64::from_ne_bytes(id),
            })),
        })
    }

    pub fn create_stream(
        &self,
        stream: VisionStream,
        count: usize,
        layout: VisionLayout,
    ) -> Result<Vec<VisionImage>, Error> {
        let mut state = self.owner.borrow_mut();
        let stream_index = stream.native() as usize;
        if state.listener.is_some()
            || state.streams[stream_index].is_some()
            || count == 0
            || count >= vision_wire::MAX_FDS
        {
            return Err(Error::Invalid("invalid VisionIPC stream creation"));
        }
        let mapped_length = layout
            .len
            .checked_add(8)
            .ok_or(Error::Invalid("VisionIPC buffer length overflow"))?;
        layout.validate(mapped_length)?;
        let mut buffers = Vec::with_capacity(count);
        for index in 0..count {
            buffers.push(Rc::new(Buffer::allocate(vision_wire::Buffer {
                layout,
                mapped_length,
                server_id: state.server_id,
                index,
                stream,
            })?));
        }
        let publisher = Queue::open(
            &format!("visionipc_{}_{}", state.name, stream.native()),
            Kind::Publisher(PublisherMode::Original),
            1024 * 1024,
            Namespace::Runtime,
        )?;
        let images = buffers
            .iter()
            .map(|buffer| VisionImage {
                owner: Rc::clone(&self.owner),
                buffer: Rc::clone(buffer),
            })
            .collect();
        state.streams[stream_index] = Some(Stream {
            publisher,
            buffers,
            #[cfg(feature = "webcam-inactive-stream")]
            webcam_files: None,
        });
        Ok(images)
    }

    pub fn start_listener(&self) -> Result<(), Error> {
        let mut state = self.owner.borrow_mut();
        if state.listener.is_some() {
            return Err(Error::Invalid("VisionIPC listener already started"));
        }
        let mut transfers = std::array::from_fn(|_| None);
        for (target, stream) in transfers.iter_mut().zip(&state.streams) {
            let Some(stream) = stream else { continue };
            let mut transfer = Transfer {
                payload: Vec::new(),
                descriptors: Vec::new(),
            };
            for buffer in &stream.buffers {
                transfer
                    .payload
                    .extend_from_slice(&vision_wire::encode_buffer(
                        buffer.wire,
                        buffer.mapping.address(),
                        buffer.fd.as_raw_fd(),
                    )?);
                transfer
                    .descriptors
                    .push(buffer.fd.as_fd().try_clone_to_owned().map_err(|error| {
                        Error::Io("duplicate VisionIPC listener descriptor", error)
                    })?);
            }
            #[cfg(feature = "webcam-inactive-stream")]
            if let Some(files) = &stream.webcam_files {
                files.transfer(&mut transfer, state.server_id)?;
            }
            *target = Some(transfer);
        }
        let socket = vision_socket::bind(&state.path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let path = state.path.clone();
        let thread = std::thread::Builder::new()
            .name("visionipc".to_owned())
            .spawn(move || listen(socket, transfers, thread_stop, path))
            .map_err(|error| {
                let _ = std::fs::remove_file(&state.path);
                Error::Io("start VisionIPC listener", error)
            })?;
        state.listener = Some(Listener {
            stop,
            thread: Some(thread),
        });
        Ok(())
    }
}

impl VisionImage {
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.buffer.mapping.write(offset, bytes)
    }
    pub fn copy_into(&self, bytes: &mut [u8]) -> Result<(), Error> {
        self.buffer.mapping.copy_into(bytes)
    }

    pub fn publish(&self, metadata: VisionMetadata) -> Result<(), Error> {
        let mut state = self.owner.borrow_mut();
        self.buffer
            .mapping
            .set_frame_id(u64::from(metadata.frame_id))?;
        self.buffer.sync(true);
        let packet = vision_wire::encode_packet(state.server_id, self.buffer.wire.index, metadata)?;
        let stream = state.streams[self.buffer.wire.stream.native() as usize]
            .as_mut()
            .ok_or(Error::Invalid("VisionIPC image stream is absent"))?;
        stream.publisher.send(&packet)
    }
}

impl AsFd for VisionImage {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.buffer.fd.as_fd()
    }
}

pub struct RawVisionImage {
    buffer: Buffer,
}

impl RawVisionImage {
    pub fn new(length: usize) -> Result<Self, Error> {
        let mapped_length = length
            .checked_add(8)
            .ok_or(Error::Invalid("VisionIPC length overflow"))?;
        let layout = VisionLayout {
            width: 0,
            height: 0,
            stride: 0,
            uv_offset: 0,
            len: length,
        };
        Ok(Self {
            buffer: Buffer::allocate(vision_wire::Buffer {
                layout,
                mapped_length,
                server_id: 0,
                index: 0,
                stream: VisionStream::Road,
            })?,
        })
    }
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        self.buffer.mapping.write(offset, bytes)
    }
    pub fn copy_into(&self, bytes: &mut [u8]) -> Result<(), Error> {
        self.buffer.mapping.copy_into(bytes)
    }
}

impl AsFd for RawVisionImage {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.buffer.fd.as_fd()
    }
}
