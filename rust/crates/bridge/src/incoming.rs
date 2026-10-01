use crate::{port, Error};
use openpilot_messaging::services::Service;
use openpilot_msgq::Publisher;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn run(services: &[&Service], ip: &str, stop: &AtomicBool) -> Result<(), Error> {
    let context = zmq::Context::new();
    let mut pairs = Vec::with_capacity(services.len());
    for service in services {
        let publisher = Publisher::for_runtime(service.name, service.queue_size)?;
        let subscriber = context.socket(zmq::SUB)?;
        subscriber.set_subscribe(b"")?;
        subscriber.set_reconnect_ivl_max(500)?;
        subscriber.connect(&format!("tcp://{ip}:{}", port(service.name)))?;
        pairs.push((subscriber, publisher));
    }
    while !stop.load(Ordering::Relaxed) {
        let ready = {
            let mut polls: Vec<_> = pairs
                .iter()
                .map(|(socket, _)| socket.as_poll_item(zmq::POLLIN))
                .collect();
            if zmq::poll(&mut polls, 100).is_err() {
                continue;
            }
            polls
                .iter()
                .enumerate()
                .filter_map(|(index, poll)| poll.is_readable().then_some(index))
                .collect::<Vec<_>>()
        };
        for index in ready {
            let (socket, publisher) = &mut pairs[index];
            if let Ok(message) = socket.recv_msg(zmq::DONTWAIT) {
                publisher.send(&message)?;
            }
        }
    }
    Ok(())
}
