//! UI authentication policy from common/api.py and ui/lib/api_helpers.py.
use num_traits::ToPrimitive;
use openpilot_timed::clock::{self, Clock};
use openpilot_uploader::http::SigningKey;
use std::path::Path;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Registration(#[from] openpilot_registration::Error),
    #[error(transparent)]
    Clock(#[from] openpilot_timed::Error),
    #[error(transparent)]
    Signing(#[from] openpilot_uploader::TransferError),
    #[error("System time is not valid, cannot generate token")]
    InvalidTime,
    #[error("pairing key is unavailable")]
    MissingKey,
    #[error("token clock outside representable range")]
    ClockRange,
}
#[derive(Default)]
pub struct TokenCache {
    entry: Option<(String, u64, String)>,
}
impl TokenCache {
    /// The source LRU holds exactly one identity/hour entry. Failures do not evict it.
    pub fn get(
        &mut self,
        identity: &str,
        clock: &dyn Clock,
        paths: TokenPaths<'_>,
    ) -> Result<String, Error> {
        let bucket = clock.monotonic()? / 3_600_000_000_000;
        if let Some((old_identity, old_bucket, token)) = &self.entry {
            if old_identity == identity && *old_bucket == bucket {
                return Ok(token.clone());
            }
        }
        if !clock::valid(clock, paths.systemd)? {
            return Err(Error::InvalidTime);
        }
        let token = sign(identity, clock.wall_seconds()?, paths.persist, false)?;
        self.entry = Some((identity.into(), bucket, token.clone()));
        Ok(token)
    }
}
pub struct TokenPaths<'a> {
    pub persist: &'a Path,
    pub systemd: &'a Path,
}
/// Pairing uses the uncached one-hour token and intentionally does not check time validity.
pub fn pairing(identity: &str, wall: f64, persist: &Path) -> Result<String, Error> {
    sign(identity, wall, persist, true)
}
fn sign(identity: &str, wall: f64, persist: &Path, pair: bool) -> Result<String, Error> {
    let key = openpilot_registration::get_key_pair(persist)?.ok_or(Error::MissingKey)?;
    let seconds = wall.floor().to_i64().ok_or(Error::ClockRange)?;
    let expiry = seconds
        .checked_add(if pair { 3600 } else { 7200 })
        .ok_or(Error::ClockRange)?;
    let mut claims =
        serde_json::json!({"identity":identity,"nbf":seconds,"iat":seconds,"exp":expiry});
    if pair {
        claims["pair"] = true.into();
    }
    Ok(SigningKey::from_pem(key.algorithm, key.private.into_bytes()).token_claims(&claims)?)
}
