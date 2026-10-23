//! Native counterpart of `openpilot/tools/lib/sanitizer.py` for ordinary Cereal
//! Event readers. No file, route, device, logger or provider operations.
//!
//! Text inputs are valid UTF-8 (the Cereal Text boundary). Python-only lone
//! surrogate strings and arbitrary duck-typed objects are outside this API.
use capnp::message::{Builder, HeapAllocator};
use openpilot_cereal::log_capnp::event;

/// The source's exact names and order; no additional service is admitted.
pub const PRESERVE_SERVICES: [&str; 4] =
    ["can", "carParams", "pandaStates", "pandaStateDEPRECATED"];

/// An arena that owns its Event without a self-referential reader or unsafe code.
/// A reader returned by `as_reader` cannot outlive this owner.
pub struct OwnedEvent {
    message: Box<Builder<HeapAllocator>>,
}

impl OwnedEvent {
    /// Wrap an existing native arena without cloning or eagerly reading fields.
    pub fn new(message: Builder<HeapAllocator>) -> Self {
        Self {
            message: Box::new(message),
        }
    }

    pub fn as_reader(&self) -> capnp::Result<event::Reader<'_>> {
        self.message.get_root_as_reader()
    }
}

/// Non-carParams retains the original borrowed reader or owned arena unchanged.
/// For carParams, the result is a new Owned arena independent of the input.
/// Extract that OwnedEvent to retain it beyond an input reader's lifetime.
pub enum LogMessage<'a> {
    Borrowed(event::Reader<'a>),
    Owned(OwnedEvent),
}

impl LogMessage<'_> {
    pub fn as_reader(&self) -> capnp::Result<event::Reader<'_>> {
        match self {
            Self::Borrowed(reader) => Ok(*reader),
            Self::Owned(message) => message.as_reader(),
        }
    }
}

/// Python `vin[:-6] + "X" * 6`: remove Unicode code points, not UTF-8 bytes
/// or grapheme clusters. Empty and shorter-than-six inputs still produce six Xs.
pub fn sanitize_vin(vin: &str) -> String {
    let keep = vin.chars().count().saturating_sub(6);
    let end = vin
        .char_indices()
        .nth(keep)
        .map_or(vin.len(), |(offset, _)| offset);
    let mut sanitized = vin[..end].to_owned();
    sanitized.push_str("XXXXXX");
    sanitized
}

/// Source order: inspect union, deep-copy the entire Event only for carParams,
/// then read/redact carVin and return a reader backed by that new arena.
/// `set_root` is noncanonical and copies actual wire struct sizes and pointers,
/// including unknown Event/CarParams fields, rather than rebuilding known fields.
/// Invalid copied pointers, unknown union tags and invalid VIN UTF-8 are errors;
/// no lossy decoding, skipping or partial sanitized output is used.
pub fn sanitize_msg<'a>(message: LogMessage<'a>) -> capnp::Result<LogMessage<'a>> {
    if service(&message)? != "carParams" {
        return Ok(message);
    }
    let mut copy = Builder::new_default();
    copy.set_root(message.as_reader()?)?;
    let replacement = match copy.get_root_as_reader::<event::Reader<'_>>()?.which()? {
        event::CarParams(params) => sanitize_vin(params?.get_car_vin()?.to_str()?),
        _ => {
            return Err(capnp::Error::failed(
                "copied Event changed its union".into(),
            ))
        }
    };
    match copy.get_root::<event::Builder<'_>>()?.which()? {
        event::CarParams(params) => params?.set_car_vin(replacement.as_str()),
        _ => {
            return Err(capnp::Error::failed(
                "copied Event changed its union".into(),
            ))
        }
    }
    Ok(LogMessage::Owned(OwnedEvent::new(copy)))
}

fn service(message: &LogMessage<'_>) -> capnp::Result<&'static str> {
    // Generated Event::which() calls a payload getter while constructing Which.
    // Dynamic which() reads only the tag, matching Python and avoiding hidden
    // payload validation/traversal-limit consumption before filtering/cloning.
    let capnp::dynamic_value::Reader::Struct(root) = message.as_reader()?.into() else {
        return Err(capnp::Error::failed("Event is not a struct".into()));
    };
    let field = root
        .which()?
        .ok_or_else(|| capnp::Error::failed("unknown Event union discriminant".into()))?;
    Ok(field.get_proto().get_name()?.to_str()?)
}

/// Lazy filter, then map, in input order. `into_iter` happens on construction;
/// neither next() nor message inspection happens until the consumer requests it.
/// Upstream errors keep their type/value. A Cereal error occupies the failing
/// next() only; the adapter does not fuse, prefetch, swallow it or terminate the
/// upstream iterator. A source that itself stops after an error still does so.
/// Callers with infallible readers can supply `map(Ok::<_, capnp::Error>)`.
pub fn sanitize<'a, I, E>(messages: I) -> impl Iterator<Item = Result<LogMessage<'a>, E>>
where
    I: IntoIterator<Item = Result<LogMessage<'a>, E>>,
    E: From<capnp::Error>,
{
    let mut messages = messages.into_iter();
    std::iter::from_fn(move || loop {
        let message = match messages.next()? {
            Ok(message) => message,
            Err(error) => return Some(Err(error)),
        };
        match service(&message) {
            Ok(name) if PRESERVE_SERVICES.contains(&name) => {
                return Some(sanitize_msg(message).map_err(E::from));
            }
            Ok(_) => continue,
            Err(error) => return Some(Err(E::from(error))),
        }
    })
}

#[cfg(test)]
#[path = "route_sanitizer_tests.rs"]
mod tests;
