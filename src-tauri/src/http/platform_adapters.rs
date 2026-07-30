use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    domain::{AppError, AppResult, StorageInput},
    platform::{
        PlatformError,
        pairing::PairingManager,
        storage_volumes::{LocalVolumeProvider, VolumeKind},
    },
};

use super::{
    PairingCompleteResponse, PairingPort, PairingStartResponse, StorageVolume, StorageVolumePort,
};

pub struct PlatformStorageVolumeAdapter {
    data_directory: PathBuf,
}

impl PlatformStorageVolumeAdapter {
    pub fn new(data_directory: PathBuf) -> Self {
        Self { data_directory }
    }
}

impl StorageVolumePort for PlatformStorageVolumeAdapter {
    fn enumerate(&self) -> AppResult<Vec<StorageVolume>> {
        LocalVolumeProvider::enumerate(&self.data_directory)
            .map(|volumes| {
                volumes
                    .into_iter()
                    .map(|volume| StorageVolume {
                        id: volume.id,
                        root_path: volume.root_path,
                        label: volume.label,
                        file_system: volume.file_system,
                        available_bytes: volume.available_bytes,
                        kind: match volume.kind {
                            VolumeKind::Fixed => "fixed",
                            VolumeKind::Removable => "removable",
                        }
                        .to_owned(),
                        writable: volume.writable,
                        destination_path: volume.destination_path,
                    })
                    .collect()
            })
            .map_err(map_platform_error)
    }

    fn resolve_artifact_directories(
        &self,
        backup_volume_id: &str,
        recovery_volume_id: &str,
        active_directory: &Path,
    ) -> AppResult<StorageInput> {
        if active_directory != self.data_directory.join("active") {
            return Err(AppError::security());
        }
        let locations = LocalVolumeProvider::resolve_artifact_locations(
            backup_volume_id,
            recovery_volume_id,
            &self.data_directory,
        )
        .and_then(|locations| {
            LocalVolumeProvider::prepare_artifact_locations(&locations, &self.data_directory)
        })
        .map_err(map_platform_error)?;
        Ok(StorageInput {
            backup_directory: locations.backup_directory.to_string_lossy().into_owned(),
            recovery_package_directory: locations.recovery_directory.to_string_lossy().into_owned(),
        })
    }
}

pub struct PlatformPairingAdapter {
    manager: Arc<PairingManager>,
    identity: RwLock<Option<PairingIdentity>>,
    operational: AtomicBool,
}

#[derive(Clone)]
struct PairingIdentity {
    hostname: String,
    fingerprint_sha256: String,
    ca_certificate_der: Vec<u8>,
}

impl PlatformPairingAdapter {
    pub fn new(manager: Arc<PairingManager>) -> Self {
        Self {
            manager,
            identity: RwLock::new(None),
            operational: AtomicBool::new(false),
        }
    }

    pub fn mark_operational(&self) {
        self.operational.store(true, Ordering::Release);
    }

    pub fn mark_unavailable(&self) {
        self.operational.store(false, Ordering::Release);
        let _ = self.manager.cancel();
    }

    pub fn update_identity(
        &self,
        hostname: String,
        fingerprint_sha256: String,
        ca_certificate_der: Vec<u8>,
    ) -> AppResult<()> {
        if ca_certificate_der.is_empty() || ca_certificate_der.len() > 128 * 1024 {
            return Err(AppError::security());
        }
        let identity = PairingIdentity {
            hostname,
            fingerprint_sha256,
            ca_certificate_der,
        };
        *self.identity.write().map_err(|_| AppError::worker())? = Some(identity);
        Ok(())
    }

    fn identity(&self) -> AppResult<PairingIdentity> {
        self.identity
            .read()
            .map_err(|_| AppError::worker())?
            .clone()
            .ok_or_else(|| {
                AppError::new(
                    "PAIRING_UNAVAILABLE",
                    "O pareamento não está disponível neste momento.",
                )
            })
    }

