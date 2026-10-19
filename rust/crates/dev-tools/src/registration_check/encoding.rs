//! Strict response-byte construction, independent from candidate decoding policy.
//! Existing native CPython codec tables are mechanics, never expected HTTP data.
use crate::seal::Result;
use std::{collections::BTreeMap, sync::OnceLock};

fn cp949_reverse() -> &'static BTreeMap<char, [u8; 2]> {
    static ENCODER: OnceLock<BTreeMap<char, [u8; 2]>> = OnceLock::new();
    ENCODER.get_or_init(|| {
        let mut reverse = BTreeMap::new();
        let mut decoded = String::new();
        for lead in 0x80u8..=0xff {
            for trail in 0..=0xff {
                let bytes = [lead, trail];
                if charset_norm::codecs::decode_into(
                    &bytes,
                    "cp949",
                    charset_norm::codecs::Errors::Strict,
                    &mut decoded,
                )
                .is_ok()
                {
                    let mut points = decoded.chars();
                    if let (Some(point), None) = (points.next(), points.next()) {
                        reverse.entry(point).or_insert(bytes);
                    }
                }
            }
        }
        reverse
    })
}

pub(super) fn encode(text: &str, encoding: &str) -> Result<Vec<u8>> {
    let canonical = charset_norm::encoding::iana_name(encoding, false)?;
    let bytes = if canonical == "utf_7" {
        encode_utf7(text)
    } else if canonical == "cp949" {
        // CPython CP949 is ASCII plus a bijective two-byte table. Its existing
        // decoder supplies that mechanical table; independent original controls
        // verify every one of its 17,176 canonical encoded scalars, not a selected
        // body/output. No fixture data participates in encoding.
        let mut bytes = Vec::with_capacity(text.len());
        for point in text.chars() {
            if point.is_ascii() {
                bytes.push(point as u8);
            } else {
                bytes.extend_from_slice(
                    cp949_reverse()
                        .get(&point)
                        .ok_or("fixture character cannot be encoded as strict CP949")?,
                );
            }
        }
        bytes
    } else {
        charset_norm::codecs::encode(text, &canonical)
            .ok_or_else(|| format!("unsupported fixture body encoding {encoding}"))?
    };
    // The shared convenience encoder uses errors=replace for single-byte
    // codecs. Python's str.encode here is strict: reject rather than silently
    // manufacture question marks. Valid encodings round-trip to the input.
    if charset_norm::codecs::decode(&bytes, &canonical, charset_norm::codecs::Errors::Strict)?
        != text
    {
        return Err(format!("fixture character cannot be encoded as strict {encoding}").into());
    }
    Ok(bytes)
}

fn encode_utf7(text: &str) -> Vec<u8> {
    use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
    // CPython's default codec leaves RFC2152 Set D, Set O and whitespace
    // direct. Only '+', backslash and tilde need special handling among graphic
    // ASCII; '+' is the explicit '+-' escape below.
    let direct = |c: char| {
        (c.is_ascii_graphic() && !matches!(c, '+' | '\\' | '~'))
            || matches!(c, ' ' | '\t' | '\r' | '\n')
    };
    let mut result = String::new();
    let mut shifted = Vec::new();
    let flush = |bytes: &mut Vec<u8>, out: &mut String, terminate: bool| {
        if !bytes.is_empty() {
            out.push('+');
            out.push_str(&STANDARD_NO_PAD.encode(&*bytes));
            if terminate {
                out.push('-');
            }
            bytes.clear()
        }
    };
    for c in text.chars() {
        if direct(c) {
            let terminate = c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '-');
            flush(&mut shifted, &mut result, terminate);
            result.push(c)
        } else if c == '+' && shifted.is_empty() {
            result.push_str("+-")
        } else {
            let mut units = [0; 2];
            for unit in c.encode_utf16(&mut units) {
                shifted.extend_from_slice(&unit.to_be_bytes());
            }
        }
    }
    flush(&mut shifted, &mut result, true);
    result.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn original_response_byte_encoders_cover_every_scenario_and_reject_unencodable_text() {
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registration/response-encode-original.json"
        ))
        .unwrap();
        let rows = original["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 147);
        let utf7: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registration/utf7-encoder-original.json"
        ))
        .unwrap();
        let utf7_rows = utf7["rows"].as_array().unwrap();
        assert_eq!(utf7_rows.len(), 265);
        for row in utf7_rows {
            assert_eq!(
                super::super::values::hex(&encode(row["text"].as_str().unwrap(), "utf-7").unwrap()),
                row["bytes_hex"].as_str().unwrap()
            );
        }
        for row in rows {
            let result = encode(
                row["text"].as_str().unwrap(),
                row["encoding"].as_str().unwrap(),
            );
            if let Some(expected) = row["bytes_hex"].as_str() {
                assert_eq!(
                    super::super::values::hex(&result.unwrap()),
                    expected,
                    "{} response {}",
                    row["case"],
                    row["response"]
                );
            } else {
                assert!(
                    result.is_err(),
                    "{} must reject instead of replacing",
                    row["case"]
                );
            }
        }
    }

    #[test]
    fn cp949_encoder_matches_all_original_canonical_scalars_and_generic_text() {
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/registration/cp949-encoder-original.json"
        ))
        .unwrap();
        let mut scalars = (0u8..128).map(char::from).collect::<Vec<_>>();
        scalars.extend(cp949_reverse().keys().copied());
        scalars.sort_unstable();
        scalars.dedup();
        assert_eq!(
            scalars.len() as u64,
            original["scalar_count"].as_u64().unwrap()
        );
        let mut digest = Sha256::new();
        for point in scalars {
            let bytes = encode(&point.to_string(), "cp949").unwrap();
            digest.update(u32::from(point).to_le_bytes());
            digest.update([u8::try_from(bytes.len()).unwrap()]);
            digest.update(bytes);
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            original["encoded_sha256"].as_str().unwrap()
        );
        for (text, bytes) in original["selected"].as_object().unwrap() {
            assert_eq!(
                super::super::values::hex(&encode(text, "cp949").unwrap()),
                bytes.as_str().unwrap()
            );
        }
        assert_eq!(
            encode("arbitrary한글힣뷁€", "949").unwrap(),
            encode("arbitrary한글힣뷁€", "cp949").unwrap()
        );
    }
}
