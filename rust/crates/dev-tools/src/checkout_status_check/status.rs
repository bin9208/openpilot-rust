use super::{peer::Peer, require, Result};
use serde_json::{json, Value};
use std::fs;
fn float_hex(now: f64) -> String {
    if now.is_nan() {
        return "nan".into();
    }
    let sign = if now.is_sign_negative() { "-" } else { "" };
    if now.is_infinite() {
        return format!("{sign}inf");
    }
    if now == 0.0 {
        return format!("{sign}0x0.0p+0");
    }
    let bits = now.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    format!(
        "{sign}0x{}.{:013x}p{:+}",
        if exponent == 0 { 0 } else { 1 },
        bits & 0x000f_ffff_ffff_ffff,
        if exponent == 0 {
            -1022
        } else {
            exponent - 1023
        }
    )
}
pub fn packaged(peer: &mut Peer) -> Result<Value> {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    peer.metadata(&a)?;
    require(
        peer.op("capture", 0.0)?["running_commit"] == a,
        "packaged startup identity differs",
    )?;
    let mut rows = Vec::new();
    for (now, commit, expected) in [
        (0.0, &b, false),
        (4.999, &b, false),
        (5.0, &b, true),
        (10.0, &a, false),
        (15.0, &b, false),
        (20.0, &b, true),
    ] {
        peer.metadata(commit)?;
        let response = peer.op("update", now)?;
        require(
            response["returned"] == expected && response["running_commit"] == a,
            "packaged update differs",
        )?;
        rows.push(
            json!({"now":now,"required":response["returned"],"running":response["running_commit"]}),
        );
    }
    Ok(json!(rows))
}
fn installed(peer: &mut Peer, commit: &str, exit: i32) -> Result<()> {
    peer.fixture(json!({"stdout":commit.as_bytes(),"exit_code":exit}))
}
fn capture(peer: &mut Peer, commit: &str, exit: i32) -> Result<usize> {
    installed(peer, commit, exit)?;
    let before = peer.calls()?.len();
    let response = peer.op("capture", 0.0)?;
    require(
        response["running_commit"]
            == if exit == 0 {
                json!(commit)
            } else {
                Value::Null
            },
        "fixture capture identity differs",
    )?;
    require(
        peer.calls()?.len() == before + 1,
        "fixture capture read count differs",
    )?;
    Ok(before)
}
fn update(
    peer: &mut Peer,
    rows: &mut Vec<Value>,
    now: f64,
    expected: bool,
    calls: usize,
    running: Option<&str>,
) -> Result<()> {
    let response = peer.op("update", now)?;
    require(
        response["returned"] == expected
            && response["reboot_required"] == expected
            && response["running_commit"].as_str() == running,
        format!("Git update {} state differs: {response}", float_hex(now)),
    )?;
    require(
        peer.calls()?.len() == calls,
        format!("Git update {} read count differs", float_hex(now)),
    )?;
    rows.push(json!({"now_hex":float_hex(now),"returned":response["returned"],"running_commit":running,"calls":calls}));
    Ok(())
}
pub fn fixture(peer: &mut Peer) -> Result<Value> {
    fs::create_dir(peer.repo.join(".git"))?;
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let c = "c".repeat(40);
    let mut rows = Vec::new();
    let base = capture(peer, &a, 0)?;
    for (now, offset) in [(f64::NEG_INFINITY, 1), (-f64::MIN_POSITIVE, 1), (-0.0, 2)] {
        update(peer, &mut rows, now, false, base + offset, Some(&a))?;
    }
    installed(peer, &b, 0)?;
    update(
        peer,
        &mut rows,
        5.0_f64.next_down(),
        false,
        base + 2,
        Some(&a),
    )?;
    update(peer, &mut rows, 5.0, false, base + 3, Some(&a))?;
    installed(peer, &c, 0)?;
    update(peer, &mut rows, 4.0, false, base + 3, Some(&a))?;
    update(peer, &mut rows, 10.0, false, base + 4, Some(&a))?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, 15.0, false, base + 5, Some(&a))?;
    update(peer, &mut rows, 20.0, true, base + 6, Some(&a))?;
    installed(peer, &b, 7)?;
    update(peer, &mut rows, 25.0, false, base + 7, Some(&a))?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, 30.0, false, base + 8, Some(&a))?;
    update(peer, &mut rows, 35.0, true, base + 9, Some(&a))?;
    installed(peer, &a, 0)?;
    update(peer, &mut rows, 40.0, false, base + 10, Some(&a))?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, 45.0, false, base + 11, Some(&a))?;
    update(peer, &mut rows, 50.0, true, base + 12, Some(&a))?;
    for (interruption, exit) in [(&b, 7), (&a, 0)] {
        let base = capture(peer, &a, 0)?;
        installed(peer, &b, 0)?;
        update(peer, &mut rows, 0.0, false, base + 2, Some(&a))?;
        installed(peer, interruption, exit)?;
        update(peer, &mut rows, 5.0, false, base + 3, Some(&a))?;
        installed(peer, &b, 0)?;
        update(peer, &mut rows, 10.0, false, base + 4, Some(&a))?;
        update(peer, &mut rows, 15.0, true, base + 5, Some(&a))?;
    }
    let base = capture(peer, &a, 7)?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, 0.0, false, base + 2, None)?;
    update(peer, &mut rows, 5.0, false, base + 3, None)?;
    installed(peer, &c, 0)?;
    update(peer, &mut rows, 10.0, false, base + 4, None)?;
    update(peer, &mut rows, 15.0, false, base + 5, None)?;
    let base = capture(peer, &a, 0)?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, f64::NAN, false, base + 2, Some(&a))?;
    update(peer, &mut rows, f64::NEG_INFINITY, true, base + 3, Some(&a))?;
    installed(peer, &c, 0)?;
    update(
        peer,
        &mut rows,
        f64::NEG_INFINITY,
        false,
        base + 4,
        Some(&a),
    )?;
    update(peer, &mut rows, 0.0, true, base + 5, Some(&a))?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, f64::INFINITY, false, base + 6, Some(&a))?;
    update(peer, &mut rows, f64::MAX, false, base + 6, Some(&a))?;
    update(peer, &mut rows, f64::INFINITY, true, base + 7, Some(&a))?;
    installed(peer, &a, 0)?;
    update(peer, &mut rows, f64::NAN, false, base + 8, Some(&a))?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, f64::INFINITY, false, base + 9, Some(&a))?;
    update(
        peer,
        &mut rows,
        f64::NEG_INFINITY,
        false,
        base + 9,
        Some(&a),
    )?;
    update(peer, &mut rows, f64::NAN, true, base + 10, Some(&a))?;
    let base = capture(peer, &a, 0)?;
    installed(peer, &b, 0)?;
    update(peer, &mut rows, f64::MAX, false, base + 2, Some(&a))?;
    update(peer, &mut rows, f64::MAX, true, base + 3, Some(&a))?;
    update(
        peer,
        &mut rows,
        f64::MAX.next_down(),
        true,
        base + 3,
        Some(&a),
    )?;
    let base = capture(peer, &a, 0)?;
    installed(peer, &b, 0)?;
    let large = 1e16_f64;
    update(peer, &mut rows, large, false, base + 2, Some(&a))?;
    update(
        peer,
        &mut rows,
        (large + 5.0).next_down(),
        false,
        base + 2,
        Some(&a),
    )?;
    update(peer, &mut rows, large + 5.0, true, base + 3, Some(&a))?;
    require(rows.len() == 44, "Git status history coverage differs")?;
    Ok(json!(rows))
}