    fn ensure_operational(&self) -> AppResult<()> {
        if self.operational.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(pairing_unavailable())
        }
    }
}

impl PairingPort for PlatformPairingAdapter {
    fn start(&self) -> AppResult<PairingStartResponse> {
        self.ensure_operational()?;
        let identity = self.identity()?;
        let challenge = self
            .manager
            .start(&identity.hostname, &identity.fingerprint_sha256)
            .map_err(map_platform_error)?;
        if self.ensure_operational().is_err() {
            let _ = self.manager.cancel();
            return Err(pairing_unavailable());
        }
        Ok(PairingStartResponse {
            token: challenge.token.clone(),
            pairing_url: challenge.qr_payload.clone(),
            fingerprint_sha256: challenge.ca_fingerprint_sha256.clone(),
            expires_at: challenge.expires_at.to_rfc3339(),
        })
    }

    fn complete(&self, token: &str) -> AppResult<PairingCompleteResponse> {
        self.ensure_operational()?;
        let identity = self.identity()?;
        let grant = self
            .manager
            .consume(token)
            .map_err(map_pairing_completion_error)?;
        if grant.hostname != identity.hostname
            || grant.ca_fingerprint_sha256 != identity.fingerprint_sha256
        {
            return Err(AppError::security());
        }
        Ok(PairingCompleteResponse {
            fingerprint_sha256: grant.ca_fingerprint_sha256,
            ca_certificate_der_base64: STANDARD.encode(&identity.ca_certificate_der),
            ca_file_name: "offline-dental-system-ca.cer".to_owned(),
            server_url: format!("https://{}:8743", grant.hostname),
        })
    }
}

fn pairing_unavailable() -> AppError {
    AppError::new(
        "PAIRING_UNAVAILABLE",
        "O pareamento não está disponível neste momento.",
    )
}

fn map_pairing_completion_error(error: PlatformError) -> AppError {
    match error.code() {
        "PLATFORM_SECURITY_FAILURE" | "PLATFORM_INVALID_INPUT" => AppError::new(
            "PAIRING_TOKEN_INVALID",
            "O código de pareamento é inválido, expirou ou já foi utilizado.",
        ),
        "PLATFORM_UNAVAILABLE" => pairing_unavailable(),
        _ => AppError::worker(),
    }
}

fn map_platform_error(error: PlatformError) -> AppError {
    match error.code() {
        "PLATFORM_INVALID_INPUT" => AppError::validation(vec![crate::domain::FieldError {
            field: "storage".to_owned(),
            message: "Selecione volumes locais, graváveis e diferentes.".to_owned(),
        }]),
        "PLATFORM_UNAVAILABLE" => AppError::new(
            "PLATFORM_STORAGE_UNAVAILABLE",
            "O armazenamento local não está disponível.",
        ),
        "PLATFORM_STORAGE_FAILURE" => AppError::storage(),
        _ => AppError::security(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_challenges_exist_only_while_the_lan_listener_is_operational() {
        let adapter = PlatformPairingAdapter::new(Arc::new(PairingManager::new()));
        adapter
            .update_identity(
                "dental-018f0f7d-82ab-7d6e-b234-0123456789ab.local".to_owned(),
                "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF".to_owned(),
                vec![1, 2, 3],
            )
            .expect("identity");
        assert_eq!(
            adapter.start().err().expect("LAN unavailable").code,
            "PAIRING_UNAVAILABLE"
        );

        adapter.mark_operational();
        let challenge = adapter.start().expect("pairing challenge");
        adapter.mark_unavailable();
        assert_eq!(
            adapter
                .complete(&challenge.token)
                .err()
                .expect("unavailable completion")
                .code,
            "PAIRING_UNAVAILABLE"
        );

        adapter.mark_operational();
        assert_eq!(
            adapter
                .complete(&challenge.token)
                .err()
                .expect("cancelled challenge")
                .code,
            "PAIRING_TOKEN_INVALID"
        );
    }
}
