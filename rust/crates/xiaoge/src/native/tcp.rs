use super::Error;
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

pub type Clients = Arc<Mutex<Vec<(SocketAddr, Arc<TcpStream>)>>>;

pub fn start(
    address: SocketAddrV4,
    clients: Clients,
    running: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, Error> {
    Ok(thread::Builder::new()
        .name("xiaoge-tcp".to_owned())
        .spawn(move || {
            if let Err(error) = accept(address, &clients, &running) {
                if running.load(Ordering::Acquire) {
                    eprintln!("Xiaoge data TCP server error: {error}");
                }
            }
        })?)
}

fn accept(
    address: SocketAddrV4,
    clients: &Clients,
    running: &Arc<AtomicBool>,
) -> Result<(), Error> {
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&SocketAddr::V4(address).into())?;
    socket.listen(5)?;
    let listener: TcpListener = socket.into();
    println!(
        "Xiaoge data TCP server listening on port {}",
        address.port()
    );
    while running.load(Ordering::Acquire) {
        let (stream, address) = listener.accept()?;
        if !running.load(Ordering::Acquire) {
            break;
        }
        let clients = Arc::clone(clients);
        let running = Arc::clone(running);
        thread::Builder::new()
            .name("xiaoge-client".to_owned())
            .spawn(move || {
                if let Err(error) = client(stream, address, &clients, &running) {
                    eprintln!("Xiaoge client thread failed: {error}");
                }
            })?;
    }
    Ok(())
}

fn client(
    stream: TcpStream,
    address: SocketAddr,
    clients: &Clients,
    running: &AtomicBool,
) -> Result<(), Error> {
    println!("Client connected from {address}");
    let stream = Arc::new(stream);
    clients
        .lock()
        .map_err(|_| Error::Contract("TCP client registry poisoned"))?
        .push((address, Arc::clone(&stream)));
    let mut input = &*stream;
    let mut command = [0; 4];
    while running.load(Ordering::Acquire) {
        match input.read_exact(&mut command) {
            Ok(()) => {
                if u32::from_be_bytes(command) == 2 {
                    if let Err(error) = input.write_all(&0u32.to_be_bytes()) {
                        if !matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe
                                | std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        ) {
                            eprintln!("Xiaoge heartbeat write failed: {error}");
                        }
                        break;
                    }
                }
            }
            Err(error) => {
                if !matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ) {
                    eprintln!("Xiaoge client read failed: {error}");
                }
                break;
            }
        }
    }
    clients
        .lock()
        .map_err(|_| Error::Contract("TCP client registry poisoned"))?
        .retain(|(peer, _)| *peer != address);
    println!("Client {address} disconnected");
    Ok(())
}

pub fn broadcast(clients: &Clients, packet: &[u8]) -> Result<(), Error> {
    let length = u32::try_from(packet.len())
        .map_err(|_| Error::Contract("TCP packet length overflow"))?
        .to_be_bytes();
    let snapshot = clients
        .lock()
        .map_err(|_| Error::Contract("TCP client registry poisoned"))?
        .clone();
    for (address, stream) in snapshot {
        let mut writer = &*stream;
        if let Err(error) = writer
            .write_all(&length)
            .and_then(|()| writer.write_all(packet))
        {
            clients
                .lock()
                .map_err(|_| Error::Contract("TCP client registry poisoned"))?
                .retain(|(peer, _)| *peer != address);
            if let Err(close) = stream.shutdown(Shutdown::Both) {
                if close.kind() != std::io::ErrorKind::NotConnected {
                    eprintln!("Xiaoge failed client close: {error}; {close}");
                }
            }
        }
    }
    Ok(())
}

pub fn shutdown(clients: &Clients) -> Result<(), Error> {
    let mut clients = clients
        .lock()
        .map_err(|_| Error::Contract("TCP client registry poisoned"))?;
    for (_, stream) in clients.drain(..) {
        if let Err(error) = stream.shutdown(Shutdown::Both) {
            if error.kind() != std::io::ErrorKind::NotConnected {
                eprintln!("Xiaoge client shutdown failed: {error}");
            }
        }
    }
    Ok(())
}
