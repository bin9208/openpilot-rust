//! Test-only reproduction of pyzmq's inherited-handle PID guard.
use openpilot_logging::{
    log_site,
    producer::{Delivery, Factory, Logger},
    record::{Level, Record},
};
use std::{error::Error, io};
fn acknowledgement() -> Result<(), Box<dyn Error>> {
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Err("missing peer acknowledgement".into());
    }
    Ok(())
}
fn send(logger: &mut Logger, label: &str) -> Result<(), Box<dyn Error>> {
    if logger.emit(log_site!(), Record::text(Level::Info, label.into()))? != Delivery::Sent {
        return Err("fork fixture record was dropped".into());
    }
    acknowledgement()
}
#[expect(
    unsafe_code,
    reason = "audited test-only fork/waitpid/_exit; production logging forbids unsafe"
)]
fn fork_case(logger: &mut Logger, reconnect: bool) -> Result<(), Box<dyn Error>> {
    // SAFETY: the fixture has one Rust application thread, no held Rust locks and a live
    // libzmq context. The child exercises only the supported fresh-context/PID-guard path,
    // explicitly closes or forgets its handles, then _exit avoids inherited Rust destructors.
    let child = unsafe { libc::fork() };
    if child < 0 {
        return Err(io::Error::last_os_error().into());
    }
    if child == 0 {
        let code = if reconnect {
            match send(logger, "child-record") {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("child: {error}");
                    1
                }
            }
        } else {
            0
        };
        logger.close();
        // SAFETY: terminate only the fork child after its explicit handle cleanup.
        unsafe { libc::_exit(code) };
    }
    let mut status = 0;
    // SAFETY: child is a positive PID from fork and status is a live writable integer.
    if unsafe { libc::waitpid(child, &mut status, 0) } != child {
        return Err(io::Error::last_os_error().into());
    }
    if !libc::WIFEXITED(status) || libc::WEXITSTATUS(status) != 0 {
        return Err("fork child failed".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let endpoint = std::env::args().nth(1).ok_or("endpoint required")?;
    let reconnect = std::env::args().nth(2).as_deref() != Some("drop");
    let mut logger = Factory::new(endpoint)?.logger();
    send(&mut logger, "parent-before")?;
    fork_case(&mut logger, reconnect)?;
    send(&mut logger, "parent-after")?;
    logger.close();
    Ok(())
}
