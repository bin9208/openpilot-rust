use super::*;
use openpilot_msgq::{Publisher, Subscriber};
const NAMES: [&str; 2] = ["logMessage", "carState"];
fn capacity(name: &str) -> Result<usize> {
    Ok(openpilot_messaging::services::lookup(name)
        .ok_or("unknown bridge service")?
        .queue_size)
}
fn subscriber(context: &zmq::Context, name: &str) -> Result<zmq::Socket> {
    let socket = context.socket(zmq::SUB)?;
    socket.set_linger(0)?;
    socket.set_rcvtimeo(5000)?;
    socket.set_subscribe(b"")?;
    socket.connect(&format!("tcp://127.0.0.1:{}", port(name)))?;
    Ok(socket)
}
fn receive(socket: &zmq::Socket, expected: &[u8]) -> Result<()> {
    if socket.recv_bytes(0)? != expected {
        return Err("bridge wire packet mismatch".into());
    }
    Ok(())
}
pub(super) fn outgoing(
    binary: &Path,
    output: &Path,
    namespace: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    let wire = zmq::Context::new();
    let mut process = Process::start(binary, &[], output)?;
    std::thread::sleep(Duration::from_millis(250));
    if !process.alive()? || fs::read_dir(namespace)?.next().is_some() {
        return Err("bridge opened queues before subscription".into());
    }
    let mut publishers = NAMES
        .iter()
        .map(|name| Ok(Publisher::for_runtime(name, capacity(name)?)?))
        .collect::<Result<Vec<_>>>()?;
    let first = subscriber(&wire, NAMES[0])?;
    let second = subscriber(&wire, NAMES[1])?;
    wait(cancel, || {
        Ok(process.mapped(NAMES[0])? && process.mapped(NAMES[1])?)
    })?;
    std::thread::sleep(Duration::from_millis(150));
    let burst = packets("outgoing")?;
    for packet in &burst {
        check(cancel)?;
        for publisher in &mut publishers {
            publisher.send(packet)?;
        }
    }
    for socket in [&first, &second] {
        for packet in &burst {
            check(cancel)?;
            receive(socket, packet)?;
        }
    }
    drop(second);
    wait(cancel, || Ok(!process.mapped(NAMES[1])?))?;
    publishers[0].send(b"still-connected")?;
    receive(&first, b"still-connected")?;
    let reconnected = subscriber(&wire, NAMES[1])?;
    wait(cancel, || process.mapped(NAMES[1]))?;
    std::thread::sleep(Duration::from_millis(150));
    publishers[1].send(b"reconnected")?;
    receive(&reconnected, b"reconnected")?;
    drop(reconnected);
    drop(first);
    wait(cancel, || {
        Ok(!process.mapped(NAMES[0])? && !process.mapped(NAMES[1])?)
    })?;
    if !process.alive()? {
        return Err("bridge died after disconnect".into());
    }
    drop(publishers);
    process.shutdown()
}
pub(super) fn incoming(
    binary: &Path,
    output: &Path,
    namespace: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    let wire = zmq::Context::new();
    let publisher = wire.socket(zmq::PUB)?;
    publisher.set_linger(0)?;
    publisher.bind(&format!("tcp://127.0.0.1:{}", port(NAMES[0])))?;
    let mut process = Process::start(binary, &["127.0.0.1", "prefix-logMessage-suffix"], output)?;
    wait(cancel, || {
        if !process.alive()? {
            return Err("bridge died before incoming queue".into());
        }
        Ok(namespace.join(NAMES[0]).exists())
    })?;
    if namespace.join(NAMES[1]).exists() {
        return Err("bridge ignored substring whitelist".into());
    }
    let mut subscriber = Subscriber::for_runtime(NAMES[0], false, capacity(NAMES[0])?)?;
    std::thread::sleep(Duration::from_millis(400));
    let burst = packets("incoming")?;
    for packet in &burst {
        check(cancel)?;
        publisher.send(packet.as_slice(), 0)?;
    }
    for packet in &burst {
        check(cancel)?;
        if subscriber.receive(Duration::from_secs(5))?.as_deref() != Some(packet.as_slice()) {
            return Err("bridge incoming packet mismatch".into());
        }
    }
    drop(subscriber);
    if !process.alive()? {
        return Err("bridge died after incoming burst".into());
    }
    process.shutdown()
}
