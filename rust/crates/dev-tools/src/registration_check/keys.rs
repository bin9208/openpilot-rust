//! Real QA RSA/P-256 keys and JOSE verification using primitive crypto only.
use super::values as v;
use crate::seal::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use openpilot_log_format::JsonValue;
use p256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) struct Pair {
    pub(super) private: String,
    pub(super) pkcs8: String,
    pub(super) public: String,
}
pub(crate) struct Keys(pub(super) BTreeMap<String, Pair>);
impl Keys {
    pub(crate) fn new() -> Result<Self> {
        let mut rng = rsa::rand_core::OsRng;
        let rsa = RsaPrivateKey::new(&mut rng, 2048)?;
        let ec = p256::SecretKey::random(&mut rng);
        Ok(Self(BTreeMap::from([
            (
                "rsa".into(),
                Pair {
                    private: rsa.to_pkcs1_pem(LineEnding::LF)?.to_string(),
                    pkcs8: rsa.to_pkcs8_pem(LineEnding::LF)?.to_string(),
                    public: RsaPublicKey::from(&rsa).to_public_key_pem(LineEnding::LF)?,
                },
            ),
            (
                "ec".into(),
                Pair {
                    private: ec.to_sec1_pem(LineEnding::LF)?.to_string(),
                    pkcs8: ec.to_pkcs8_pem(LineEnding::LF)?.to_string(),
                    public: ec.public_key().to_public_key_pem(LineEnding::LF)?,
                },
            ),
        ])))
    }
    pub(crate) fn public(&self) -> BTreeMap<String, String> {
        self.0
            .iter()
            .map(|(name, pair)| (name.clone(), pair.public.clone()))
            .collect()
    }
}
fn digest_info(message: &[u8]) -> Vec<u8> {
    // RFC 8017 SHA-256 DigestInfo; no runtime signer policy is shared.
    let mut bytes = vec![
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    bytes.extend_from_slice(&Sha256::digest(message));
    bytes
}
pub(super) fn sign(private: &str, algorithm: &str, expiration: i64) -> Result<String> {
    let header = URL_SAFE_NO_PAD.encode(format!(r#"{{"alg":"{algorithm}","typ":"JWT"}}"#));
    let claims = URL_SAFE_NO_PAD.encode(format!(r#"{{"register":true,"exp":{expiration}}}"#));
    let message = format!("{header}.{claims}");
    let signature = match algorithm {
        "RS256" => {
            let key = RsaPrivateKey::from_pkcs1_pem(private)
                .or_else(|_| RsaPrivateKey::from_pkcs8_pem(private))?;
            key.sign(
                Pkcs1v15Sign::new_unprefixed(),
                &digest_info(message.as_bytes()),
            )?
        }
        "ES256" => {
            let key = p256::SecretKey::from_sec1_pem(private)
                .or_else(|_| p256::SecretKey::from_pkcs8_pem(private))?;
            let key = SigningKey::from(key);
            let signature: Signature = key.sign(message.as_bytes());
            signature.to_bytes().to_vec()
        }
        _ => return Err("unsupported registration JWT algorithm".into()),
    };
    Ok(format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature)))
}
pub(super) fn verify(
    token: &str,
    public_keys: &BTreeMap<String, String>,
    expiration: i64,
) -> Result<(String, JsonValue, String)> {
    let pieces = token.split('.').collect::<Vec<_>>();
    if pieces.len() != 3 {
        return Err("registration token is not JOSE compact form".into());
    }
    let header = JsonValue::parse(std::str::from_utf8(&URL_SAFE_NO_PAD.decode(pieces[0])?)?)?;
    let algorithm = v::required_text(&header, "alg")?;
    let public = public_keys
        .get(if algorithm == "RS256" { "rsa" } else { "ec" })
        .ok_or("missing public key for actual signed registration request")?;
    let signature = URL_SAFE_NO_PAD.decode(pieces[2])?;
    let message = format!("{}.{}", pieces[0], pieces[1]);
    match algorithm.as_str() {
        "RS256" => RsaPublicKey::from_public_key_pem(public)?.verify(
            Pkcs1v15Sign::new_unprefixed(),
            &digest_info(message.as_bytes()),
            &signature,
        )?,
        "ES256" => VerifyingKey::from_public_key_pem(public)?
            .verify(message.as_bytes(), &Signature::from_slice(&signature)?)?,
        _ => return Err("unexpected registration token algorithm".into()),
    }
    let claims = JsonValue::parse(std::str::from_utf8(&URL_SAFE_NO_PAD.decode(pieces[1])?)?)?;
    let expected = v::value(serde_json::json!({"register":true,"exp":expiration}))?;
    if !v::equal(&claims, &expected)
        || !matches!(
            v::get(&claims, "register")?.view(),
            openpilot_log_format::JsonView::Bool(true)
        )
        || !matches!(
            v::get(&claims, "exp")?.view(),
            openpilot_log_format::JsonView::Integer(_)
        )
    {
        return Err("signed registration claims/types differ from original".into());
    }
    Ok((algorithm, claims, public.clone()))
}
impl Keys {
    #[cfg(feature = "navd-config-tools-native")]
    pub(crate) fn private(&self, kind: &str) -> Option<&str> {
        self.0.get(kind).map(|pair| pair.private.as_str())
    }
    pub(super) fn request(&self) -> Result<JsonValue> {
        v::object(
            self.0
                .iter()
                .map(|(name, pair)| {
                    Ok((
                        name.clone(),
                        v::object([
                            ("private".into(), v::text(&pair.private)),
                            ("pkcs8".into(), v::text(&pair.pkcs8)),
                            ("public".into(), v::text(&pair.public)),
                        ])?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }
    pub(super) fn from_request(value: &JsonValue) -> Result<Self> {
        Ok(Self(
            v::fields(value)?
                .into_iter()
                .map(|(name, pair)| {
                    Ok((
                        name,
                        Pair {
                            private: v::required_text(&pair, "private")?,
                            pkcs8: v::required_text(&pair, "pkcs8")?,
                            public: v::required_text(&pair, "public")?,
                        },
                    ))
                })
                .collect::<Result<_>>()?,
        ))
    }
}
