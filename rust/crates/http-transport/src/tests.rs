use super::*;
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};
use ureq::unversioned::transport::LazyBuffers;

#[derive(Debug)]
struct Input {
    buffers: LazyBuffers,
    replies: VecDeque<(Duration, Result<bool, io::ErrorKind>)>,
    deadlines: Arc<Mutex<Vec<Duration>>>,
}
impl Transport for Input {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, _: usize, _: NextTimeout) -> Result<(), ureq::Error> {
        Ok(())
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.deadlines.lock().unwrap().push(*timeout.after);
        let (delay, reply) = self.replies.pop_front().expect("unexpected extra read");
        std::thread::sleep(delay);
        reply.map_err(|kind| ureq::Error::Io(io::Error::from(kind)))
    }
    fn is_open(&mut self) -> bool {
        true
    }
}

fn transport(
    timeout: Duration,
    replies: Vec<(Duration, Result<bool, io::ErrorKind>)>,
) -> (SocketTimeout, Arc<Mutex<Vec<Duration>>>) {
    let deadlines = Arc::new(Mutex::new(Vec::new()));
    let input = Input {
        buffers: LazyBuffers::new(1024, 1024),
        replies: replies.into(),
        deadlines: Arc::clone(&deadlines),
    };
    (
        SocketTimeout {
            inner: Box::new(input),
            timeout,
        },
        deadlines,
    )
}
fn next() -> NextTimeout {
    NextTimeout {
        after: Duration::from_secs(30).into(),
        reason: ureq::Timeout::RecvBody,
    }
}

#[test]
fn interrupted_read_retries_with_remaining_deadline() {
    let timeout = Duration::from_secs(1);
    let (mut transport, deadlines) = transport(
        timeout,
        vec![
            (Duration::from_millis(2), Err(io::ErrorKind::Interrupted)),
            (Duration::ZERO, Ok(true)),
        ],
    );
    assert!(transport.await_input(next()).unwrap());
    let deadlines = deadlines.lock().unwrap();
    assert_eq!(deadlines.len(), 2);
    assert!(deadlines[1] < deadlines[0]);
    assert!(deadlines[0] <= timeout);
}

#[test]
fn interrupted_read_cannot_restart_expired_deadline() {
    let (mut transport, deadlines) = transport(
        Duration::from_millis(1),
        vec![(Duration::from_millis(5), Err(io::ErrorKind::Interrupted))],
    );
    assert!(matches!(
        transport.await_input(next()),
        Err(ureq::Error::Timeout(ureq::Timeout::RecvBody))
    ));
    assert_eq!(deadlines.lock().unwrap().len(), 1);
}

#[test]
fn other_read_errors_are_not_retried() {
    let (mut transport, deadlines) = transport(
        Duration::from_secs(1),
        vec![(Duration::ZERO, Err(io::ErrorKind::ConnectionReset))],
    );
    assert!(
        matches!(transport.await_input(next()), Err(ureq::Error::Io(error)) if error.kind() == io::ErrorKind::ConnectionReset)
    );
    assert_eq!(deadlines.lock().unwrap().len(), 1);
}

#[test]
fn bytes_arriving_after_deadline_remain_rejected() {
    let (mut transport, _) = transport(
        Duration::from_millis(1),
        vec![(Duration::from_millis(5), Ok(true))],
    );
    assert!(matches!(
        transport.await_input(next()),
        Err(ureq::Error::Timeout(ureq::Timeout::RecvBody))
    ));
}
