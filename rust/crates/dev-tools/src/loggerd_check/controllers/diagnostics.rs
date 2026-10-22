//! Source controller's 10,000-input accounting and original suppression schedule.
use super::super::{fixtures, peer, require, scenarios, validation, Request, Result};
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use std::{fs, io::Cursor, path::Path, time::Duration};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_counter_grammar_rejects_partial_or_nonfinite_reports() {
        for text in [
            "9999 messages, 1.00 msg/sec, 2.00 KB/sec",
            "10000 messages, NaN msg/sec, 2.00 KB/sec",
            "10000 messages, 1e3 msg/sec, 2.00 KB/sec",
            "10000 messages, 1.00 msg/sec, 2.00 KB/sec trailing",
        ] {
            assert!(counter(text).is_err(), "{text}");
        }
        // Grammar comes directly from the original fullmatch expression.
        assert_eq!(
            counter("10000 messages, 12.50 msg/sec, 0.40 KB/sec").unwrap(),
            (12.5, 0.4)
        );
    }
    #[test]
    fn rounding_bound_is_the_original_two_decimal_bound() {
        assert!(accounting(10_000_000, 10.0, 10.01).is_ok());
        assert!(accounting(10_000_000, 10.0, 10.011).is_err());
        assert!(accounting(10_000_000, 0.0, 0.0).is_err());
    }
    #[test]
    fn full_event_accounting_excludes_sentinels_and_preserves_can_payload() {
        let can = fixtures::event("can", 17).unwrap();
        let mut sentinel = capnp::message::Builder::new_default();
        sentinel.init_root::<event::Builder>().init_sentinel();
        let mut raw = capnp::serialize::write_message_to_words(&sentinel);
        raw.extend_from_slice(&can);
        assert_eq!(count_bytes(&raw).unwrap(), can.len() as u64);
        raw.pop();
        assert!(count_bytes(&raw).is_err());
    }
}

