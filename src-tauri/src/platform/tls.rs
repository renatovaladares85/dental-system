use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, PublicKeyData,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use time::OffsetDateTime;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::infrastructure::KeyProtector;

use super::{PlatformError, PlatformResult, sync_directory};

type HmacSha256 = Hmac<Sha256>;

const IDENTITY_DIRECTORY: &str = "tls";
const ENVELOPE_PREFIX: &str = "identity-v1-";
const ENVELOPE_EXTENSION: &str = "json";
const PUBLIC_CA_FILE: &str = "ca.cer";
const MAX_ENVELOPE_BYTES: u64 = 512 * 1024;
const MAX_GENERATIONS: usize = 32;
const CA_VALIDITY_DAYS: i64 = 5 * 365;
const SERVER_VALIDITY_DAYS: i64 = 397;
const RENEWAL_WINDOW_DAYS: i64 = 30;
const CLOCK_SKEW_MINUTES: i64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsIdentityAction {
    Created,
    Renewed,
    Unchanged,
}

/// Runtime TLS material. Private key bytes are zeroized when dropped and are
/// never serializable or printable.
pub struct LoadedTlsIdentity {
    pub generation_id: String,
    pub hostname: String,
    pub certificate_chain_der: Vec<Vec<u8>>,
    pub private_key_der: Zeroizing<Vec<u8>>,
    pub ca_certificate_der: Vec<u8>,
    pub ca_fingerprint_sha256: String,
    pub server_not_after: DateTime<Utc>,
    pub action: TlsIdentityAction,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TlsEnvelopePayload {
    format_version: u16,
    generation_id: String,
    installation_id: String,
    hostname: String,
    protection: String,
    key_algorithm: String,
    created_at: DateTime<Utc>,
    ca_not_after: DateTime<Utc>,
    server_not_before: DateTime<Utc>,
    server_not_after: DateTime<Utc>,
    ca_certificate_der_base64: String,
    server_certificate_der_base64: String,
    protected_ca_private_key_base64: String,
    protected_server_private_key_base64: String,
    ca_public_key_sha256: String,
    server_public_key_sha256: String,
    ca_fingerprint_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TlsEnvelope {
    payload: TlsEnvelopePayload,
    integrity_tag_base64: String,
}

pub struct TlsIdentityManager<P> {
    directory: PathBuf,
    protector: P,
    operation_lock: Mutex<()>,
}

impl<P: KeyProtector> TlsIdentityManager<P> {
    pub fn new(product_root: &Path, protector: P) -> PlatformResult<Self> {
        if !product_root.is_absolute() {
            return Err(PlatformError::invalid_input());
        }
        Ok(Self {
            directory: product_root.join(IDENTITY_DIRECTORY),
            protector,
            operation_lock: Mutex::new(()),
        })
    }

    /// Creates CA and leaf immediately on host bootstrap (before setup), or
    /// renews only the leaf when it is inside the 30-day renewal window.
    pub fn ensure_identity(
        &self,
        installation_id: &str,
        hostname: &str,
        now: DateTime<Utc>,
    ) -> PlatformResult<LoadedTlsIdentity> {
        validate_identity_binding(installation_id, hostname)?;
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|_| PlatformError::invalid_state())?;
        fs::create_dir_all(&self.directory).map_err(|_| PlatformError::storage())?;

        let current = self.find_current_envelope(installation_id, hostname)?;
        let (envelope, action) = match current {
            None => (
                self.create_initial_envelope(installation_id, hostname, now)?,
                TlsIdentityAction::Created,
            ),
            Some(envelope) => {
                self.verify_envelope(&envelope, installation_id, hostname, now)?;
                if envelope.payload.server_not_after
                    <= now + ChronoDuration::days(RENEWAL_WINDOW_DAYS)
                {
                    (
                        self.create_renewal_envelope(&envelope, installation_id, hostname, now)?,
                        TlsIdentityAction::Renewed,
                    )
                } else {
                    (envelope, TlsIdentityAction::Unchanged)
                }
            }
        };

        self.persist_public_ca(&envelope)?;
        self.load_runtime_identity(envelope, action, installation_id, hostname, now)
    }

    pub fn public_ca_path(&self) -> PathBuf {
        self.directory.join(PUBLIC_CA_FILE)
    }

    fn create_initial_envelope(
        &self,
        installation_id: &str,
        hostname: &str,
        now: DateTime<Utc>,
    ) -> PlatformResult<TlsEnvelope> {
        let ca_not_after = now + ChronoDuration::days(CA_VALIDITY_DAYS);
        let mut ca_params = ca_parameters(installation_id, now, ca_not_after)?;
        let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
            .map_err(|_| PlatformError::security())?;
        let ca_private_der = Zeroizing::new(ca_key.serialize_der());
        let ca_public_key_sha256 = hex::encode(Sha256::digest(ca_key.subject_public_key_info()));
        let ca_certificate = ca_params
            .self_signed(&ca_key)
            .map_err(|_| PlatformError::security())?;
        let ca_certificate_der = ca_certificate.der().to_vec();
        let ca_fingerprint_sha256 = certificate_fingerprint(&ca_certificate_der);

        // The issuer owns the same parameters and key used by the self-signed CA.
        let issuer = Issuer::new(std::mem::take(&mut ca_params), ca_key);
        let generated = generate_server_certificate(hostname, now, ca_not_after, &issuer)?;
        let protected_ca = self
            .protector
            .protect(ca_private_der.as_slice())
            .map_err(|_| PlatformError::security())?;
        let protected_server = self
            .protector
            .protect(generated.private_key_der.as_slice())
            .map_err(|_| PlatformError::security())?;

        let payload = TlsEnvelopePayload {
            format_version: 1,
            generation_id: Uuid::now_v7().to_string(),
            installation_id: installation_id.to_owned(),
            hostname: hostname.to_owned(),
            protection: self.protector.protection_name().to_owned(),
            key_algorithm: "ECDSA_P256_SHA256".to_owned(),
            created_at: now,
            ca_not_after,
            server_not_before: generated.not_before,
            server_not_after: generated.not_after,
            ca_certificate_der_base64: STANDARD.encode(&ca_certificate_der),
            server_certificate_der_base64: STANDARD.encode(&generated.certificate_der),
            protected_ca_private_key_base64: STANDARD.encode(protected_ca),
            protected_server_private_key_base64: STANDARD.encode(protected_server),
            ca_public_key_sha256,
            server_public_key_sha256: generated.public_key_sha256,
            ca_fingerprint_sha256,
        };
        let envelope = sign_payload(payload, ca_private_der.as_slice())?;
        self.persist_envelope(&envelope)?;
        Ok(envelope)
    }

    fn create_renewal_envelope(
        &self,
        current: &TlsEnvelope,
        installation_id: &str,
        hostname: &str,
        now: DateTime<Utc>,
    ) -> PlatformResult<TlsEnvelope> {
        if current.payload.ca_not_after <= now + ChronoDuration::days(RENEWAL_WINDOW_DAYS + 1) {
            return Err(PlatformError::new("TLS_CA_REPAIR_REQUIRED"));
        }
        let protected_ca =
            decode_bounded(&current.payload.protected_ca_private_key_base64, 64 * 1024)?;
        let ca_private_der = self
            .protector
            .unprotect(&protected_ca)
            .map_err(|_| PlatformError::security())?;
        let ca_key =
            KeyPair::try_from(ca_private_der.as_slice()).map_err(|_| PlatformError::security())?;
        let ca_params = ca_parameters(
            installation_id,
            current.payload.created_at,
            current.payload.ca_not_after,
        )?;
        let issuer = Issuer::new(ca_params, ca_key);
        let generated =
            generate_server_certificate(hostname, now, current.payload.ca_not_after, &issuer)?;
        let protected_server = self
            .protector
            .protect(generated.private_key_der.as_slice())
            .map_err(|_| PlatformError::security())?;

        let payload = TlsEnvelopePayload {
            format_version: 1,
            generation_id: Uuid::now_v7().to_string(),
            installation_id: installation_id.to_owned(),
            hostname: hostname.to_owned(),
            protection: self.protector.protection_name().to_owned(),
            key_algorithm: "ECDSA_P256_SHA256".to_owned(),
            created_at: now,
            ca_not_after: current.payload.ca_not_after,
            server_not_before: generated.not_before,
            server_not_after: generated.not_after,
            ca_certificate_der_base64: current.payload.ca_certificate_der_base64.clone(),
            server_certificate_der_base64: STANDARD.encode(&generated.certificate_der),
            protected_ca_private_key_base64: current
                .payload
                .protected_ca_private_key_base64
                .clone(),
            protected_server_private_key_base64: STANDARD.encode(protected_server),
            ca_public_key_sha256: current.payload.ca_public_key_sha256.clone(),
            server_public_key_sha256: generated.public_key_sha256,
            ca_fingerprint_sha256: current.payload.ca_fingerprint_sha256.clone(),
        };
        let envelope = sign_payload(payload, ca_private_der.as_slice())?;
        self.persist_envelope(&envelope)?;
        Ok(envelope)
    }

    fn find_current_envelope(
        &self,
        installation_id: &str,
        hostname: &str,
    ) -> PlatformResult<Option<TlsEnvelope>> {
        let mut envelopes = Vec::new();
        let entries = fs::read_dir(&self.directory).map_err(|_| PlatformError::storage())?;
        for entry in entries {
            let entry = entry.map_err(|_| PlatformError::storage())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with(ENVELOPE_PREFIX) || !name.ends_with(".json") {
                continue;
            }
            if envelopes.len() >= MAX_GENERATIONS {
                return Err(PlatformError::security());
            }
            let metadata = entry.metadata().map_err(|_| PlatformError::storage())?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ENVELOPE_BYTES {
                return Err(PlatformError::security());
            }
            let bytes = fs::read(entry.path()).map_err(|_| PlatformError::storage())?;
            let envelope: TlsEnvelope =
                serde_json::from_slice(&bytes).map_err(|_| PlatformError::security())?;
            let expected_name = format!(
                "{ENVELOPE_PREFIX}{}.{}",
                envelope.payload.generation_id, ENVELOPE_EXTENSION
            );
            if envelope.payload.installation_id != installation_id
                || envelope.payload.hostname != hostname
                || name != expected_name
            {
                return Err(PlatformError::security());
            }
            envelopes.push(envelope);
        }

        if envelopes.is_empty() {
            return Ok(None);
        }
        let expected_ca = &envelopes[0].payload.ca_fingerprint_sha256;
        if envelopes
            .iter()
            .any(|value| value.payload.ca_fingerprint_sha256 != *expected_ca)
        {
            return Err(PlatformError::security());
        }
        envelopes.sort_by_key(|value| (value.payload.server_not_after, value.payload.created_at));
        Ok(envelopes.pop())
    }

    fn verify_envelope(
        &self,
        envelope: &TlsEnvelope,
        installation_id: &str,
        hostname: &str,
        now: DateTime<Utc>,
    ) -> PlatformResult<()> {
        let payload = &envelope.payload;
        if payload.format_version != 1
            || payload.installation_id != installation_id
            || payload.hostname != hostname
            || payload.protection != self.protector.protection_name()
            || payload.key_algorithm != "ECDSA_P256_SHA256"
            || Uuid::parse_str(&payload.generation_id).is_err()
            || payload.server_not_before > now + ChronoDuration::minutes(CLOCK_SKEW_MINUTES)
            || payload.server_not_after > payload.ca_not_after
        {
            return Err(PlatformError::security());
        }

        let ca_certificate = decode_bounded(&payload.ca_certificate_der_base64, 32 * 1024)?;
        let fingerprint = certificate_fingerprint(&ca_certificate);
        if !constant_time_ascii_equal(&fingerprint, &payload.ca_fingerprint_sha256) {
            return Err(PlatformError::security());
        }
        let server_certificate = decode_bounded(&payload.server_certificate_der_base64, 32 * 1024)?;
        if server_certificate.is_empty() {
            return Err(PlatformError::security());
        }

        let protected_ca = decode_bounded(&payload.protected_ca_private_key_base64, 64 * 1024)?;
        let ca_private = self
            .protector
            .unprotect(&protected_ca)
            .map_err(|_| PlatformError::security())?;
        let ca_key =
            KeyPair::try_from(ca_private.as_slice()).map_err(|_| PlatformError::security())?;
        if !constant_time_ascii_equal(
            &hex::encode(Sha256::digest(ca_key.subject_public_key_info())),
            &payload.ca_public_key_sha256,
        ) {
            return Err(PlatformError::security());
        }

        let protected_server =
            decode_bounded(&payload.protected_server_private_key_base64, 64 * 1024)?;
        let server_private = self
            .protector
            .unprotect(&protected_server)
            .map_err(|_| PlatformError::security())?;
        let server_key =
            KeyPair::try_from(server_private.as_slice()).map_err(|_| PlatformError::security())?;
        if !constant_time_ascii_equal(
            &hex::encode(Sha256::digest(server_key.subject_public_key_info())),
            &payload.server_public_key_sha256,
        ) {
            return Err(PlatformError::security());
        }

        verify_payload_tag(envelope, ca_private.as_slice())
    }

    fn load_runtime_identity(
        &self,
        envelope: TlsEnvelope,
        action: TlsIdentityAction,
        installation_id: &str,
        hostname: &str,
        now: DateTime<Utc>,
    ) -> PlatformResult<LoadedTlsIdentity> {
        self.verify_envelope(&envelope, installation_id, hostname, now)?;
        let ca_certificate_der =
            decode_bounded(&envelope.payload.ca_certificate_der_base64, 32 * 1024)?;
        let server_certificate_der =
            decode_bounded(&envelope.payload.server_certificate_der_base64, 32 * 1024)?;
        let protected_server = decode_bounded(
            &envelope.payload.protected_server_private_key_base64,
            64 * 1024,
        )?;
        let private_key_der = self
            .protector
            .unprotect(&protected_server)
            .map_err(|_| PlatformError::security())?;

        Ok(LoadedTlsIdentity {
            generation_id: envelope.payload.generation_id,
            hostname: envelope.payload.hostname,
            certificate_chain_der: vec![server_certificate_der, ca_certificate_der.clone()],
            private_key_der,
            ca_certificate_der,
            ca_fingerprint_sha256: envelope.payload.ca_fingerprint_sha256,
            server_not_after: envelope.payload.server_not_after,
            action,
        })
    }

    fn persist_envelope(&self, envelope: &TlsEnvelope) -> PlatformResult<()> {
        let bytes = serde_json::to_vec(envelope).map_err(|_| PlatformError::storage())?;
        if bytes.len() as u64 > MAX_ENVELOPE_BYTES {
            return Err(PlatformError::security());
        }
        let path = self.directory.join(format!(
            "{ENVELOPE_PREFIX}{}.{}",
            envelope.payload.generation_id, ENVELOPE_EXTENSION
        ));
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|_| PlatformError::storage())?;
        temporary
            .write_all(&bytes)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| PlatformError::storage())?;
        let file = temporary
            .persist_noclobber(path)
            .map_err(|_| PlatformError::storage())?;
        file.sync_all().map_err(|_| PlatformError::storage())?;
        sync_directory(&self.directory)
    }

    fn persist_public_ca(&self, envelope: &TlsEnvelope) -> PlatformResult<()> {
        let expected = decode_bounded(&envelope.payload.ca_certificate_der_base64, 32 * 1024)?;
        let path = self.public_ca_path();
        if path.exists() {
            let actual = fs::read(&path).map_err(|_| PlatformError::storage())?;
            if !bool::from(actual.as_slice().ct_eq(expected.as_slice())) {
                return Err(PlatformError::security());
            }
            return Ok(());
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|_| PlatformError::storage())?;
        temporary
            .write_all(&expected)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| PlatformError::storage())?;
        match temporary.persist_noclobber(&path) {
            Ok(file) => {
                file.sync_all().map_err(|_| PlatformError::storage())?;
                sync_directory(&self.directory)
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let actual = fs::read(&path).map_err(|_| PlatformError::storage())?;
                if bool::from(actual.as_slice().ct_eq(expected.as_slice())) {
                    Ok(())
                } else {
                    Err(PlatformError::security())
                }
            }
            Err(_) => Err(PlatformError::storage()),
        }
    }
}

