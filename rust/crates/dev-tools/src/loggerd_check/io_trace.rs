//! Actual strace write/sync/close evidence. Ignore only PID/FD identities and
//! the two owned route roots; retain syscall order, lengths, results and errno.
use super::super::{require, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone)]
pub struct Call {
    pub pid: u32,
    pub operation: String,
    pub path: String,
    pub requested: Option<u64>,
    pub result: i64,
    pub errno: Option<String>,
}
pub struct Trace {
    pub calls: Vec<Call>,
    pub exits: BTreeMap<u32, i32>,
}

/// Rejoin strace's per-PID unfinished/resumed records before interpreting any
/// syscall. Unknown/unannotated descriptors never establish media evidence.
pub fn parse(text: &str) -> Result<Trace> {
    let mut pending = BTreeMap::new();
    let mut calls = Vec::new();
    let mut exits = BTreeMap::new();
    for line in text.lines() {
        let (pid, tail) = line
            .split_once(char::is_whitespace)
            .ok_or("strace PID missing")?;
        let pid: u32 = pid.parse()?;
        let tail = tail.trim_start();
        if let Some(status) = tail
            .strip_prefix("+++ exited with ")
            .and_then(|v| v.strip_suffix(" +++"))
        {
            require(
                exits.insert(pid, status.parse()?).is_none(),
                "duplicate trace exit",
            )?;
            continue;
        }
        if tail.starts_with("--- ") || tail.starts_with("+++ killed by ") {
            continue;
        }
        let joined;
        let tail = if let Some(prefix) = tail.strip_suffix("<unfinished ...>") {
            require(
                pending.insert(pid, prefix.to_owned()).is_none(),
                "duplicate unfinished syscall",
            )?;
            continue;
        } else if tail.starts_with("<... ") {
            let (_, rest) = tail
                .split_once(" resumed>")
                .ok_or("invalid resumed syscall")?;
            let prefix = pending
                .remove(&pid)
                .ok_or("resumed syscall has no owned entry")?;
            let operation = tail
                .strip_prefix("<... ")
                .unwrap()
                .split_once(' ')
                .ok_or("resumed name missing")?
                .0;
            require(
                prefix.starts_with(&format!("{operation}(")),
                "resumed syscall changed operation",
            )?;
            joined = prefix + rest;
            joined.as_str()
        } else {
            tail
        };
        let Some((operation, arguments)) = tail.split_once('(') else {
            return Err("unexpected strace record".into());
        };
        // Retain raw exec records for the parent's provider/descendant audit;
        // executable names are not media IO and never count as write evidence.
        if ["execve", "execveat"].contains(&operation) {
            continue;
        }
        require(
            ["write", "fsync", "fdatasync", "close"].contains(&operation),
            "unexpected traced operation",
        )?;
        let Some((fd, annotated)) = arguments.split_once('<') else {
            // E.g. close(3) = -1 EBADF: no -yy path, hence no file attribution.
            continue;
        };
        let _: u32 = fd.parse()?;
        let mut depth = 1usize;
        let mut boundary = None;
        for (i, byte) in annotated.bytes().enumerate() {
            match byte {
                b'<' => depth += 1,
                b'>' => {
                    depth -= 1;
                    if depth == 0 {
                        boundary = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = boundary.ok_or("unterminated strace descriptor annotation")?;
        let path = annotated[..end].to_owned();
        let (arguments, result) = annotated[end + 1..]
            .rsplit_once(" = ")
            .ok_or("strace syscall result absent")?;
        let arguments = arguments
            .trim_end()
            .strip_suffix(')')
            .ok_or("strace syscall closing delimiter absent")?;
        let mut words = result.split_whitespace();
        let result: i64 = words.next().ok_or("strace return absent")?.parse()?;
        let errno = if result == -1 {
            Some(
                words
                    .next()
                    .ok_or("failed syscall errno absent")?
                    .to_owned(),
            )
        } else {
            None
        };
        let requested = if operation == "write" {
            Some(
                arguments
                    .rsplit_once(',')
                    .ok_or("write length missing")?
                    .1
                    .trim()
                    .parse()?,
            )
        } else {
            None
        };
        calls.push(Call {
            pid,
            operation: operation.into(),
            path,
            requested,
            result,
            errno,
        });
    }
    require(pending.is_empty(), "trace ended with unfinished syscalls")?;
    Ok(Trace { calls, exits })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub operation: String,
    pub target: String,
    pub requested: Option<u64>,
    pub result: i64,
    pub errno: Option<String>,
}

fn writer(
    operations: &[Operation],
    target: &str,
    full: bool,
    exact_bytes: Option<u64>,
) -> Result<()> {
    let rows = operations
        .iter()
        .filter(|v| v.target == target)
        .collect::<Vec<_>>();
    require(
        !rows.is_empty(),
        format!("{target}: no actual media syscalls"),
    )?;
    let mut bytes = 0_u64;
    let mut writes = 0;
    let mut closed = false;
    for row in rows {
        require(!closed, "media syscall after final close")?;
        match row.operation.as_str() {
            "write" => {
                writes += 1;
                let count = row.requested.ok_or("media write count absent")?;
                require(count > 0, "empty media write is not traffic evidence")?;
                if full {
                    require(
                        row.result == -1 && row.errno.as_deref() == Some("ENOSPC"),
                        "/dev/full did not return actual ENOSPC",
                    )?;
                } else {
                    require(
                        row.result == i64::try_from(count)? && row.errno.is_none(),
                        "media write was incomplete",
                    )?;
                    bytes = bytes.checked_add(count).ok_or("media byte sum overflow")?;
                }
            }
            "close" => {
                require(
                    writes > 0 && row.result == 0 && row.errno.is_none(),
                    "media did not close successfully after writes",
                )?;
                closed = true;
            }
            _ => return Err("unexpected media sync syscall".into()),
        }
    }
    require(closed, "media close syscall absent")?;
    if let Some(exact) = exact_bytes {
        require(bytes == exact, "raw flush byte count changed")?;
    }
    Ok(())
}

/// Validate actual trace and return only approved identity projections. The
/// resulting ordered operations must also match the current original process.
pub fn audit(text: &str, root: &Path, route: &str, mode: &str) -> Result<Value> {
    let parsed = parse(text)?;
    let log_root = root.join("logs");
    let log_root = log_root.to_str().ok_or("trace log root is not UTF-8")?;
    let params = root.join("params");
    let params = params.to_str().ok_or("trace Params root is not UTF-8")?;
    let raw = format!("{log_root}/{route}--0/fcamera.hevc");
    let rotated = format!("{log_root}/{route}--1/qcamera.ts");
    let mut operations = Vec::new();
    let mut actors = Vec::new();
    let mut params_sync = false;
    for row in &parsed.calls {
        if ["fsync", "fdatasync"].contains(&row.operation.as_str()) {
            require(
                !row.path.starts_with(&format!("{log_root}/")),
                "log fsync/fdatasync policy changed",
            )?;
            params_sync |= row.operation == "fsync"
                && row.path.starts_with(&format!("{params}/"))
                && row.result == 0;
        }
        let target = if row.path == raw {
            Some("raw-segment0")
        } else if row.path == "/dev/full" || row.path.starts_with("/dev/full<") {
            Some("dev-full")
        } else if row.path == rotated {
            Some("qcamera-segment1")
        } else {
            None
        };
        if let Some(target) = target {
            actors.push(row.pid);
            operations.push(Operation {
                operation: row.operation.clone(),
                target: target.into(),
                requested: row.requested,
                result: row.result,
                errno: row.errno.clone(),
            });
        }
    }
    require(
        !actors.is_empty() && actors.iter().all(|pid| *pid == actors[0]),
        "media trace actor is absent or changed",
    )?;
    require(
        parsed.exits.get(&actors[0]) == Some(&0),
        "actual media actor has no successful final trace exit",
    )?;
    match mode {
        "flush" => {
            require(params_sync, "Params sync positive control absent")?;
            writer(&operations, "raw-segment0", false, Some(29))?;
            require(
                operations.iter().all(|r| r.target == "raw-segment0"),
                "unexpected flush media target",
            )?;
        }
        "raw" => {
            writer(&operations, "dev-full", true, None)?;
            require(
                operations.iter().all(|r| r.target == "dev-full"),
                "unexpected raw failure target",
            )?;
        }
        "remux" => {
            writer(&operations, "dev-full", true, None)?;
            writer(&operations, "qcamera-segment1", false, None)?;
            let old_close = operations
                .iter()
                .position(|v| v.target == "dev-full" && v.operation == "close")
                .ok_or("old remux close absent")?;
            let new_write = operations
                .iter()
                .position(|v| v.target == "qcamera-segment1" && v.operation == "write")
                .ok_or("rotated remux write absent")?;
            require(
                old_close < new_write,
                "rotated media write preceded old writer close",
            )?;
        }
        _ => return Err("unknown trace audit mode".into()),
    }
    let mut result = json!({"operations":operations,"log_syncs":[],"media_actor_exit":0});
    if mode == "flush" {
        result["params_sync_positive_control"] = json!(params_sync);
    }
    Ok(result)
}
