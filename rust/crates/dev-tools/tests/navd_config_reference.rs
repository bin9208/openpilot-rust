//! Independent source worker versus unchanged original real Params/JWT controls.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openpilot_dev_tools::navd_config_check::{self as native, Case};
use openpilot_log_format::JsonValue;
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use rsa::{pkcs8::DecodePublicKey, Pkcs1v15Sign, RsaPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn read(n: &str) -> Value {
    serde_json::from_slice(
        &fs::read(
            root()
                .join("rust/crates/dev-tools/tests/fixtures/navd-config")
                .join(n),
        )
        .unwrap(),
    )
    .unwrap()
}
fn worker() -> PathBuf {
    let p = PathBuf::from(
        std::env::var_os("NAVD_CONFIG_WORKER_BINARY")
            .expect("prerequisite: NAVD_CONFIG_WORKER_BINARY must name current native checker ELF"),
    );
    assert!(p.is_absolute() && p.is_file());
    p
}
fn json(v: &JsonValue) -> Value {
    serde_json::from_str(&v.to_json().unwrap()).unwrap()
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
#[test]
fn all26_and_eight_dynamic_original_source_controls_preserve_raw_outcomes_and_real_signatures() {
    let f = read("source-original.json");
    assert_eq!(f["rows"].as_array().unwrap().len(), 34);
    let t = tempfile::tempdir().unwrap();
    let w = worker();
    for row in f["rows"].as_array().unwrap() {
        let name = row["case"].as_str().unwrap();
        let case = Case {
            name: name.into(),
            input: JsonValue::parse(&row["input"].to_string()).unwrap(),
        };
        let output = t.path().join(name);
        let before = now();
        let observed = native::observe(&case, &root(), &output, &w).unwrap();
        let after = now();
        assert_eq!(
            json(&observed.normalized),
            row["normalized"],
            "normalized {name}"
        );
        assert_eq!(
            observed.returncode,
            row["returncode"].as_i64().unwrap() as i32,
            "child phase {name}"
        );
        let mut raw = json(&observed.raw);
        if raw["ok"] == true && raw["host"] == "https://maps.comma.ai" {
            let token = raw["token"].as_str().unwrap();
            let parts = token.split('.').collect::<Vec<_>>();
            assert_eq!(parts.len(), 3);
            let header: Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
            let claims_raw = URL_SAFE_NO_PAD.decode(parts[1]).unwrap();
            let claims: Value = serde_json::from_slice(&claims_raw).unwrap();
            let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
            let message = format!("{}.{}", parts[0], parts[1]);
            match header["alg"].as_str().unwrap() {
                "RS256" => {
                    let mut digest_info = vec![
                        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03,
                        0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20,
                    ];
                    digest_info.extend_from_slice(&Sha256::digest(message.as_bytes()));
                    let public = &observed.public_keys["rsa"];
                    RsaPublicKey::from_public_key_pem(public)
                        .unwrap()
                        .verify(Pkcs1v15Sign::new_unprefixed(), &digest_info, &signature)
                        .unwrap();
                    let mut wrong = signature.clone();
                    wrong[0] ^= 1;
                    assert!(RsaPublicKey::from_public_key_pem(public)
                        .unwrap()
                        .verify(Pkcs1v15Sign::new_unprefixed(), &digest_info, &wrong)
                        .is_err());
                }
                "ES256" => {
                    let key =
                        VerifyingKey::from_public_key_pem(&observed.public_keys["ec"]).unwrap();
                    key.verify(
                        message.as_bytes(),
                        &Signature::from_slice(&signature).unwrap(),
                    )
                    .unwrap();
                    assert!(key
                        .verify(
                            b"changed-claims",
                            &Signature::from_slice(&signature).unwrap()
                        )
                        .is_err());
                }
                "none" => assert!(signature.is_empty()),
                other => panic!("unsupported original JWT {other}"),
            };
            let nbf = claims["nbf"].as_i64().unwrap();
            assert!((before - 1.0..=after).contains(&(nbf as f64)));
            assert_eq!(claims["iat"], claims["nbf"]);
            assert_eq!(claims["exp"].as_i64().unwrap() - nbf, 2419200);
            assert_eq!(claims["identity"], row["normalized"]["identity"]);
            if header["alg"] == "none" {
                let wanted = format!(
                    "{{\"identity\":{},\"nbf\":{nbf},\"iat\":{nbf},\"exp\":{}}}",
                    if claims["identity"].is_null() {
                        "null".to_owned()
                    } else {
                        ascii_json(claims["identity"].as_str().unwrap())
                    },
                    nbf + 2419200
                );
                assert_eq!(claims_raw, wanted.as_bytes());
            }
            raw["token"] = "<original-signature-and-claims-verified>".into();
        }
        assert_eq!(raw, row["raw_result"], "raw source {name}");
        assert!(output.join("stdout").exists());
        assert!(output.join("stderr").exists());
    }
}
// Independent tiny JSON-string reference for unsigned original ensure_ascii;
// it does not import the native source policy or use captured token output.
fn ascii_json(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap();
    let mut out = String::new();
    for c in quoted.chars() {
        if c as u32 >= 127 {
            for u in c.encode_utf16(&mut [0; 2]) {
                out.push_str(&format!("\\u{u:04x}"));
            }
        } else {
            out.push(c)
        }
    }
    out
}

#[test]
fn original_gps_place_name_get_precedes_coordinate_indexing_for_membership_containers() {
    let path =
        root().join("rust/crates/dev-tools/tests/fixtures/navd-config/gps-get-original.json");
    let bytes = fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "3939c5583d588d1989bf38bf0f6364cbfb6adfe9dd913d51553078735146508f"
    );
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fixture["source_worker_captured"], true);
    assert_eq!(fixture["candidate_used_for_expectation"], false);
    let t = tempfile::tempdir().unwrap();
    let worker = worker();
    for row in fixture["rows"].as_array().unwrap() {
        let name = row["case"].as_str().unwrap();
        let case = Case {
            name: name.into(),
            input: JsonValue::parse(&row["input"].to_string()).unwrap(),
        };
        let observed = native::observe(&case, &root(), &t.path().join(name), &worker).unwrap();
        assert_eq!(observed.returncode, 0);
        assert_eq!(
            json(&observed.raw),
            row["raw_result"],
            "actual original raw exception phase {name}"
        );
        assert_eq!(json(&observed.normalized), serde_json::json!({"ok":false}));
    }
}