struct GeneratedServerCertificate {
    certificate_der: Vec<u8>,
    private_key_der: Zeroizing<Vec<u8>>,
    public_key_sha256: String,
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
}

fn generate_server_certificate(
    hostname: &str,
    now: DateTime<Utc>,
    ca_not_after: DateTime<Utc>,
    issuer: &Issuer<'_, KeyPair>,
) -> PlatformResult<GeneratedServerCertificate> {
    let not_before = now - ChronoDuration::minutes(CLOCK_SKEW_MINUTES);
    let not_after = std::cmp::min(
        now + ChronoDuration::days(SERVER_VALIDITY_DAYS),
        ca_not_after - ChronoDuration::days(1),
    );
    if not_after <= now + ChronoDuration::days(RENEWAL_WINDOW_DAYS) {
        return Err(PlatformError::new("TLS_CA_REPAIR_REQUIRED"));
    }

    let mut params = CertificateParams::new(vec!["localhost".to_owned(), hostname.to_owned()])
        .map_err(|_| PlatformError::invalid_input())?;
    params.distinguished_name.push(DnType::CommonName, hostname);
    params.not_before = to_offset_date_time(not_before)?;
    params.not_after = to_offset_date_time(not_after)?;
    params.use_authority_key_identifier_extension = true;
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);

    let server_key =
        KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|_| PlatformError::security())?;
    let public_key_sha256 = hex::encode(Sha256::digest(server_key.subject_public_key_info()));
    let private_key_der = Zeroizing::new(server_key.serialize_der());
    let certificate = params
        .signed_by(&server_key, issuer)
        .map_err(|_| PlatformError::security())?;

    Ok(GeneratedServerCertificate {
        certificate_der: certificate.der().to_vec(),
        private_key_der,
        public_key_sha256,
        not_before,
        not_after,
    })
}

