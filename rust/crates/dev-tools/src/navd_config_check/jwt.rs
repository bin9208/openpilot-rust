//! Original Api JWT wire policy, implemented with primitive crypto only.
use super::values as v;
use crate::seal::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use openpilot_log_format::{JsonValue, JsonView};
use p256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use rsa::{
    pkcs1::DecodeRsaPrivateKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
fn digest(message: &[u8]) -> Vec<u8> {
    let mut info = vec![
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    info.extend_from_slice(&Sha256::digest(message));
    info
}
pub(super) fn payload(identity: &JsonValue, seconds: i64) -> Result<String> {
    Ok(format!(
        "{{\"identity\":{},\"nbf\":{seconds},\"iat\":{seconds},\"exp\":{}}}",
        identity.to_json()?,
        seconds.checked_add(2419200).ok_or("JWT expiry overflow")?
    ))
}
pub(super) fn sign(
    algorithm: &str,
    private: &str,
    identity: &JsonValue,
    seconds: i64,
) -> Result<String> {
    let header = URL_SAFE_NO_PAD.encode(format!("{{\"alg\":\"{algorithm}\",\"typ\":\"JWT\"}}"));
    let body = URL_SAFE_NO_PAD.encode(payload(identity, seconds)?);
    let message = format!("{header}.{body}");
    let signature = match algorithm {
        "none" => Vec::new(),
        "RS256" => {
            let key = RsaPrivateKey::from_pkcs1_pem(private)
                .or_else(|_| RsaPrivateKey::from_pkcs8_pem(private))?;
            key.sign(Pkcs1v15Sign::new_unprefixed(), &digest(message.as_bytes()))?
        }
        "ES256" => {
            let key = p256::SecretKey::from_sec1_pem(private)
                .or_else(|_| p256::SecretKey::from_pkcs8_pem(private))?;
            let signature: Signature = SigningKey::from(key).sign(message.as_bytes());
            signature.to_bytes().to_vec()
        }
        _ => return Err("unsupported original JWT algorithm".into()),
    };
    Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
}
fn integer(v: &JsonValue) -> Result<i64> {
    match v.view() {
        JsonView::Integer(n) => Ok(n.parse()?),
        _ => Err("JWT timestamp is not original integer".into()),
    }
}
pub(super) fn normalize(
    raw: &JsonValue,
    public: &BTreeMap<String, String>,
    before: f64,
    after: f64,
) -> Result<JsonValue> {
    let mut fields = v::fields(raw)?;
    fields.retain(|(key, _)| key != "error" && key != "returncode");
    let ok = raw
        .get("ok")
        .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)));
    if ok
        && raw
            .get("host")
            .is_some_and(|v| v.text_eq("https://maps.comma.ai"))
    {
        let token = v::text(&v::field(raw, "token")?)?;
        let parts = token.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err("JWT compact form requires three parts".into());
        }
        let header = JsonValue::parse(std::str::from_utf8(&URL_SAFE_NO_PAD.decode(parts[0])?)?)?;
        let body = URL_SAFE_NO_PAD.decode(parts[1])?;
        let claims = JsonValue::parse(std::str::from_utf8(&body)?)?;
        let algorithm = v::text(&v::field(&header, "alg")?)?;
        let signature = URL_SAFE_NO_PAD.decode(parts[2])?;
        let message = format!("{}.{}", parts[0], parts[1]);
        match algorithm.as_str() {
            "none" => {
                if !signature.is_empty() {
                    return Err("unsigned source token has signature".into());
                }
            }
            "RS256" => RsaPublicKey::from_public_key_pem(
                public.get("rsa").ok_or("missing RSA verification key")?,
            )?
            .verify(
                Pkcs1v15Sign::new_unprefixed(),
                &digest(message.as_bytes()),
                &signature,
            )?,
            "ES256" => VerifyingKey::from_public_key_pem(
                public.get("ec").ok_or("missing EC verification key")?,
            )?
            .verify(message.as_bytes(), &Signature::from_slice(&signature)?)?,
            _ => return Err("unexpected JWT algorithm".into()),
        }
        let identity = v::field(&claims, "identity")?;
        let nbf = integer(&v::field(&claims, "nbf")?)?;
        let iat = integer(&v::field(&claims, "iat")?)?;
        let exp = integer(&v::field(&claims, "exp")?)?;
        if (nbf as f64) < before - 1.0
            || (nbf as f64) > after
            || iat != nbf
            || exp.checked_sub(iat) != Some(2419200)
        {
            return Err("JWT original time/expiry boundary changed".into());
        }
        if algorithm == "none" && body != payload(&identity, nbf)?.as_bytes() {
            return Err("unsigned JWT original JSON order/ASCII bytes changed".into());
        }
        fields.retain(|(key, _)| key != "token");
        fields.extend([
            ("header".into(), header),
            ("identity".into(), identity),
            ("expiry".into(), v::value(exp - iat)?),
        ]);
    }
    v::object(fields)
}
