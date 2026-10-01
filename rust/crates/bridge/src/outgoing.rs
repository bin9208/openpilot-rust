use crate::{port, Error};
use openpilot_messaging::services::Service;
use openpilot_msgq::{MultiSubscriber, Subscription};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

struct Pair<'a> {
    service: &'a Service,
    publisher: zmq::Socket,
    monitor: zmq::Socket,
    clients: usize,
}

pub fn run(services: &[&Service], stop: &AtomicBool) -> Result<(), Error> {
    let context = zmq::Context::new();
    let mut pairs = Vec::with_capacity(services.len());
    for (index, service) in services.iter().enumerate() {
        let publisher = context.socket(zmq::PUB)?;
        publisher.bind(&format!("tcp://*:{}", port(service.name)))?;
        let address = format!("inproc://op-bridge-monitor-{index}");
        publisher.monitor(
            &address,
            i32::from(
                zmq::SocketEvent::ACCEPTED.to_raw() | zmq::SocketEvent::DISCONNECTED.to_raw(),
            ),
        )?;
        let monitor = context.socket(zmq::PAIR)?;
        monitor.connect(&address)?;
        pairs.push(Pair {
            service,
            publisher,
            monitor,
            clients: 0,
        });
    }
    let specs: Vec<_> = services
        .iter()
        .map(|service| Subscription {
            endpoint: service.name,
            capacity: service.queue_size,
            polled: true,
        })
        .collect();
    let mut queues = MultiSubscriber::lazy_for_runtime(&specs)?;
    while !stop.load(Ordering::Relaxed) {
        let active = pairs.iter().any(|pair| pair.clients > 0);
        monitor(&mut pairs, &mut queues, if active { 0 } else { 1000 })?;
        if pairs.iter().all(|pair| pair.clients == 0) {
            continue;
        }
        for index in queues.poll_ready(Duration::from_millis(100))? {
            for _ in 0..50 {
                let Some(message) = queues.receive_one(index)? else {
                    break;
                };
                while let Err(zmq::Error::EINTR) =
                    pairs[index].publisher.send(&message, zmq::DONTWAIT)
                {}
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn monitor(
    pairs: &mut [Pair<'_>],
    queues: &mut MultiSubscriber,
    timeout: i64,
) -> Result<(), Error> {
    let ready = {
        let mut polls: Vec<_> = pairs
            .iter()
            .map(|pair| pair.monitor.as_poll_item(zmq::POLLIN))
            .collect();
        match zmq::poll(&mut polls, timeout) {
            Err(zmq::Error::EINTR) => {
                thread::sleep(Duration::from_millis(200));
                return Ok(());
            }
            Err(_) => return Ok(()),
            Ok(_) => (),
        }
        polls
            .iter()
            .enumerate()
            .filter_map(|(index, poll)| poll.is_readable().then_some(index))
            .collect::<Vec<_>>()
    };
    for index in ready {
        let pair = &mut pairs[index];
        let frame = match pair.monitor.recv_bytes(0) {
            Ok(frame) if !frame.is_empty() => frame,
            Ok(_) | Err(_) => continue,
        };
        let address = pair.monitor.recv_bytes(0)?;
        if address.is_empty() {
            continue;
        }
        let event = u16::from_ne_bytes(
            frame
                .get(..2)
                .ok_or(Error::Monitor)?
                .try_into()
                .map_err(|_| Error::Monitor)?,
        );
        if event & zmq::SocketEvent::ACCEPTED.to_raw() != 0 {
            println!("socket [{}] connected", pair.service.name);
            pair.clients += 1;
            if pair.clients == 1 {
                queues.set_active(index, true)?;
            }
        } else if event & zmq::SocketEvent::DISCONNECTED.to_raw() != 0 {
            println!("socket [{}] disconnected", pair.service.name);
            pair.clients = pair.clients.saturating_sub(1);
            if pair.clients == 0 {
                queues.set_active(index, false)?;
            }
        }
    }
    Ok(())
}
