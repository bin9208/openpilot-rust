use crate::{param_changes::text, Error, Value};
use base64::{
    alphabet,
    engine::{general_purpose::GeneralPurpose, GeneralPurposeConfig},
    Engine,
};
use sha2::{Digest, Sha256};

fn fingerprint(points: &[u32]) -> String {
    let Ok(blob) = Value::Text(points.to_vec()).string() else {
        return String::new();
    };
    if !blob.is_ascii()
        || blob
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !b"+/=".contains(&byte))
    {
        return String::new();
    }
    let end = blob.find('=').unwrap_or(blob.len());
    if blob[end..].bytes().any(|byte| byte != b'=') {
        return String::new();
    }
    let pads = blob.len() - end;
    if (end == 0 && pads != 0)
        || !match end % 4 {
            0 => pads == 0,
            2 => pads == 2,
            3 => pads == 1,
            _ => false,
        }
    {
        return String::new();
    }
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    );
    let Ok(raw) = engine.decode(&blob[..end]) else {
        return String::new();
    };
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(raw))
            .trim_end_matches('=')
    )
}

pub(super) fn keys(value: &Value) -> Result<Vec<Value>, Error> {
    let Value::Text(points) = text::string(value, true)? else {
        return Err(Error::Source("expected Python string".into()));
    };
    let mut summaries = Vec::new();
    for line in points.split(|point| matches!(*point, 10..=13 | 28..=30 | 133 | 8232 | 8233)) {
        let line = crate::state::trim(line);
        if line.is_empty() || line.first() == Some(&35) {
            continue;
        }
        let mut parts = line
            .split(|point| crate::state::trim(std::slice::from_ref(point)).is_empty())
            .filter(|part| !part.is_empty());
        let Some(kind) = parts.next() else { continue };
        let Some(blob) = parts.next() else { continue };
        let Ok(kind) = Value::Text(kind.to_vec()).string() else {
            continue;
        };
        if !matches!(
            kind.as_str(),
            "ssh-ed25519"
                | "ssh-rsa"
                | "ecdsa-sha2-nistp256"
                | "ecdsa-sha2-nistp384"
                | "ecdsa-sha2-nistp521"
                | "sk-ssh-ed25519@openssh.com"
                | "sk-ecdsa-sha2-nistp256@openssh.com"
        ) {
            continue;
        }
        summaries.push(Value::object([
            ("type", Value::text(&kind.replace("ssh-", ""))),
            ("fingerprint", Value::text(&fingerprint(blob))),
        ]));
    }
    Ok(summaries)
}
