use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use super::{PlatformError, PlatformResult};

const TOKEN_BYTES: usize = 32;
const TOKEN_ENCODED_LENGTH: usize = 43;
const PAIRING_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingChallenge {
    pub challenge_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub hostname: String,
    pub ca_fingerprint_sha256: String,
    /// The token is placed in the fragment so reverse proxies and access logs
    /// never receive it as part of the request target.
    pub qr_payload: String,
}

impl Drop for PairingChallenge {
    fn drop(&mut self) {
        self.token.zeroize();
        self.qr_payload.zeroize();
    }
}

#[derive(Clone)]
pub struct PairingGrant {
    pub challenge_id: String,
    pub hostname: String,
    pub ca_fingerprint_sha256: String,
}

struct PendingPairing {
    challenge_id: String,
    token_hash: [u8; 32],
    expires_at: Instant,
    hostname: String,
    ca_fingerprint_sha256: String,
}

impl Drop for PendingPairing {
    fn drop(&mut self) {
        self.token_hash.zeroize();
    }
}

/// Holds at most one pairing challenge. Challenges are process-local,
/// one-shot and intentionally disappear on service restart.
pub struct PairingManager {
    pending: Mutex<Option<PendingPairing>>,
    ttl: Duration,
}

impl Default for PairingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingManager {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            ttl: PAIRING_TTL,
        }
    }

    #[cfg(test)]
    fn with_ttl(ttl: Duration) -> Self {
        Self {
            pending: Mutex::new(None),
            ttl,
        }
    }

    /// Invalidates any earlier challenge before creating a new one.
    /// This must only be called by the loopback administrative endpoint.
    pub fn start(
        &self,
        hostname: &str,
        ca_fingerprint_sha256: &str,
    ) -> PlatformResult<PairingChallenge> {
        validate_hostname(hostname)?;
        validate_fingerprint(ca_fingerprint_sha256)?;
        let ca_fingerprint_sha256 = ca_fingerprint_sha256.replace(':', "").to_ascii_uppercase();

        let mut token_bytes = Zeroizing::new([0_u8; TOKEN_BYTES]);
        OsRng.fill_bytes(token_bytes.as_mut());
        let mut token = Zeroizing::new(URL_SAFE_NO_PAD.encode(token_bytes.as_slice()));
        if token.len() != TOKEN_ENCODED_LENGTH {
            return Err(PlatformError::security());
        }

        let token_hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let challenge_id = Uuid::now_v7().to_string();
        let expires_at_utc = Utc::now()
            + chrono::Duration::from_std(self.ttl).map_err(|_| PlatformError::invalid_state())?;
        let expires_at = Instant::now()
            .checked_add(self.ttl)
            .ok_or_else(PlatformError::invalid_state)?;
        let qr_payload = format!(
            "https://{hostname}:8743/pair#token={}&fingerprint={ca_fingerprint_sha256}",
            token.as_str()
        );

        let pending = PendingPairing {
            challenge_id: challenge_id.clone(),
            token_hash,
            expires_at,
            hostname: hostname.to_owned(),
            ca_fingerprint_sha256: ca_fingerprint_sha256.clone(),
        };
        *self
            .pending
            .lock()
            .map_err(|_| PlatformError::invalid_state())? = Some(pending);

        let challenge = PairingChallenge {
            challenge_id,
            token: token.to_string(),
            expires_at: expires_at_utc,
            hostname: hostname.to_owned(),
            ca_fingerprint_sha256,
            qr_payload,
        };
        token.zeroize();
        Ok(challenge)
    }

    /// Consumes a challenge exactly once. Callers must avoid logging `token`.
    pub fn consume(&self, token: &str) -> PlatformResult<PairingGrant> {
        if token.len() != TOKEN_ENCODED_LENGTH {
            return Err(PlatformError::security());
        }
        let decoded = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(token)
                .map_err(|_| PlatformError::security())?,
        );
        if decoded.len() != TOKEN_BYTES {
            return Err(PlatformError::security());
        }

        let presented_hash = Zeroizing::new(<[u8; 32]>::from(Sha256::digest(token.as_bytes())));
        let mut guard = self
            .pending
            .lock()
            .map_err(|_| PlatformError::invalid_state())?;
        let pending = guard.as_ref().ok_or_else(PlatformError::security)?;

        if Instant::now() > pending.expires_at {
            *guard = None;
            return Err(PlatformError::security());
        }
        if !bool::from(
            presented_hash
                .as_slice()
                .ct_eq(pending.token_hash.as_slice()),
        ) {
            return Err(PlatformError::security());
        }

        let pending = guard.take().ok_or_else(PlatformError::security)?;

        Ok(PairingGrant {
            challenge_id: pending.challenge_id.clone(),
            hostname: pending.hostname.clone(),
            ca_fingerprint_sha256: pending.ca_fingerprint_sha256.clone(),
        })
    }

    pub fn cancel(&self) -> PlatformResult<()> {
        *self
            .pending
            .lock()
            .map_err(|_| PlatformError::invalid_state())? = None;
        Ok(())
    }
}

fn validate_hostname(hostname: &str) -> PlatformResult<()> {
    let label = hostname
        .strip_suffix(".local")
        .ok_or_else(PlatformError::invalid_input)?;
    if label.is_empty()
        || label.len() > 63
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || label.starts_with('-')
        || label.ends_with('-')
    {
        return Err(PlatformError::invalid_input());
    }
    Ok(())
}

fn validate_fingerprint(value: &str) -> PlatformResult<()> {
    let compact = value.replace(':', "");
    if compact.len() != 64 || !compact.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PlatformError::invalid_input());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::PairingManager;

    const FINGERPRINT: &str = "01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF";

    #[test]
    fn challenge_is_single_use_and_token_is_not_in_request_target() {
        let manager = PairingManager::new();
        let challenge = manager
            .start("dental-018f.local", FINGERPRINT)
            .expect("start pairing");

        assert_eq!(challenge.token.len(), 43);
        assert!(challenge.qr_payload.contains("#token="));
        assert!(!challenge.qr_payload.contains("?token="));

        let grant = manager.consume(&challenge.token).expect("consume once");
        assert_eq!(grant.challenge_id, challenge.challenge_id);
        assert!(manager.consume(&challenge.token).is_err());
    }

    #[test]
    fn wrong_token_does_not_consume_the_challenge() {
        let manager = PairingManager::new();
        let challenge = manager
            .start("dental-018f.local", FINGERPRINT)
            .expect("start pairing");
        let wrong = "A".repeat(43);

        assert!(manager.consume(&wrong).is_err());
        assert!(manager.consume(&challenge.token).is_ok());
    }

    #[test]
    fn expired_challenge_fails_closed() {
        let manager = PairingManager::with_ttl(Duration::from_millis(1));
        let challenge = manager
            .start("dental-018f.local", FINGERPRINT)
            .expect("start pairing");
        thread::sleep(Duration::from_millis(5));

        assert!(manager.consume(&challenge.token).is_err());
    }

    #[test]
    fn starting_again_invalidates_the_previous_challenge() {
        let manager = PairingManager::new();
        let first = manager
            .start("dental-018f.local", FINGERPRINT)
            .expect("first pairing");
        let second = manager
            .start("dental-018f.local", FINGERPRINT)
            .expect("second pairing");

        assert!(manager.consume(&first.token).is_err());
        assert!(manager.consume(&second.token).is_ok());
    }
}
