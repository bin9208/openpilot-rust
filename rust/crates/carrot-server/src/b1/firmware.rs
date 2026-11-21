//! Installs the B1 image bundled in this checkout when the enrolled
//! indicator controller is idle. Port of carrot's
//! `tools/carrot_b1/auto_update.py` and `ble_update.install`.
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const IMAGE: &str = "/data/openpilot/tools/carrot_b1/CarrotB1.bin";
pub const RECEIPT: &str = "/data/carrot/b1-update.json";
pub const TRACE: &str = "db340a96-804c-4ead-af37-8a0e6af754a1";
pub const OTA: &str = "db340a94-804c-4ead-af37-8a0e6af754a1";

/// `carrot-b1-release=<n>` occurrences.
fn tags(data: &[u8]) -> Vec<u64> {
    const TAG: &[u8] = b"carrot-b1-release=";
    let mut found = Vec::new();
    let mut i = 0;
    while let Some(pos) = data[i..].windows(TAG.len()).position(|w| w == TAG) {
        let start = i + pos + TAG.len();
        let digits: Vec<u8> = data[start..]
            .iter()
            .copied()
            .take_while(u8::is_ascii_digit)
            .collect();
        if digits.first().is_some_and(|d| *d != b'0') {
            if let Some(n) = std::str::from_utf8(&digits)
                .ok()
                .and_then(|t| t.parse().ok())
            {
                found.push(n);
            }
        }
        i = start;
    }
    found
}

/// `(release, elf sha)` of a valid ESP32-S3 release image.
pub fn release_id(image: &[u8]) -> Result<(u64, String), String> {
    let tags = tags(image);
    let valid = image.len() >= 256
        && image[0] == 0xe9
        && image[12..14] == [0x09, 0x00]
        && image[32..36] == [0x32, 0x54, 0xcd, 0xab]
        && Sha256::digest(&image[..image.len() - 32]).as_slice() == &image[image.len() - 32..]
        && tags.len() == 1
        && image[176..208].iter().any(|b| *b != 0);
    if !valid {
        return Err("invalid bundled B1 release image".into());
    }
    Ok((tags[0], hex::encode(&image[176..208])))
}

fn build(report: &[u8]) -> Option<String> {
    const KEY: &[u8] = b"elf=";
    report.windows(KEY.len()).enumerate().find_map(|(i, w)| {
        let word = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_';
        if w != KEY || (i > 0 && word(&report[i - 1])) {
            return None;
        }
        let hex = &report[i + KEY.len()..];
        let hex = hex.get(..64)?;
        let boundary = report.get(i + KEY.len() + 64).is_none_or(|b| !word(b));
        (boundary
            && hex
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)))
        .then(|| String::from_utf8_lossy(hex).into_owned())
    })
}

/// `decision(report, target, record, mac, now)`.
pub fn decision(
    report: &[u8],
    target: &(u64, String),
    record: &Value,
    mac: &str,
    now: f64,
) -> Result<&'static str, String> {
    let (release, elf) = target;
    let tag = tags(report).first().copied();
    let built = build(report);
    if tag.is_some() || built.is_some() {
        let (Some(version), Some(built)) = (tag, built) else {
            return Err("incomplete board build identity".into());
        };
        if version > *release {
            return Ok("newer");
        }
        if version == *release {
            return Ok(if built == *elf {
                "current"
            } else {
                "version conflict"
            });
        }
    }
    if record.get("mac").and_then(Value::as_str) == Some(mac)
        && record.get("elf").and_then(Value::as_str) == Some(elf.as_str())
    {
        let status = record.get("status").and_then(Value::as_str);
        if tag.is_none() && matches!(status, Some("uploaded" | "current")) {
            return Ok("waiting for build identity");
        }
        let time = record.get("time").and_then(Value::as_f64).unwrap_or(0.);
        if -600. < now - time && now - time < 600. {
            return Ok("retry later");
        }
    }
    Ok("install")
}

/// The OTA manifest: `<BII` mode, nonce, size + image SHA-256.
pub fn manifest(mode: u8, nonce: u32, image: &[u8]) -> Vec<u8> {
    let mut out = vec![mode];
    out.extend_from_slice(&nonce.to_le_bytes());
    out.extend_from_slice(&(image.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(image));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bundled_image_is_a_valid_release() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/carrot_b1/CarrotB1.bin"
        );
        let image = std::fs::read(path).expect("bundled image");
        let (release, elf) = release_id(&image).expect("valid");
        assert!(release > 0);
        assert_eq!(elf.len(), 64);
    }

    #[test]
    fn decision_follows_board_identity_and_receipts() {
        let target = (5, "a".repeat(64));
        let elf = format!("elf={}", "a".repeat(64));
        let report = format!("carrot-b1-release=5 {elf}");
        assert_eq!(
            decision(report.as_bytes(), &target, &json!({}), "M", 0.),
            Ok("current")
        );
        let newer = format!("carrot-b1-release=6 {elf}");
        assert_eq!(
            decision(newer.as_bytes(), &target, &json!({}), "M", 0.),
            Ok("newer")
        );
        assert!(decision(b"carrot-b1-release=5", &target, &json!({}), "M", 0.).is_err());
        assert_eq!(decision(b"", &target, &json!({}), "M", 0.), Ok("install"));
        let receipt = json!({"mac": "M", "elf": "a".repeat(64), "status": "attempt", "time": 100.});
        assert_eq!(
            decision(b"", &target, &receipt, "M", 200.),
            Ok("retry later")
        );
        let uploaded =
            json!({"mac": "M", "elf": "a".repeat(64), "status": "uploaded", "time": 100.});
        assert_eq!(
            decision(b"", &target, &uploaded, "M", 2000.),
            Ok("waiting for build identity")
        );
    }
}