fn ca_parameters(
    installation_id: &str,
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
) -> PlatformResult<CertificateParams> {
    let mut params =
        CertificateParams::new(Vec::<String>::new()).map_err(|_| PlatformError::security())?;
    params.distinguished_name.push(
        DnType::CommonName,
        format!("Offline Dental System CA {installation_id}"),
    );
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Offline Dental System");
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    params.not_before =
        to_offset_date_time(not_before - ChronoDuration::minutes(CLOCK_SKEW_MINUTES))?;
    params.not_after = to_offset_date_time(not_after)?;
    Ok(params)
}

fn sign_payload(payload: TlsEnvelopePayload, ca_private_der: &[u8]) -> PlatformResult<TlsEnvelope> {
    let payload_bytes = serde_json::to_vec(&payload).map_err(|_| PlatformError::storage())?;
    let tag = payload_tag(&payload_bytes, ca_private_der)?;
    Ok(TlsEnvelope {
        payload,
        integrity_tag_base64: STANDARD.encode(tag),
    })
}

fn verify_payload_tag(envelope: &TlsEnvelope, ca_private_der: &[u8]) -> PlatformResult<()> {
    let payload_bytes =
        serde_json::to_vec(&envelope.payload).map_err(|_| PlatformError::security())?;
    let expected = payload_tag(&payload_bytes, ca_private_der)?;
    let actual = decode_bounded(&envelope.integrity_tag_base64, 64)?;
    if actual.len() != expected.len() || !bool::from(actual.as_slice().ct_eq(expected.as_slice())) {
        return Err(PlatformError::security());
    }
    Ok(())
}

