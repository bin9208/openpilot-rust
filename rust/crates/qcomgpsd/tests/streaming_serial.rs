//! Synthetic owned PTYs only. Exercise real setup/steady/setup consumption.
use openpilot_qcomgpsd::{framing, serial::Diagnostic, setup, Error};
use serialport::SerialPort;
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

fn range_reply() -> Vec<u8> {
    let mut reply = vec![115, 0, 0, 0, 1, 0, 0, 0];
    reply.extend([0; 68]); // status + sixteen zero counts
    reply
}

#[test]
fn setup_skips_large_logs_and_accepts_large_reply_tails_across_mode_transitions() {
    let (mut master, slave) = serialport::TTYPort::pair().unwrap();
    master.set_timeout(Duration::from_secs(3)).unwrap();
    let mut diag = Diagnostic::open(Path::new(&slave.name().unwrap())).unwrap();
    let stop = AtomicBool::new(false);
    let peer = std::thread::spawn(move || {
        let mut commands = framing::Frames::default();
        for round in 0..2 {
            loop {
                let mut transfer = [0; 2048];
                let count = master.read(&mut transfer).unwrap();
                commands.extend(&transfer[..count]);
                if let Some((opcode, payload)) = commands.next_frame().unwrap() {
                    assert_eq!((opcode, payload), (115, vec![0, 0, 0, 1, 0, 0, 0]));
                    break;
                }
            }
            let mut skipped = vec![0x7d; 150_000];
            skipped[0] = 16; // setup skips logs before any log extent validation
            master.write_all(&framing::encode(&skipped)).unwrap();
            let mut reply = range_reply();
            reply.resize(150_000, 0x7e); // valid config prefix, unrestricted tail
            let mut wire = framing::encode(&reply);
            if round == 0 {
                wire.extend(framing::encode(&[99, 0x7d, 0x7e]));
            }
            master.write_all(&wire).unwrap();
        }
        master
    });
    setup::logs(&mut diag, &[], &stop).unwrap();
    assert_eq!(diag.recv_log(&stop).unwrap(), (99, vec![]));
    setup::logs(&mut diag, &[], &stop).unwrap();
    let _master = peer.join().unwrap();
}

#[test]
fn stop_interrupts_an_unterminated_oversize_stream() {
    let (mut master, slave) = serialport::TTYPort::pair().unwrap();
    master.set_timeout(Duration::from_secs(3)).unwrap();
    let mut diag = Diagnostic::open(Path::new(&slave.name().unwrap())).unwrap();
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..128 {
                master.write_all(&[0x7d; 4096]).unwrap();
            }
            stop.store(true, Ordering::Relaxed);
        });
        assert!(matches!(diag.recv(&stop), Err(Error::Stopped)));
    });
}

#[test]
fn runtime_ignores_large_nonlog_tails_but_rejects_oversize_logs_after_crc() {
    for corrupt in [false, true] {
        let (mut master, slave) = serialport::TTYPort::pair().unwrap();
        master.set_timeout(Duration::from_secs(3)).unwrap();
        let mut diag = Diagnostic::open(Path::new(&slave.name().unwrap())).unwrap();
        let stop = AtomicBool::new(false);
        let peer = std::thread::spawn(move || {
            let mut nonlog = vec![0x7e; 150_000];
            nonlog[0] = 99;
            master.write_all(&framing::encode(&nonlog)).unwrap();
            nonlog[0] = 16;
            let mut oversized = framing::encode(&nonlog);
            if corrupt {
                oversized[1] ^= 1;
            }
            oversized.extend(framing::encode(&[99]));
            master.write_all(&oversized).unwrap();
            master
        });
        assert_eq!(diag.recv_log(&stop).unwrap(), (99, vec![]));
        assert_eq!(
            diag.recv_log(&stop).unwrap_err().to_string(),
            if corrupt {
                "HDLC checksum mismatch"
            } else {
                "diagnostic log extent"
            }
        );
        assert_eq!(diag.recv_log(&stop).unwrap(), (99, vec![]));
        let _master = peer.join().unwrap();
    }
}

#[test]
fn setup_validates_entire_skipped_log_and_reply_tail_before_consumption() {
    for opcode in [16, 115] {
        let (mut master, slave) = serialport::TTYPort::pair().unwrap();
        master.set_timeout(Duration::from_secs(3)).unwrap();
        let mut diag = Diagnostic::open(Path::new(&slave.name().unwrap())).unwrap();
        let stop = AtomicBool::new(false);
        let peer = std::thread::spawn(move || {
            let mut commands = framing::Frames::default();
            loop {
                let mut transfer = [0; 2048];
                let count = master.read(&mut transfer).unwrap();
                commands.extend(&transfer[..count]);
                if commands.next_frame().unwrap().is_some() {
                    break;
                }
            }
            let mut message = range_reply();
            message[0] = opcode;
            message.resize(150_000, 0x7e);
            let mut wire = framing::encode(&message);
            // Corrupt the ignored tail, beyond retained capacity, not the header.
            wire[140_000] ^= 1;
            master.write_all(&wire).unwrap();
            master
        });
        assert_eq!(
            setup::logs(&mut diag, &[], &stop).unwrap_err().to_string(),
            "HDLC checksum mismatch"
        );
        let _master = peer.join().unwrap();
    }
}
