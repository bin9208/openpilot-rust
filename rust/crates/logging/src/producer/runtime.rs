//! Process-retained Python-compatible producer; no background sender thread.
use super::{Connection, Delivery, Factory};
use crate::{
    record::{Level, Record},
    site::Site,
    Error,
};
use std::sync::{Mutex, OnceLock};

struct RuntimeProducer {
    factory: Factory,
    connection: Connection,
}
impl RuntimeProducer {
    fn emit(&mut self, site: Site, record: Record) -> Result<Delivery, Error> {
        // This local facade lives and formats on the caller's thread. Only
        // Factory and the Send socket are retained in the synchronized state.
        let logger = self.factory.logger();
        let Some(packet) = logger.prepare(site, record)? else {
            return Ok(Delivery::Filtered);
        };
        self.connection.send(&self.factory.endpoint, &packet)
    }
}
impl Drop for RuntimeProducer {
    fn drop(&mut self) {
        self.connection.release();
    }
}
static RUNTIME: OnceLock<Mutex<Option<RuntimeProducer>>> = OnceLock::new();

/// Emit with the process-global Python-compatible handler lifetime.
///
/// The first successful emission selects the runtime endpoint and inherited
/// global context. Records retain the calling thread's metadata and use the
/// ordinary formatter/nonblocking transport. Dropping an emitting thread does
/// not close queued records. No thread-local bindings are accepted by this API.
///
/// The socket is Send (provided by zmq), and the mutex serializes its complete
/// connect/send/close lifecycle. Logger's Rc context remains neither Send nor
/// Sync. Controlled fork is supported only with one Rust application thread and
/// no held logging locks, as in the existing fork fixture; inherited libzmq
/// handles are forgotten by the common PID guard before reconnect/close.
pub fn emit_runtime(site: Site, record: Record) -> Result<Delivery, Error> {
    if record.level < Level::Debug {
        return Ok(Delivery::Filtered);
    }
    with_runtime(Factory::for_runtime, |producer| producer.emit(site, record))
}

// Facades have already formatted their own local/global context and caller
// metadata. Only mechanical transport is shared with emit_runtime; never retain
// the Rc-backed Logger or merge one facade's local bindings into another.
pub(super) fn send_prepared(factory: &Factory, packet: &[u8]) -> Result<Delivery, Error> {
    with_runtime(
        || Ok(factory.clone()),
        |producer| producer.connection.send(&producer.factory.endpoint, packet),
    )
}
fn with_runtime(
    factory: impl FnOnce() -> Result<Factory, Error>,
    emit: impl FnOnce(&mut RuntimeProducer) -> Result<Delivery, Error>,
) -> Result<Delivery, Error> {
    let mut state = RUNTIME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| Error::Contract("runtime logger lock poisoned"))?;
    if let Some(producer) = &mut *state {
        return emit(producer);
    }
    let mut producer = RuntimeProducer {
        factory: factory()?,
        connection: Connection::Disconnected,
    };
    let delivery = emit(&mut producer)?;
    *state = Some(producer);
    Ok(delivery)
}

/// Close the process-retained connection after the final diagnostic.
///
/// This is idempotent and does not initialize unused state. An initialized
/// connection stays closed (ENOTSOCK on later unfiltered emission in this PID),
/// matching ordinary Logger::close. Rust statics do not run exit destructors;
/// outer executable boundaries must retain RuntimeLogScope or call this API.
pub fn close_runtime() -> Result<(), Error> {
    let Some(state) = RUNTIME.get() else {
        return Ok(());
    };
    let mut state = state
        .lock()
        .map_err(|_| Error::Contract("runtime logger lock poisoned"))?;
    if let Some(producer) = &mut *state {
        producer.connection.close();
    }
    Ok(())
}

/// Process-root shutdown guard. Retain only at the outermost executable/runtime
/// boundary, until all emitting threads have joined and final diagnostics end.
/// Do not place it in individual helpers, short-lived threads or nested scopes:
/// dropping any instance closes the one process-retained runtime transport.
/// An unused guard does not initialize a Factory, socket or logging context.
#[must_use = "retain this guard only at the outermost process boundary until final diagnostics"]
pub struct RuntimeLogScope;
impl Drop for RuntimeLogScope {
    fn drop(&mut self) {
        if let Err(error) = close_runtime() {
            eprintln!("runtime log shutdown: {error}");
        }
    }
}