fn payload_tag(payload: &[u8], ca_private_der: &[u8]) -> PlatformResult<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"offline-dental-tls-envelope-v1"), ca_private_der);
    let mut key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(b"metadata-integrity", key.as_mut())
        .map_err(|_| PlatformError::security())?;
    let mut mac =
        HmacSha256::new_from_slice(key.as_slice()).map_err(|_| PlatformError::security())?;
    mac.update(payload);
    Ok(mac.finalize().into_bytes().into())
}

fn decode_bounded(value: &str, max_bytes: usize) -> PlatformResult<Vec<u8>> {
    if value.len() > max_bytes.saturating_mul(2) {
        return Err(PlatformError::security());
    }
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| PlatformError::security())?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(PlatformError::security());
    }
    Ok(bytes)
}

fn certificate_fingerprint(certificate_der: &[u8]) -> String {
    hex::encode_upper(Sha256::digest(certificate_der))
}

fn constant_time_ascii_equal(left: &str, right: &str) -> bool {
    left.len() == right.len() && bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

fn validate_identity_binding(installation_id: &str, hostname: &str) -> PlatformResult<()> {
    let parsed = Uuid::parse_str(installation_id).map_err(|_| PlatformError::invalid_input())?;
    if hostname != format!("dental-{parsed}.local") {
        return Err(PlatformError::invalid_input());
    }
    Ok(())
}

fn to_offset_date_time(value: DateTime<Utc>) -> PlatformResult<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(value.timestamp())
        .map_err(|_| PlatformError::invalid_input())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use chrono::{Duration, TimeZone, Utc};

    use crate::infrastructure::TestKeyProtector;

    use super::{TlsIdentityAction, TlsIdentityManager};

    #[test]
    fn creates_loads_and_renews_without_exposing_plaintext_keys_on_disk() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manager =
            TlsIdentityManager::new(directory.path(), TestKeyProtector).expect("identity manager");
        let installation_id = "018f0f7d-82ab-7d6e-b234-0123456789ab";
        let hostname = format!("dental-{installation_id}.local");
        let now = Utc
            .with_ymd_and_hms(2026, 7, 22, 12, 0, 0)
            .single()
            .expect("fixed time");

        let created = manager
            .ensure_identity(installation_id, &hostname, now)
            .expect("create identity");
        assert_eq!(created.action, TlsIdentityAction::Created);
        assert_eq!(created.certificate_chain_der.len(), 2);
        assert_eq!(created.private_key_der[0], 0x30);
        assert_eq!(created.ca_fingerprint_sha256.len(), 64);

        let loaded = manager
            .ensure_identity(installation_id, &hostname, now + Duration::days(1))
            .expect("load identity");
        assert_eq!(loaded.action, TlsIdentityAction::Unchanged);
        assert_eq!(loaded.generation_id, created.generation_id);

        let renewed = manager
            .ensure_identity(
                installation_id,
                &hostname,
                created.server_not_after - Duration::days(20),
            )
            .expect("renew identity");
        assert_eq!(renewed.action, TlsIdentityAction::Renewed);
        assert_ne!(renewed.generation_id, created.generation_id);
        assert_eq!(renewed.ca_certificate_der, created.ca_certificate_der);

        for entry in fs::read_dir(directory.path().join("tls")).expect("list TLS files") {
            let path = entry.expect("directory entry").path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                let text = fs::read_to_string(path).expect("read envelope");
                assert!(!text.contains(&STANDARD.encode(created.private_key_der.as_slice())));
            }
        }
    }

    #[test]
    fn metadata_tampering_is_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manager =
            TlsIdentityManager::new(directory.path(), TestKeyProtector).expect("identity manager");
        let installation_id = "018f0f7d-82ab-7d6e-b234-0123456789ab";
        let hostname = format!("dental-{installation_id}.local");
        let now = Utc::now();
        manager
            .ensure_identity(installation_id, &hostname, now)
            .expect("create identity");

        let envelope_path = fs::read_dir(directory.path().join("tls"))
            .expect("list files")
            .map(|entry| entry.expect("entry").path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .expect("envelope path");
        let text = fs::read_to_string(&envelope_path).expect("read envelope");
        fs::write(
            &envelope_path,
            text.replace("ECDSA_P256_SHA256", "ECDSA_P256_SHA384"),
        )
        .expect("tamper envelope");

        assert!(
            manager
                .ensure_identity(installation_id, &hostname, now + Duration::minutes(1))
                .is_err()
        );
    }

    #[test]
    fn truncated_generation_is_rejected_without_being_overwritten() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let manager =
            TlsIdentityManager::new(directory.path(), TestKeyProtector).expect("identity manager");
        let installation_id = "018f0f7d-82ab-7d6e-b234-0123456789ab";
        let hostname = format!("dental-{installation_id}.local");
        let now = Utc::now();
        manager
            .ensure_identity(installation_id, &hostname, now)
            .expect("create identity");
        let truncated = directory
            .path()
            .join("tls")
            .join("identity-v1-018f0f7d-82ab-7d6e-b234-012345678900.json");
        fs::write(&truncated, b"{\"payload\":").expect("write interrupted generation fixture");

        assert!(
            manager
                .ensure_identity(installation_id, &hostname, now + Duration::minutes(1))
                .is_err()
        );
        assert_eq!(
            fs::read(&truncated).expect("read fixture"),
            b"{\"payload\":"
        );
    }
}
