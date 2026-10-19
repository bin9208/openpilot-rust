//! Independent native controller for the original synthetic procLog comparison.
//! No candidate collector/encoder is linked and no golden fixture is read here.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    path_resolve::resolve_non_strict,
    seal::Result,
};
use std::{
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
pub(crate) mod inputs;
mod peer;
mod source;
mod wire;
pub use source::{advance, expected_event, make_process, prepare};
pub use wire::{compare_frame, project_event};
pub fn queue_size() -> u64 {
    10_485_760
}
/// Original int(bytes) decimal grammar, including ASCII VT/FF and underscores.
pub fn parse_queue_size(bytes: &[u8]) -> Result<i64> {
    let space = |b: &u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c');
    let start = bytes.iter().position(|b| !space(b)).unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !space(b))
        .map_or(start, |i| i + 1);
    let bytes = &bytes[start..end];
    let sign = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    if bytes.len() == sign {
        return Err("invalid queue-size integer".into());
    }
    for (index, byte) in bytes.iter().enumerate().skip(sign) {
        if byte.is_ascii_digit() {
            continue;
        }
        if *byte == b'_'
            && index > sign
            && bytes[index - 1].is_ascii_digit()
            && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
        {
            continue;
        }
        return Err("invalid queue-size integer".into());
    }
    Ok(std::str::from_utf8(
        &bytes
            .iter()
            .copied()
            .filter(|b| *b != b'_')
            .collect::<Vec<_>>(),
    )?
    .parse()?)
}
pub fn arguments(args: &[OsString], root: &Path) -> Result<Option<PathBuf>> {
    let parsed = scan_os(args, &[OptionSpec::value("--binary", false)])?;
    Ok(if parsed.help {
        None
    } else {
        Some(
            parsed
                .values
                .get("--binary")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("rust/target/debug/examples/reference_trace")),
        )
    })
}
pub fn check(root: &Path, binary: &Path, printed: &mut dyn Write, stop: &AtomicBool) -> Result<()> {
    let binary_path = if binary.is_absolute() {
        binary.to_owned()
    } else {
        std::env::current_dir()?.join(binary)
    };
    let binary = resolve_non_strict(binary)?;
    let sources = inputs::Sources::open(root)?;
    let artifact = inputs::Bound::open(&binary, None)?;
    let ticks = rustix::param::clock_ticks_per_second();
    let page = u64::try_from(rustix::param::page_size())?;
    for fallback in [false, true] {
        peer::check(stop)?;
        if parse_queue_size(&peer::Peer::queue(&binary, stop)?)? != i64::try_from(queue_size())? {
            return Err("procLog service queue size differs".into());
        }
        let root = tempfile::Builder::new()
            .prefix("rust-proclog-reference-")
            .tempdir()?;
        prepare(root.path(), fallback)?;
        let mut child = peer::Peer::start(
            &binary,
            &[
                root.path().as_os_str().to_owned(),
                ticks.to_string().into(),
                page.to_string().into(),
            ],
            stop,
        )?;
        for cycle in 0..22 {
            advance(root.path(), fallback, cycle)?;
            child.send(1000 + cycle as u64)?;
            let bytes = child.frame()?;
            compare_frame(cycle, ticks, page, &bytes)?;
            // Keep the original assertion independent of full Event equality.
            // A host too small for the synthetic RSS threshold must fail here,
            // as the original checker does; source-only parameter controls still
            // cover the collector's legitimate skip branch independently.
            let actual = project_event(&bytes)?;
            let first = actual["procLog"]["procs"]
                .as_array()
                .and_then(|procs| procs.iter().min_by_key(|proc| proc["pid"].as_i64()))
                .ok_or("procLog processes missing")?;
            if first["memPss"].as_u64() != Some(if cycle < 20 { 50 * 1024 } else { 99 * 1024 }) {
                return Err("procLog original PSS cache assertion differs".into());
            }
        }
        child.finish()?;
        drop(child);
        root.close()?;
        sources.verify()?;
        artifact.verify_path(&binary)?;
        artifact.verify_path(&binary_path)?;
        peer::check(stop)?;
        writeln!(printed,"PASS: every Event/procLog field matches the original Python implementation for 22 {} cache cycles",if fallback {"smaps"} else {"smaps_rollup"})?;
    }
    peer::check(stop)
}
struct Signals(Vec<signal_hook::SigId>);
impl Drop for Signals {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
pub fn cli() -> Result<()> {
    let root = std::env::var_os("OPENPILOT_RUST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let Some(binary) = arguments(&std::env::args_os().skip(1).collect::<Vec<_>>(), &root)? else {
        println!("usage: check-proclog-reference [--binary BINARY]");
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let mut signals = Signals(vec![]);
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signals
            .0
            .push(signal_hook::flag::register(signal, stop.clone())?);
    }
    check(&root, &binary, &mut std::io::stdout(), &stop)
}