fn counter_fields(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix("10000 messages, ")?;
    let (messages, bytes) = text.split_once(" msg/sec, ")?;
    let bytes = bytes.strip_suffix(" KB/sec")?;
    [messages, bytes]
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .then_some((messages, bytes))
}
fn counter(text: &str) -> Result<(f64, f64)> {
    let (messages, bytes) = counter_fields(text).ok_or("not a complete original report")?;
    Ok((messages.parse()?, bytes.parse()?))
}
fn accounting(total: u64, messages: f64, bytes: f64) -> Result<()> {
    require(
        messages.is_finite() && bytes.is_finite() && messages > 0.0 && bytes > 0.0,
        "nonpositive/nonfinite logger rate",
    )?;
    let per_message = total as f64 * 0.001 / 10_000.0;
    require(
        (bytes - messages * per_message).abs() <= 0.005 * (1.0 + per_message),
        "original byte-rate rounding bound exceeded",
    )
}
fn count_bytes(raw: &[u8]) -> Result<u64> {
    let mut input = Cursor::new(raw);
    let mut total = 0_u64;
    while input.position() < raw.len() as u64 {
        let message = capnp::serialize::read_message(&mut input, Default::default())?;
        let root = message.get_root::<event::Reader<'_>>()?;
        if matches!(root.which()?, event::InitData(_) | event::Sentinel(_)) {
            continue;
        }
        // Match pycapnp packet.as_builder().to_bytes(): deep-copy the complete
        // Event into a fresh default arena, rather than count stream padding.
        let mut copy = capnp::message::Builder::new_default();
        copy.set_root(root)?;
        total = total
            .checked_add(capnp::serialize::write_message_to_words(&copy).len() as u64)
            .ok_or("logger byte count overflow")?;
    }
    Ok(total)
}
pub(super) fn drive(request: &Request) -> Result<Value> {
    let fixtures = fixtures::Fixtures::load(&request.fixtures)?;
    let mut settings = if request.kind == "diagnostic-throughput" {
        peer::Settings::new(
            &scenarios::ENCODERS
                .into_iter()
                .chain(["can"])
                .collect::<Vec<_>>(),
        )
    } else {
        let mut settings = peer::Settings::new(&["qRoadEncodeData", "rawAudioData"]);
        settings.audio = true;
        settings
    };
    settings.runner.clone_from(&request.runner);
    let mut peer = peer::Peer::start(&request.binary, &request.output, &settings)?;
    if request.kind == "diagnostic-throughput" {
        scenarios::drive(&mut peer, "queue-restart", &fixtures)?;
        while peer.inputs.len() < 10_000 {
            peer.send("can", &fixtures::event("can", peer.inputs.len() as u32)?)?;
        }
        require(
            peer.inputs.len() == 10_000,
            "source report input count differs",
        )?;
    } else {
        for frame in 0..205 {
            send_qroad(&mut peer, fixtures.packet("qRoadEncodeData", 0)?, frame)?;
        }
        std::thread::sleep(Duration::from_millis(120));
        for frame in [205, 206] {
            send_qroad(&mut peer, fixtures.packet("qRoadEncodeData", 0)?, frame)?;
        }
        peer.send("rawAudioData", &fixtures::event("rawAudioData", 0)?)?;
        send_qroad(&mut peer, fixtures.packet("qRoadEncodeData", 0)?, 207)?;
    }
    let logs = peer.stop(rustix::process::Signal::TERM)?;
    require(
        peer.inputs.last().is_some_and(|v| v.0 == "logMessage"),
        "final source stop service differs",
    )?;
    let barrier = capnp::serialize::read_message(
        Cursor::new(&peer.inputs.last().ok_or("stop barrier absent")?.1),
        Default::default(),
    )?;
    let barrier = barrier.get_root::<event::Reader<'_>>()?;
    require(
        !barrier.get_valid()
            && matches!(barrier.which()?, event::LogMessage(v) if v?.to_str()? == "logger-qa-barrier"),
        "final source stop barrier differs",
    )?;
    Ok(json!({"segments":logs.len(),"inputs":peer.inputs.len()}))
}
fn send_qroad(peer: &mut peer::Peer, fixture: &[u8], frame: u32) -> Result<()> {
    peer.send(
        "qRoadEncodeData",
        &fixtures::encoded(fixture, "qRoadEncodeData", 0, frame, Some(true))?,
    )
}
pub(super) fn verify(
    repository: &Path,
    root: &Path,
    native: bool,
    throughput: bool,
) -> Result<Value> {
    let peer = peer::Evidence::load(root, true)?;
    let (messages, _) = validation::diagnostic_records(repository, &peer, native)?;
    if !throughput {
        require(
            messages
                .iter()
                .any(|v| v == &json!([30, "cloudlog: 2 messages suppressed"])),
            "source suppression summary absent",
        )?;
        require(
            messages
                .iter()
                .filter(|v| {
                    v[1].as_str()
                        .is_some_and(|s| s.contains("dropping frame waiting for audio"))
                })
                .count()
                == 4,
            "source audio waiting count differs",
        )?;
        return Ok(json!({"audio_waiting_warnings":4,"suppressed":2}));
    }
    throughput_report(&peer, &messages)
}
pub(super) fn throughput_report(peer: &peer::Evidence, messages: &[Value]) -> Result<Value> {
    require(
        peer.inputs.len() == 10_001,
        "10,000 inputs plus final barrier required",
    )?;
    let counters = messages
        .iter()
        .filter_map(|row| row[1].as_str())
        .filter(|text| counter_fields(text).is_some())
        .map(counter)
        .collect::<Result<Vec<_>>>()?;
    require(
        counters.len() == 1,
        "exactly one 10,000-message report required",
    )?;
    let mut total = 0_u64;
    for part in 0..peer.records()?.len() {
        total = total
            .checked_add(count_bytes(&fs::read(
                peer.segment(part).join("rlog.capnp"),
            )?)?)
            .ok_or("logger total overflow")?;
    }
    total = total
        .checked_sub(peer.inputs.last().ok_or("stop input absent")?.1.len() as u64)
        .ok_or("final barrier exceeds logged bytes")?;
    accounting(total, counters[0].0, counters[0].1)?;
    Ok(
        json!({"input_messages_at_report":10000,"logged_bytes_at_report":total,"message_rate":counters[0].0,"kilobyte_rate":counters[0].1,"rate_rounding":"two decimal places","queued_index_packets_included":true}),
    )
}
